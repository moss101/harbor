//! DOCX document: paragraph extraction and typed text operations over the
//! OOXML package (word/document.xml). Supported GA scope per the Office
//! Feature Matrix: paragraphs, runs, headings, lists, tables (read),
//! inline text replacement. Floating drawings/fields are PRESERVE_ONLY —
//! unknown parts are passed through untouched.

use std::io::{Cursor, Read, Write};
use std::collections::BTreeMap;

use harbor_canonical::JsonValue;

#[derive(Debug, Clone, PartialEq)]
pub struct DocxParagraph {
    /// Stable paragraph ordinal in document order (1-based).
    pub index: u32,
    pub style: Option<String>,
    pub text: String,
    /// Content hash of this paragraph's text (precondition binding).
    pub content_hash: String,
}

#[derive(Debug, Clone, Default)]
pub struct DocxDocument {
    pub paragraphs: Vec<DocxParagraph>,
    /// Non-rendered part names preserved verbatim (compatibility report).
    pub preserved_parts: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("malformed docx: {0}")]
    Malformed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("paragraph {0} changed since read")]
    ParagraphChanged(u32),
}

pub enum DocxOp {
    /// Replace the entire text of paragraph `index` (text.replace).
    TextReplace { index: u32, new_text: String },
}

fn para_style(p: roxmltree::Node) -> Option<String> {
    for n in p.descendants() {
        if n.has_tag_name("pStyle") {
            return n.attribute("val").map(|s| s.to_string());
        }
    }
    None
}

/// Extract paragraphs from word/document.xml with proper w:p scoping.
fn extract_paragraphs(xml: &str) -> Result<Vec<(Option<String>, String)>, DocxError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| DocxError::Malformed(e.to_string()))?;
    let mut out = Vec::new();
    for p in doc.descendants().filter(|n| n.has_tag_name("p")) {
        let mut text = String::new();
        for t in p.descendants().filter(|n| n.has_tag_name("t")) {
            text.push_str(t.text().unwrap_or_default());
        }
        for br in p.descendants().filter(|n| n.has_tag_name("tab")) {
            let _ = br;
            text.push('\t');
        }
        out.push((para_style(p), text));
    }
    Ok(out)
}

impl DocxDocument {
    pub fn load(bytes: &[u8]) -> Result<Self, DocxError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut xml = String::new();
        {
            let mut f = archive
                .by_name("word/document.xml")
                .map_err(|_| DocxError::Malformed("missing word/document.xml".into()))?;
            f.read_to_string(&mut xml)?;
        }
        let mut preserved_parts = Vec::new();
        for i in 0..archive.len() {
            let name = archive.by_index(i)?.name().to_string();
            // Parts we do not interpret are preserved (reportable).
            if name.starts_with("word/embeddings/")
                || name.starts_with("word/activeX/")
                || name.ends_with("vbaProject.bin")
            {
                preserved_parts.push(name);
            }
        }
        let mut paragraphs = Vec::new();
        for (i, (style, text)) in extract_paragraphs(&xml)?.into_iter().enumerate() {
            paragraphs.push(DocxParagraph {
                index: (i + 1) as u32,
                style,
                content_hash: harbor_canonical::sha256_hex(text.as_bytes()),
                text,
            });
        }
        Ok(DocxDocument { paragraphs, preserved_parts })
    }

    /// Apply typed ops to the document XML and return the full new DOCX
    /// package. Precondition: paragraph text hash must match.
    pub fn apply(&self, bytes: &[u8], ops: &[DocxOp]) -> Result<Vec<u8>, DocxError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut xml = String::new();
        {
            let mut f = archive
                .by_name("word/document.xml")
                .map_err(|_| DocxError::Malformed("missing word/document.xml".into()))?;
            f.read_to_string(&mut xml)?;
        }
        // Parse current paragraphs (in document order) to locate replacements.
        let doc = roxmltree::Document::parse(&xml).map_err(|e| DocxError::Malformed(e.to_string()))?;
        let para_nodes: Vec<roxmltree::Node<'_, '_>> = doc
            .descendants()
            .filter(|n| n.has_tag_name("p"))
            .collect();
        let mut replacement: BTreeMap<usize, String> = BTreeMap::new();
        for op in ops {
            let DocxOp::TextReplace { index, new_text } = op;
            let Some(node) = para_nodes.get((*index as usize).saturating_sub(1)) else {
                return Err(DocxError::ParagraphChanged(*index));
            };
            let mut text = String::new();
            for t in node.descendants().filter(|n| n.has_tag_name("t")) {
                text.push_str(t.text().unwrap_or_default());
            }
            let hash = harbor_canonical::sha256_hex(text.as_bytes());
            let expected = self
                .paragraphs
                .iter()
                .find(|p| p.index == *index)
                .ok_or(DocxError::ParagraphChanged(*index))?;
            if expected.content_hash != hash {
                return Err(DocxError::ParagraphChanged(*index));
            }
            replacement.insert((*index as usize) - 1, new_text.clone());
        }
        // Serialize replacements by rewriting runs of target paragraphs:
        // keep the first run's properties, set its w:t to the new text, and
        // drop the remaining runs' text nodes.
        let mut new_xml = rewrite_paragraph_texts(&xml, &replacement)?;
        let _ = &mut new_xml;

        // Rebuild package with replaced document.xml.
        let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for i in 0..archive.len() {
            let mut f = archive.by_index(i)?;
            let name = f.name().to_string();
            out.start_file(name.clone(), opts)?;
            if name == "word/document.xml" {
                out.write_all(new_xml.as_bytes())?;
            } else {
                std::io::copy(&mut f, &mut out)?;
            }
        }
        Ok(out.finish()?.into_inner())
    }
}

/// Rewrite the text of the n-th (0-based) w:p in the document. Strategy:
/// walk raw XML with a lightweight scanner counting "<w:p " / "<w:p>"
/// starts; within a target paragraph replace the first "<w:t...>...</w:t>"
/// content with the new text and empty all other w:t contents.
fn rewrite_paragraph_texts(xml: &str, replacement: &BTreeMap<usize, String>) -> Result<String, DocxError> {
    // Find paragraph spans (w:p elements, including self-closing).
    let bytes = xml.as_bytes();
    let mut spans: Vec<(usize, usize)> = Vec::new(); // start of <w:p..., end inclusive
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        if bytes[i] == b'<' && xml[i..].starts_with("<w:p ") || xml[i..].starts_with("<w:p>") {
            // Find matching close accounting nesting of w:p inside w:p? DOCX
            // does not nest w:p (except in txbxContent). We treat the first
            // matching </w:p> that is not inside a nested <w:p.
            let mut depth = 1;
            let mut j = i + 4;
            let mut end = None;
            while j + 5 <= bytes.len() {
                if xml[j..].starts_with("<w:p ") || xml[j..].starts_with("<w:p>") {
                    depth += 1;
                    j += 4;
                } else if xml[j..].starts_with("</w:p>") {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(j + 6);
                        break;
                    }
                    j += 6;
                } else {
                    j += 1;
                }
            }
            if let Some(end) = end {
                spans.push((i, end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    let mut out = String::with_capacity(xml.len());
    let mut last = 0usize;
    for (idx, (start, end)) in spans.iter().enumerate() {
        if let Some(new_text) = replacement.get(&idx) {
            out.push_str(&xml[last..*start]);
            let para = &xml[*start..*end];
            out.push_str(&replace_para_text(para, new_text)?);
            last = *end;
        } else {
            out.push_str(&xml[last..*end]);
            last = *end;
        }
    }
    out.push_str(&xml[last..]);
    Ok(out)
}

fn replace_para_text(para: &str, new_text: &str) -> Result<String, DocxError> {
    // Replace contents of the FIRST w:t and blank all following w:t in this
    // paragraph. XML-escape the new text.
    let escaped = new_text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let mut out = String::with_capacity(para.len() + escaped.len());
    let mut rest = para;
    let mut first_done = false;
    while let Some(open_idx) = find_wt(rest) {
        let (pre, after_open) = split_at_idx(rest, open_idx);
        let content_start = after_open.find('>').ok_or_else(|| DocxError::Malformed("w:t open".into()))? + 1;
        let close_idx = after_open[content_start..]
            .find("</w:t>")
            .ok_or_else(|| DocxError::Malformed("w:t close".into()))?
            + content_start;
        out.push_str(pre);
        if !first_done {
            out.push_str(&after_open[..content_start]);
            out.push_str(&escaped);
            out.push_str("</w:t>");
            first_done = true;
        } else {
            // blank additional text nodes
            out.push_str(&after_open[..content_start]);
            out.push_str("</w:t>");
        }
        rest = &after_open[close_idx + 6..];
    }
    out.push_str(rest);
    Ok(out)
}

fn find_wt(s: &str) -> Option<usize> {
    for (i, b) in s.bytes().enumerate() {
        if b == b'<' && (s[i..].starts_with("<w:t>") || s[i..].starts_with("<w:t ")) {
            return Some(i);
        }
    }
    None
}

fn split_at_idx<'a>(s: &'a str, idx: usize) -> (&'a str, &'a str) {
    (&s[..idx], &s[idx..])
}

impl DocxDocument {
    /// Diff summary entries for ops (Artifact Diff building block).
    pub fn diff_entries(ops: &[DocxOp], paragraphs: &[DocxParagraph]) -> Vec<JsonValue> {
        use harbor_canonical::JsonValue as V;
        ops.iter()
            .map(|op| {
                let DocxOp::TextReplace { index, new_text } = op;
                let before = paragraphs
                    .iter()
                    .find(|p| p.index == *index)
                    .map(|p| p.text.clone())
                    .unwrap_or_default();
                V::object([
                    ("target_id", V::str(format!("paragraph-{index}"))),
                    ("kind", V::str("text.replace")),
                    ("before", V::str(before)),
                    ("after", V::str(new_text.clone())),
                ])
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_docx(paragraphs: &[&str]) -> Vec<u8> {
        // Build a minimal valid DOCX package with N paragraphs.
        let mut paras = String::new();
        for p in paragraphs {
            paras.push_str(&format!(
                "<w:p><w:r><w:t>{}</w:t></w:r></w:p>",
                p.replace('&', "&amp;").replace('<', "&lt;")
            ));
        }
        let document = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{paras}<w:sectPr/></w:body></w:document>"#
        );
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", opts).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#).unwrap();
        zip.start_file("word/document.xml", opts).unwrap();
        zip.write_all(document.as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn read_paragraphs_and_hash() {
        let bytes = minimal_docx(&["Harbor proposal", "Second para"]);
        let doc = DocxDocument::load(&bytes).unwrap();
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(doc.paragraphs[0].text, "Harbor proposal");
        assert_eq!(doc.paragraphs[0].content_hash.len(), 64);
        assert!(doc.paragraphs[1].text == "Second para");
    }

    #[test]
    fn replace_paragraph_text_roundtrip() {
        let bytes = minimal_docx(&["Old title", "Body stays"]);
        let doc = DocxDocument::load(&bytes).unwrap();
        let out = doc
            .apply(
                &bytes,
                &[DocxOp::TextReplace { index: 1, new_text: "New title".into() }],
            )
            .unwrap();
        let reloaded = DocxDocument::load(&out).unwrap();
        assert_eq!(reloaded.paragraphs[0].text, "New title");
        assert_eq!(reloaded.paragraphs[1].text, "Body stays");
    }

    #[test]
    fn precondition_hash_mismatch_rejected() {
        let bytes = minimal_docx(&["v1"]);
        let doc = DocxDocument::load(&bytes).unwrap();
        // Tamper with the paragraph between read and apply.
        let other = minimal_docx(&["v2"]);
        let err = doc.apply(
            &other,
            &[DocxOp::TextReplace { index: 1, new_text: "x".into() }],
        );
        assert!(matches!(err, Err(DocxError::ParagraphChanged(1))));
    }
}
