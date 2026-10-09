//! DOCX document: paragraph extraction and typed text operations over the
//! OOXML package (word/document.xml). Supported GA scope per the Office
//! Feature Matrix: paragraphs, runs, headings, lists, tables (read),
//! inline text replacement. Floating drawings/fields are PRESERVE_ONLY —
//! unknown parts are passed through untouched.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

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
    /// Change a paragraph's style (heading/list/normal). The style must
    /// be one the package's styles.xml defines (create_docx emits
    /// Heading1-3, ListBullet, ListNumber, Compact, Title).
    StyleSet { index: u32, style: String },
    /// Set a table cell's first-paragraph text (cell.set). The cell is
    /// addressed merge-aware via the table/row/col map; the cell's OTHER
    /// paragraphs are preserved.
    TableCellSet {
        table: usize,
        row: usize,
        col: usize,
        new_text: String,
    },
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

/// Merged-cell bookkeeping: table index -> cell span -> (covered cells,
/// gridSpan).
type DocxMergeMap = BTreeMap<usize, BTreeMap<(usize, usize), (Vec<usize>, u32)>>;

impl DocxDocument {
    pub fn load(bytes: &[u8]) -> Result<Self, DocxError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        crate::inflate_probe(&mut archive).map_err(DocxError::Malformed)?;
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
        Ok(DocxDocument {
            paragraphs,
            preserved_parts,
        })
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
        let doc =
            roxmltree::Document::parse(&xml).map_err(|e| DocxError::Malformed(e.to_string()))?;
        let para_nodes: Vec<roxmltree::Node<'_, '_>> =
            doc.descendants().filter(|n| n.has_tag_name("p")).collect();
        let mut replacement: BTreeMap<usize, String> = BTreeMap::new();
        let mut style_set: BTreeMap<usize, String> = BTreeMap::new();
        for op in ops {
            if let DocxOp::StyleSet { index, style } = op {
                // Same precondition as text ops: the paragraph's current
                // text must hash to the loaded state.
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
                if BlockStyle::parse(style).is_none() {
                    return Err(DocxError::Malformed(format!("unknown style {style}")));
                }
                style_set.insert((*index as usize) - 1, style.clone());
                continue;
            }
            let (index, new_text): (u32, String) = match op {
                DocxOp::StyleSet { .. } => unreachable!("handled above"),
                DocxOp::TextReplace { index, new_text } => (*index, new_text.clone()),
                DocxOp::TableCellSet {
                    table,
                    row,
                    col,
                    new_text,
                } => {
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
        let mut new_xml = rewrite_paragraph_texts(&xml, &replacement)?;
        if !style_set.is_empty() {
            new_xml = rewrite_paragraph_styles(&new_xml, &style_set)?;
        }

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
fn rewrite_paragraph_texts(
    xml: &str,
    replacement: &BTreeMap<usize, String>,
) -> Result<String, DocxError> {
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
        let content_start = para[open..]
            .find('>')
            .ok_or_else(|| DocxError::Malformed("w:t open".into()))?
            + open
            + 1;
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

/// Locate tables in raw document XML using the GLOBAL paragraph ordinal
/// space (the same indexes DocxOp::TextReplace targets). Returns, per
/// table (document order), a map of (row, col) -> (paragraph indexes,
/// gridSpan). Column accounting honors gridSpan so merged cells occupy
/// their full width.
pub fn table_cell_paragraph_map(xml: &str) -> Result<DocxMergeMap, DocxError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| DocxError::Malformed(e.to_string()))?;

    let tbl_ids: Vec<roxmltree::NodeId> = doc
        .descendants()
        .filter(|n| n.has_tag_name("tbl"))
        .map(|n| n.id())
        .collect();

    let mut result: DocxMergeMap = BTreeMap::new();
    for (global_para, node) in doc
        .descendants()
        .filter(|n| n.has_tag_name("p"))
        .enumerate()
    {
        // The global ordinal counts EVERY paragraph in document order —
        // the same space DocxDocument::paragraphs and TextReplace target.
        let this_para = global_para;
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
        if let Some((t, r, c, span)) = cell_info {
            result
                .entry(t)
                .or_default()
                .entry((r, c))
                .or_insert_with(|| (Vec::new(), span))
                .0
                .push(this_para);
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Creation: a new document from typed blocks (the creation batch's
// `block.insert` operations). Deterministic — fixed zip timestamps, no
// clock, no generated ids — so the commit path can re-derive the approved
// output hash from the batch alone.

/// Paragraph roles a created document can use. Each maps to a named style
/// in the package's styles.xml, so the result stays editable in Word:
/// changing "Heading 1" restyles every heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStyle {
    Title,
    Heading1,
    Heading2,
    Heading3,
    Paragraph,
    Bullet,
    Numbered,
    /// A paragraph without space after it (address lines, sign-offs).
    Compact,
}

impl BlockStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockStyle::Title => "title",
            BlockStyle::Heading1 => "heading1",
            BlockStyle::Heading2 => "heading2",
            BlockStyle::Heading3 => "heading3",
            BlockStyle::Paragraph => "paragraph",
            BlockStyle::Bullet => "bullet",
            BlockStyle::Numbered => "numbered",
            BlockStyle::Compact => "compact",
        }
    }

    pub fn parse(s: &str) -> Option<BlockStyle> {
        Some(match s {
            "title" => BlockStyle::Title,
            "heading1" => BlockStyle::Heading1,
            "heading2" => BlockStyle::Heading2,
            "heading3" => BlockStyle::Heading3,
            "paragraph" => BlockStyle::Paragraph,
            "bullet" => BlockStyle::Bullet,
            "numbered" => BlockStyle::Numbered,
            "compact" => BlockStyle::Compact,
            _ => return None,
        })
    }

    /// The styles.xml style id (what `DocxDocument::load` reports).
    pub fn style_id(&self) -> Option<&'static str> {
        match self {
            BlockStyle::Title => Some("Title"),
            BlockStyle::Heading1 => Some("Heading1"),
            BlockStyle::Heading2 => Some("Heading2"),
            BlockStyle::Heading3 => Some("Heading3"),
            BlockStyle::Paragraph => None,
            BlockStyle::Bullet => Some("ListBullet"),
            BlockStyle::Numbered => Some("ListNumber"),
            BlockStyle::Compact => Some("Compact"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocxBlock {
    pub style: BlockStyle,
    pub text: String,
}

fn w_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // A line break inside one block is not representable without
            // w:br, which the reader would drop: keep the words apart.
            '\n' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// Arabic-script text is laid out right to left. A block is RTL when most
/// of its letters are Arabic; `docx.inspect` flags Arabic paragraphs that
/// are not, so created documents must never be the thing it flags.
fn is_rtl_text(text: &str) -> bool {
    let (mut arabic, mut latin) = (0usize, 0usize);
    for ch in text.chars() {
        let cp = ch as u32;
        if (0x0600..=0x06FF).contains(&cp)
            || (0x0750..=0x077F).contains(&cp)
            || (0xFB50..=0xFDFF).contains(&cp)
            || (0xFE70..=0xFEFF).contains(&cp)
        {
            arabic += 1;
        } else if ch.is_alphabetic() {
            latin += 1;
        }
    }
    arabic > 0 && arabic >= latin
}

fn block_xml(block: &DocxBlock) -> String {
    let rtl = is_rtl_text(&block.text);
    let mut ppr = String::new();
    if let Some(id) = block.style.style_id() {
        ppr.push_str(&format!("<w:pStyle w:val=\"{id}\"/>"));
    }
    match block.style {
        BlockStyle::Bullet => {
            ppr.push_str("<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>")
        }
        BlockStyle::Numbered => {
            ppr.push_str("<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"2\"/></w:numPr>")
        }
        _ => {}
    }
    if rtl {
        ppr.push_str("<w:bidi/>");
    }
    let rpr = if rtl { "<w:rPr><w:rtl/></w:rPr>" } else { "" };
    let ppr = if ppr.is_empty() {
        String::new()
    } else {
        format!("<w:pPr>{ppr}</w:pPr>")
    };
    format!(
        "<w:p>{ppr}<w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        w_escape(&block.text)
    )
}

const DOCX_STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="Calibri" w:cs="Arial"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-US" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="264" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="240"/></w:pPr><w:rPr><w:b/><w:color w:val="07111D"/><w:sz w:val="52"/><w:szCs w:val="52"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="360" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:color w:val="1F5FCC"/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:color w:val="1F5FCC"/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="200" w:after="60"/><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:b/><w:color w:val="07111D"/><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="ListBullet"><w:name w:val="List Bullet"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="60"/><w:ind w:left="360" w:hanging="360"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="ListNumber"><w:name w:val="List Number"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="60"/><w:ind w:left="360" w:hanging="360"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Compact"><w:name w:val="Compact"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="0"/></w:pPr></w:style>
</w:styles>"#;

const DOCX_NUMBERING: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="singleLevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="&#8226;"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="360" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum>
<w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="singleLevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="360" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum>
<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#;

/// compatibilityMode 15 keeps Word from opening a new document in
/// "Compatibility Mode".
const DOCX_SETTINGS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:defaultTabStop w:val="720"/><w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#;

/// Write a new DOCX package holding `blocks` in order. The page is A4 with
/// one-inch margins; `title` becomes the document's core title property.
pub fn create_docx(title: &str, blocks: &[DocxBlock]) -> Result<Vec<u8>, DocxError> {
    let mut body = String::new();
    for b in blocks {
        body.push_str(&block_xml(b));
    }
    if blocks.is_empty() {
        body.push_str("<w:p/>");
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#;
    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>Harbor</dc:creator></cp:coreProperties>"#,
        w_escape(title)
    );
    let app = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Harbor</Application></Properties>"#;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for (name, body) in [
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", root_rels),
        ("word/document.xml", document.as_str()),
        ("word/_rels/document.xml.rels", doc_rels),
        ("word/styles.xml", DOCX_STYLES),
        ("word/numbering.xml", DOCX_NUMBERING),
        ("word/settings.xml", DOCX_SETTINGS),
        ("docProps/core.xml", core.as_str()),
        ("docProps/app.xml", app),
    ] {
        zip.start_file(name, opts)?;
        zip.write_all(body.as_bytes())?;
    }
    Ok(zip.finish()?.into_inner())
}

/// Set the pStyle of the n-th (0-based) w:p. Replaces an existing
/// w:pStyle val inside w:pPr; inserts a pPr/pStyle right after the w:p
/// open tag when absent. Styles are validated against BlockStyle first.
fn rewrite_paragraph_styles(
    xml: &str,
    styles: &BTreeMap<usize, String>,
) -> Result<String, DocxError> {
    let style_id = |name: &str| {
        BlockStyle::parse(name)
            .and_then(|s| s.style_id())
            .unwrap_or("Normal")
            .to_string()
    };
    let mut out = String::with_capacity(xml.len() + 64 * styles.len());
    let mut paragraph_no = 0usize;
    let mut i = 0usize;
    let bytes = xml.as_bytes();
    while i < bytes.len() {
        // Detect a w:p start (attribute or bare) at this position.
        let is_p = bytes[i..].starts_with(b"<w:p ")
            || bytes[i..].starts_with(b"<w:p>")
            || bytes[i..].starts_with(b"<w:p/>");
        if !is_p {
            // Copy through, fast-forwarding to the next '<'.
            // Search from i+1: a '<' AT i that is not a w:p must be
            // consumed, or the loop never advances (found live: the
            // first test hung forever).
            let next = bytes[i + 1..]
                .iter()
                .position(|b| *b == b'<')
                .map(|p| i + 1 + p);
            match next {
                Some(n) => {
                    out.push_str(&xml[i..n]);
                    i = n;
                }
                None => {
                    out.push_str(&xml[i..]);
                    break;
                }
            }
            continue;
        }
        let start = i;
        let close = xml[i..].find('>').map(|p| i + p + 1).unwrap_or(bytes.len());
        let self_closing = xml[..close].trim_end().ends_with("/>");
        let target = styles.get(&paragraph_no);
        paragraph_no += 1;
        if let Some(style_name) = target {
            let sid = style_id(style_name);
            let open_tag = &xml[start..close];
            if self_closing {
                // <w:p/> → expand with a pPr.
                out.push_str(&format!(
                    "<w:p><w:pPr><w:pStyle w:val=\"{sid}\"/></w:pPr></w:p>"
                ));
                i = close;
                continue;
            }
            let rest_start = close;
            // Find this paragraph's end.
            let end = find_paragraph_end(xml, rest_start)?;
            let body = &xml[rest_start..end];
            let new_body = if let Some(ppr_at) = body.find("<w:pPr>") {
                // Replace existing pStyle val or insert one at pPr start.
                let inner = &body[ppr_at..];
                if let Some(val_at) = inner.find("<w:pStyle ") {
                    let seg = &inner[val_at..];
                    let tag_end = seg.find('/').map(|p| val_at + p + 1).unwrap_or(0);
                    let replaced = format!("<w:pStyle w:val=\"{sid}\"/");
                    format!(
                        "{}{}{}",
                        &body[..ppr_at + val_at],
                        replaced,
                        &body[ppr_at + tag_end..]
                    )
                } else {
                    format!(
                        "{}<w:pPr><w:pStyle w:val=\"{sid}\"/>{}",
                        &body[..ppr_at],
                        &body[ppr_at + "<w:pPr>".len()..]
                    )
                }
            } else {
                format!("<w:pPr><w:pStyle w:val=\"{sid}\"/></w:pPr>{body}")
            };
            out.push_str(open_tag);
            out.push_str(&new_body);
            i = end;
            continue;
        }
        // Not a target: copy the whole element.
        if self_closing {
            out.push_str(&xml[start..close]);
            i = close;
        } else {
            let end = find_paragraph_end(xml, close)?;
            out.push_str(&xml[start..end]);
            i = end;
        }
    }
    Ok(out)
}

/// End index (exclusive) of the w:p whose content starts at `from`:
/// the first </w:p> or a self-closing boundary.
fn find_paragraph_end(xml: &str, from: usize) -> Result<usize, DocxError> {
    xml[from..]
        .find("</w:p>")
        .map(|p| from + p + "</w:p>".len())
        .ok_or_else(|| DocxError::Malformed("unbalanced w:p".into()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn style_set_changes_paragraph_style_round_trip() {
        let blocks = vec![
            DocxBlock {
                style: BlockStyle::Title,
                text: "T".into(),
            },
            DocxBlock {
                style: BlockStyle::Paragraph,
                text: "plain".into(),
            },
        ];
        let bytes = create_docx("T", &blocks).unwrap();
        let doc = DocxDocument::load(&bytes).unwrap();
        let plain_index = doc
            .paragraphs
            .iter()
            .find(|p| p.text == "plain")
            .unwrap()
            .index;

        let out = doc
            .apply(
                &bytes,
                &[DocxOp::StyleSet {
                    index: plain_index,
                    style: "heading2".into(),
                }],
            )
            .unwrap();
        let re = DocxDocument::load(&out).unwrap();
        let changed = re.paragraphs.iter().find(|p| p.text == "plain").unwrap();
        assert_eq!(changed.style.as_deref(), Some("Heading2"));

        // Unknown style is a typed refusal; stale text is refused.
        assert!(doc
            .apply(
                &bytes,
                &[DocxOp::StyleSet {
                    index: plain_index,
                    style: "nope".into()
                }]
            )
            .is_err());
    }

    use super::*;

    #[test]
    fn created_document_is_sound_deterministic_and_reads_back() {
        let blocks = vec![
            DocxBlock {
                style: BlockStyle::Title,
                text: "Archive migration proposal".into(),
            },
            DocxBlock {
                style: BlockStyle::Heading1,
                text: "Summary".into(),
            },
            DocxBlock {
                style: BlockStyle::Paragraph,
                text: "Move the archive to local storage & keep <two> copies.".into(),
            },
            DocxBlock {
                style: BlockStyle::Bullet,
                text: "Two engineers for six weeks".into(),
            },
            DocxBlock {
                style: BlockStyle::Numbered,
                text: "Approve the plan".into(),
            },
            DocxBlock {
                style: BlockStyle::Paragraph,
                text: "ينتقل الأرشيف إلى التخزين المحلي".into(),
            },
        ];
        let a = create_docx("Archive & plan", &blocks).unwrap();
        let b = create_docx("Archive & plan", &blocks).unwrap();
        assert_eq!(a, b, "creation is deterministic");
        let problems = crate::package_integrity(&a);
        assert!(problems.is_empty(), "{problems:?}");
        let doc = DocxDocument::load(&a).unwrap();
        let texts: Vec<&str> = doc.paragraphs.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "Archive migration proposal",
                "Summary",
                "Move the archive to local storage & keep <two> copies.",
                "Two engineers for six weeks",
                "Approve the plan",
                "ينتقل الأرشيف إلى التخزين المحلي",
            ]
        );
        assert_eq!(doc.paragraphs[0].style.as_deref(), Some("Title"));
        assert_eq!(doc.paragraphs[1].style.as_deref(), Some("Heading1"));
        assert_eq!(doc.paragraphs[2].style, None);
        assert_eq!(doc.paragraphs[3].style.as_deref(), Some("ListBullet"));
        // The Arabic paragraph is marked right to left.
        let mut ar = zip::ZipArchive::new(Cursor::new(a.as_slice())).unwrap();
        let mut xml = String::new();
        ar.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert_eq!(xml.matches("<w:bidi/>").count(), 1);
        for s in [
            "title",
            "heading1",
            "heading2",
            "heading3",
            "paragraph",
            "bullet",
            "numbered",
            "compact",
        ] {
            assert_eq!(BlockStyle::parse(s).unwrap().as_str(), s);
        }
    }

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
                &[DocxOp::TextReplace {
                    index: 1,
                    new_text: "New title".into(),
                }],
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
            &[DocxOp::TextReplace {
                index: 1,
                new_text: "x".into(),
            }],
        );
        assert!(matches!(err, Err(DocxError::ParagraphChanged(1))));
    }
}
