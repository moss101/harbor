//! OPC package integrity: the structural checks an Office application
//! makes before it will open a file without offering to "repair" it.
//!
//! Harbor writes new packages by hand (decks, documents) or through a
//! backend (workbooks). A dangling relationship or a part with no content
//! type is invisible to Harbor's own readers — they look parts up by name
//! — and fatal to PowerPoint, Word and Excel. These checks run on every
//! created artifact before it is proposed, so a structural defect is a
//! refusal Harbor reports, not a repair dialog the user meets.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

/// Every problem found, empty when the package is sound:
///
/// - each XML part (and every `.rels`) is well-formed;
/// - each internal relationship target resolves to a part in the package;
/// - each part has a content type (an `Override`, or a `Default` for its
///   extension), and each `Override` names a part that exists;
/// - the package has a root `_rels/.rels` with an officeDocument target.
pub const MAX_PACKAGE_UNCOMPRESSED: u64 = 512 * 1024 * 1024;

pub fn package_integrity(bytes: &[u8]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut total_uncompressed: u64 = 0;
    let mut archive = match zip::ZipArchive::new(Cursor::new(bytes)) {
        Ok(a) => a,
        Err(e) => return vec![format!("not a zip package: {e}")],
    };
    let mut parts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(e) => {
                problems.push(format!("corrupt entry {i}: {e}"));
                continue;
            }
        };
        let name = entry.name().to_string();
        // SEC-003 (OOXML path traversal): entry names are canonicalized
        // part paths — absolute or climbing names are rejected outright
        // (Harbor never extracts to the filesystem, but a package whose
        // names claim to escape is malformed at best, hostile at worst).
        if name.starts_with('/') || name.split('/').any(|seg| seg == "..") {
            problems.push(format!("{name}: traversal entry name rejected (SEC-003)"));
            continue;
        }
        // SEC-002 (ZIP bomb): per-entry compression ratio and total
        // uncompressed ceilings. A part that expands more than 200x its
        // stored bytes, or a package over 512 MiB uncompressed, is
        // refused before any parsing.
        let stored = entry.compressed_size().max(1);
        let mut buf = Vec::new();
        if let Err(e) = entry.read_to_end(&mut buf) {
            problems.push(format!("{name}: {e}"));
            continue;
        }
        if buf.len() as u64 > stored.saturating_mul(200) {
            problems.push(format!(
                "{name}: compression ratio {} exceeds 200x (SEC-002)",
                buf.len() as u64 / stored
            ));
            continue;
        }
        total_uncompressed += buf.len() as u64;
        if total_uncompressed > MAX_PACKAGE_UNCOMPRESSED {
            problems.push(format!(
                "package exceeds {MAX_PACKAGE_UNCOMPRESSED} uncompressed bytes (SEC-002)"
            ));
            break;
        }
        if parts.insert(name.clone(), buf).is_some() {
            problems.push(format!("duplicate part {name}"));
        }
    }
    let names: BTreeSet<&str> = parts.keys().map(|s| s.as_str()).collect();

    // Well-formed XML.
    for (name, body) in &parts {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".xml") || lower.ends_with(".rels") {
            match std::str::from_utf8(body) {
                Ok(text) => {
                    if let Err(e) = roxmltree::Document::parse(text) {
                        problems.push(format!("{name}: malformed XML: {e}"));
                    }
                }
                Err(_) => problems.push(format!("{name}: not UTF-8")),
            }
        }
    }

    // Content types.
    let Some(ct_bytes) = parts.get("[Content_Types].xml") else {
        problems.push("missing [Content_Types].xml".into());
        return problems;
    };
    let ct_text = String::from_utf8_lossy(ct_bytes).to_string();
    let mut defaults: BTreeSet<String> = BTreeSet::new();
    let mut overrides: BTreeSet<String> = BTreeSet::new();
    if let Ok(ct) = roxmltree::Document::parse(&ct_text) {
        for n in ct.descendants() {
            match n.tag_name().name() {
                "Default" => {
                    if let Some(ext) = n.attribute("Extension") {
                        defaults.insert(ext.to_ascii_lowercase());
                    }
                }
                "Override" => {
                    if let Some(p) = n.attribute("PartName") {
                        overrides.insert(p.trim_start_matches('/').to_string());
                    }
                }
                _ => {}
            }
        }
    }
    for o in &overrides {
        if !names.contains(o.as_str()) {
            problems.push(format!("content type override for missing part /{o}"));
        }
    }
    for name in &names {
        if *name == "[Content_Types].xml" || name.ends_with('/') {
            continue;
        }
        let ext = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !overrides.contains(*name) && !defaults.contains(&ext) {
            problems.push(format!("part /{name} has no content type"));
        }
    }

    // Relationships.
    if !names.contains("_rels/.rels") {
        problems.push("missing root relationships _rels/.rels".into());
    }
    let mut has_office_document = false;
    for (rels_name, body) in parts.iter().filter(|(n, _)| n.ends_with(".rels")) {
        let Ok(text) = std::str::from_utf8(body) else {
            continue;
        };
        let Ok(doc) = roxmltree::Document::parse(text) else {
            continue;
        };
        // `word/_rels/document.xml.rels` → targets resolve against `word`.
        let source_dir = rels_name
            .rsplit_once("_rels/")
            .map(|(dir, _)| dir.trim_end_matches('/').to_string())
            .unwrap_or_default();
        let mut ids = BTreeSet::new();
        for rel in doc
            .descendants()
            .filter(|n| n.tag_name().name() == "Relationship")
        {
            let id = rel.attribute("Id").unwrap_or_default();
            if !ids.insert(id.to_string()) {
                problems.push(format!("{rels_name}: duplicate relationship id {id}"));
            }
            if rel
                .attribute("Type")
                .map(|t| t.ends_with("/officeDocument"))
                .unwrap_or(false)
                && rels_name == "_rels/.rels"
            {
                has_office_document = true;
            }
            if rel.attribute("TargetMode") == Some("External") {
                continue;
            }
            let Some(target) = rel.attribute("Target") else {
                problems.push(format!("{rels_name}: relationship {id} has no target"));
                continue;
            };
            let resolved = if let Some(abs) = target.strip_prefix('/') {
                abs.to_string()
            } else {
                resolve(&source_dir, target)
            };
            if !names.contains(resolved.as_str()) {
                problems.push(format!(
                    "{rels_name}: relationship {id} targets missing part {resolved}"
                ));
            }
        }
    }
    if !has_office_document {
        problems.push("_rels/.rels declares no officeDocument relationship".into());
    }
    problems
}

fn resolve(source_dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if source_dir.is_empty() {
        Vec::new()
    } else {
        source_dir.split('/').collect()
    };
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn package(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in parts {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    const CT: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/></Types>"#;
    const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="doc/main.xml"/></Relationships>"#;

    #[test]
    fn a_sound_package_has_no_problems() {
        let bytes = package(&[
            ("[Content_Types].xml", CT),
            ("_rels/.rels", ROOT),
            ("doc/main.xml", "<main/>"),
        ]);
        assert!(package_integrity(&bytes).is_empty());
    }

    #[test]
    fn dangling_targets_missing_types_and_bad_xml_are_reported() {
        let bytes = package(&[
            ("[Content_Types].xml", CT),
            ("_rels/.rels", ROOT),
            ("doc/main.xml", "<main>"),
            (
                "doc/_rels/main.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="x" Target="../media/chart{n}.xml"/></Relationships>"#,
            ),
            ("media/picture.png", "png"),
        ]);
        let problems = package_integrity(&bytes);
        assert!(
            problems.iter().any(|p| p.contains("malformed XML")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.contains("media/chart{n}.xml")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.contains("/media/picture.png")),
            "{problems:?}"
        );
        assert!(!package_integrity(b"not a zip").is_empty());
    }
}
