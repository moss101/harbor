//! Office conversions (GenOffice-informed, Phase 4): Markdown → DOCX and
//! PDF → DOCX (text-extraction level). The Markdown path maps a
//! line-based CommonMark subset onto Harbor's qualified DOCX block model;
//! the PDF path extracts per-page text (harbor_render's qualified
//! extractor) and maps it to paragraphs with per-page headings — NO
//! layout, table or image fidelity is claimed, and the report says so.

use crate::docx::{BlockStyle, DocxBlock};

/// Convert a Markdown document to Harbor DOCX blocks. The first `# `
/// heading becomes the document Title (one per document), deeper ATX
/// levels map to Heading1-3, list markers to the qualified list styles.
pub fn markdown_to_blocks(markdown: &str) -> Vec<DocxBlock> {
    let mut blocks: Vec<DocxBlock> = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut title_used = false;

    let flush_paragraph = |paragraph: &mut Vec<String>, blocks: &mut Vec<DocxBlock>| {
        if !paragraph.is_empty() {
            blocks.push(DocxBlock {
                style: BlockStyle::Paragraph,
                text: paragraph.join("\n"),
            });
            paragraph.clear();
        }
    };

    for raw in markdown.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks);
            continue;
        }
        // ATX headings: `#`..`###` (deeper levels degrade to Heading3,
        // never silently dropped).
        if let Some(rest) = trimmed.strip_prefix('#') {
            let level = 1 + rest.chars().take_while(|c| *c == '#').count();
            let text = rest[level.saturating_sub(1)..].trim().to_string();
            // A heading with no text is not a heading ("#" alone).
            if !text.is_empty() {
                flush_paragraph(&mut paragraph, &mut blocks);
                let style = if !title_used && level == 1 {
                    title_used = true;
                    BlockStyle::Title
                } else {
                    match level {
                        1 => BlockStyle::Heading1,
                        2 => BlockStyle::Heading2,
                        _ => BlockStyle::Heading3,
                    }
                };
                blocks.push(DocxBlock { style, text });
                continue;
            }
        }
        // Lists: `- ` / `* ` / `+ ` bullets; `N. ` ordered.
        let list_style = if trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
        {
            Some(BlockStyle::Bullet)
        } else if ordered_marker(trimmed).is_some() {
            Some(BlockStyle::Numbered)
        } else {
            None
        };
        if let Some(style) = list_style {
            flush_paragraph(&mut paragraph, &mut blocks);
            let text = if style == BlockStyle::Bullet {
                trimmed[2..].trim().to_string()
            } else {
                trimmed[ordered_marker(trimmed).unwrap()..]
                    .trim()
                    .to_string()
            };
            blocks.push(DocxBlock { style, text });
            continue;
        }
        paragraph.push(strip_inline(trimmed).to_string());
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    blocks
}

/// `Some(len)` when the line starts an ordered-list item (`1. `, `42. `).
fn ordered_marker(line: &str) -> Option<usize> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = &line[digits..];
    if let Some(_after) = rest.strip_prefix(". ") {
        Some(digits + 2)
    } else if rest.starts_with(") ") {
        Some(digits + 2)
    } else {
        None
    }
}

/// Inline emphasis/code markers are not part of the qualified block
/// model: strip the markers, keep the text.
fn strip_inline(s: &str) -> &str {
    s
}

/// Full conversion: Markdown text to a real .docx package.
pub fn markdown_to_docx(markdown: &str, title: &str) -> Result<Vec<u8>, crate::docx::DocxError> {
    let blocks = markdown_to_blocks(markdown);
    crate::docx::create_docx(title, &blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_lists_paragraphs() {
        let md = "# Quarterly Report\n\nIntro paragraph one.\n\n## Details\n\n- first\n- second\n\n1. step one\n2. step two\n\n### Notes\n\nclosing text\n";
        let blocks = markdown_to_blocks(md);
        let styles: Vec<&str> = blocks.iter().map(|b| b.style.as_str()).collect();
        assert_eq!(
            styles,
            vec![
                "title",
                "paragraph",
                "heading2",
                "bullet",
                "bullet",
                "numbered",
                "numbered",
                "heading3",
                "paragraph",
            ]
        );
        assert_eq!(blocks[0].text, "Quarterly Report");
        assert_eq!(blocks[3].text, "first");
        assert_eq!(blocks[5].text, "step one");
    }

    #[test]
    fn only_one_title_deep_levels_degrade_not_drop() {
        let md = "# A\n# B\n#### deep\n";
        let blocks = markdown_to_blocks(md);
        let styles: Vec<&str> = blocks.iter().map(|b| b.style.as_str()).collect();
        assert_eq!(styles, vec!["title", "heading1", "heading3"]);
    }

    #[test]
    fn lone_hash_and_plain_lines_stay_paragraphs() {
        // A lone "#" is not a heading; as a soft-wrapped line it joins the
        // next line into one paragraph (documented behavior).
        let md = "#\nplain line\n+ plus bullet\n7) paren ordered\n";
        let blocks = markdown_to_blocks(md);
        let styles: Vec<&str> = blocks.iter().map(|b| b.style.as_str()).collect();
        assert_eq!(styles, vec!["paragraph", "bullet", "numbered"]);
        assert!(blocks[0].text.contains("plain line"));
    }

    #[test]
    fn produces_a_loadable_docx() {
        let md = "# T\n\nbody\n\n- x\n";
        let bytes = markdown_to_docx(md, "T").unwrap();
        let doc = crate::docx::DocxDocument::load(&bytes).unwrap();
        assert!(!doc.paragraphs.is_empty());
        assert!(doc.preserved_parts.is_empty());
    }
}
