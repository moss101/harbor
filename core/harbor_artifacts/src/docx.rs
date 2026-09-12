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
    /// Set a table cell's first-paragraph text (cell.set). The cell is
    /// addressed merge-aware via the table/row/col map; the cell's OTHER
    /// paragraphs are preserved.
    TableCellSet { table: usize, row: usize, col: usize, new_text: String },
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
    /// package. Every op carries an implicit precondition: the target
    /// paragraph's current text must hash to the recorded content hash.
    pub fn apply(&self, bytes: &[u8], ops: &[DocxOp]) -> Result<Vec<u8>, DocxError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut xml = String::new();
        {
            let mut f = archive
                .by_name("word/document.xml")
                .map_err(|_| DocxError::Malformed("missing word/document.xml".into()))?;
            f.read_to_string(&mut xml)?;
        }

        // Normalize every op into (paragraph ordinal 1-based, new text),
        // validating preconditions against the loaded document state.
        let doc = roxmltree::Document::parse(&xml).map_err(|e| DocxError::Malformed(e.to_string()))?;
        let para_nodes: Vec<roxmltree::Node<'_, '_>> = doc
            .descendants()
            .filter(|n| n.has_tag_name("p"))
            .collect();
        let mut replacement: BTreeMap<usize, String> = BTreeMap::new();
        for op in ops {
            let (index, new_text): (u32, String) = match op {
                DocxOp::TextReplace { index, new_text } => (*index, new_text.clone()),
                DocxOp::TableCellSet { table, row, col, new_text } => {
                    let map = table_cell_paragraph_map(&xml)?;
                    let cell = map
                        .get(table)
                        .and_then(|t| t.get(&(*row, *col)))
                        .ok_or(DocxError::ParagraphChanged(0))?;
                    let first = cell
                        .0
                        .first()
                        .copied()
                        .ok_or(DocxError::ParagraphChanged(((*row).max(1)) as u32))?;
                    ((first + 1) as u32, new_text.clone())
                }
            };
            let Some(node) = para_nodes.get((index as usize).saturating_sub(1)) else {
                return Err(DocxError::ParagraphChanged(index));
            };
            let mut text = String::new();
            for t in node.descendants().filter(|n| n.has_tag_name("t")) {
                text.push_str(t.text().unwrap_or_default());
            }
            let hash = harbor_canonical::sha256_hex(text.as_bytes());
            let expected = self
                .paragraphs
                .iter()
                .find(|p| p.index == index)
                .ok_or(DocxError::ParagraphChanged(index))?;
            if expected.content_hash != hash {
                return Err(DocxError::ParagraphChanged(index));
            }
            replacement.insert((index as usize) - 1, new_text.clone());
        }

        // Serialize replacements by rewriting runs of target paragraphs:
        // keep the first run's properties, and distribute the new text
        // across runs when lengths align (formatting preservation).
        let new_xml = rewrite_paragraph_texts(&xml, &replacement)?;

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

/// Same-grapheme-length replacements are distributed across the existing
/// runs at their original character spans, PRESERVING per-run formatting
/// (bold/italic spans survive). Different-length replacements keep the
/// first run's formatting for the whole new text (documented behavior).
fn replace_para_text(para: &str, new_text: &str) -> Result<String, DocxError> {
    use unicode_segmentation::UnicodeSegmentation;

    let escaped = new_text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let new_graphemes: Vec<&str> = new_text.graphemes(true).collect();

    // Collect (content_start, content_end, grapheme_len) for every w:t.
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    let mut i = 0usize;
    while let Some(off) = find_wt(&para[i..]) {
        let open = i + off;
        let content_start = para[open..].find('>').ok_or_else(|| {
            DocxError::Malformed("w:t open".into())
        })? + open + 1;
        let content_end = para[content_start..]
            .find("</w:t>")
            .ok_or_else(|| DocxError::Malformed("w:t close".into()))?
            + content_start;
        let raw = &para[content_start..content_end];
        let unescaped = raw
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&");
        let gcount = unescaped.graphemes(true).count();
        spans.push((content_start, content_end, gcount));
        i = content_end + 6;
    }
    if spans.is_empty() {
        return Err(DocxError::Malformed("paragraph has no w:t".into()));
    }
    let total: usize = spans.iter().map(|(_, _, g)| *g).sum();
    let new_len = new_text.graphemes(true).count();

    let mut out = String::with_capacity(para.len() + escaped.len());
    let mut last = 0usize;
    let mut grapheme_pos = 0usize;
    for (idx, (cs, ce, gcount)) in spans.iter().enumerate() {
        out.push_str(&para[last..*cs]);
        let slice: String = if total == new_len {
            // Formatting-preserving distribution.
            new_graphemes
                .iter()
                .skip(grapheme_pos)
                .take(*gcount)
                .map(|g| g.to_string())
                .collect::<Vec<String>>()
                .concat()
        } else if idx == 0 {
            escaped.clone()
        } else {
            String::new()
        };
        out.push_str(&slice);
        grapheme_pos += slice.graphemes(true).count();
        last = *ce;
    }
    out.push_str(&para[last..]);
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

/// Locate tables in raw document XML using the GLOBAL paragraph ordinal
/// space (the same indexes DocxOp::TextReplace targets). Returns, per
/// table (document order), a map of (row, col) -> (paragraph indexes,
/// gridSpan). Column accounting honors gridSpan so merged cells occupy
/// their full width.
pub fn table_cell_paragraph_map(
    xml: &str,
) -> Result<BTreeMap<usize, BTreeMap<(usize, usize), (Vec<usize>, u32)>>, DocxError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| DocxError::Malformed(e.to_string()))?;

    let tbl_ids: Vec<roxmltree::NodeId> = doc
        .descendants()
        .filter(|n| n.has_tag_name("tbl"))
        .map(|n| n.id())
        .collect();

    let mut result: BTreeMap<usize, BTreeMap<(usize, usize), (Vec<usize>, u32)>> = BTreeMap::new();
    let mut global_para = 0usize;
    for node in doc.descendants().filter(|n| n.has_tag_name("p")) {
        // The global ordinal counts EVERY paragraph in document order —
        // the same space DocxDocument::paragraphs and TextReplace target.
        let this_para = global_para;
        global_para += 1;
        let mut cell_info: Option<(usize, usize, usize, u32)> = None;
        let mut anc = node.parent();
        while let Some(a) = anc {
            if a.has_tag_name("tc") {
                // Walk UP from the cell to find its table ordinal + row
                // ordinal, honoring the tc element itself.
                let tc = a;
                let mut table_idx = None;
                let mut row_idx = None;
                let mut cur = tc.parent();
                while let Some(x) = cur {
                    if x.has_tag_name("tr") && row_idx.is_none() {
                        let rows: Vec<roxmltree::NodeId> = x
                            .parent()
                            .map(|p| {
                                p.children()
                                    .filter(|c| c.has_tag_name("tr"))
                                    .map(|c| c.id())
                                    .collect()
                            })
                            .unwrap_or_default();
                        row_idx = rows.iter().position(|id| *id == x.id());
                    }
                    if x.has_tag_name("tbl") {
                        table_idx = tbl_ids.iter().position(|id| *id == x.id());
                        break;
                    }
                    cur = x.parent();
                }
                if let (Some(t), Some(r)) = (table_idx, row_idx) {
                    let row_node = tc.parent().unwrap();
                    let mut c = 0usize;
                    for tc_sibling in row_node.children().filter(|n| n.has_tag_name("tc")) {
                        if tc_sibling.id() == tc.id() {
                            break;
                        }
                        let span = tc_sibling
                            .descendants()
                            .find(|n| n.has_tag_name("gridSpan"))
                            .and_then(|g| g.attribute("val"))
                            .and_then(|v| v.parse::<u32>().ok())
                            .unwrap_or(1);
                        c += span as usize;
                    }
                    let grid_span = tc
                        .descendants()
                        .find(|n| n.has_tag_name("gridSpan"))
                        .and_then(|g| g.attribute("val"))
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(1);
                    cell_info = Some((t, r, c, grid_span));
                }
                break;
            }
            anc = a.parent();
        }
        match cell_info {
            Some((t, r, c, span)) => {
                result
                    .entry(t)
                    .or_default()
                    .entry((r, c))
                    .or_insert_with(|| (Vec::new(), span))
                    .0
                    .push(this_para);
            }
            None => {}
        }
    }
    Ok(result)
}
