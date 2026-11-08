//! Minimal in-tree PDF writer for text-level export (office Phase 4):
//! DOCX/XLSX content to a valid single-font PDF with NO third-party
//! writer dependency. Base-14 Helvetica/Helvetica-Bold, A4 pages,
//! word-wrapped paragraphs, heading sizes. Fidelity is TEXT-LEVEL ONLY —
//! callers must label it (the FFI reports `extraction_level`), never
//! claim layout parity.

use std::io::Write as _;

/// One output block: (font size, bold, text).
#[derive(Debug, Clone)]
pub struct TextBlock {
    pub size: f64,
    pub bold: bool,
    pub text: String,
    /// Monospace (Courier) — for grid/table text where alignment matters.
    pub mono: bool,
}

const PAGE_W: f64 = 595.0; // A4, points
const PAGE_H: f64 = 842.0;
const MARGIN: f64 = 64.0;
const LINE_GAP: f64 = 5.0;

/// Average glyph width factor for Helvetica (~0.5 em for mixed text —
/// conservative wrapping; exact metrics are not claimed).
const GLYPH_EM: f64 = 0.52;

/// The WinAnsiEncoding byte for `c`, when the base-14 fonts have a glyph
/// for it.
fn win_ansi(c: char) -> Option<u8> {
    let b = match c {
        ' '..='~' => c as u8,
        '\u{a0}'..='\u{ff}' => c as u32 as u8,
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8a,
        '‹' => 0x8b,
        'Œ' => 0x8c,
        'Ž' => 0x8e,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9a,
        '›' => 0x9b,
        'œ' => 0x9c,
        'ž' => 0x9e,
        'Ÿ' => 0x9f,
        _ => return None,
    };
    Some(b)
}

/// Characters of `text` the base-14 fonts cannot show (Arabic, CJK,
/// emoji…): they are written as `?`, so callers can warn before saving.
pub fn unsupported_chars(text: &str) -> usize {
    text.chars()
        .filter(|&c| c != '\n' && c != '\t' && win_ansi(c).is_none())
        .count()
}

/// A PDF string literal body in pure ASCII: bytes above 0x7E as octal
/// escapes of their WinAnsi code. Writing the Rust string's UTF-8 bytes
/// instead turned "José" into "JosÃ©".
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\t' => out.push(' '),
            c => match win_ansi(c) {
                Some(b) if b < 0x80 => out.push(b as char),
                Some(b) => out.push_str(&format!("\\{b:03o}")),
                None => out.push('?'),
            },
        }
    }
    out
}

fn wrap(text: &str, size: f64) -> Vec<String> {
    let max_chars = (((PAGE_W - 2.0 * MARGIN) / (size * GLYPH_EM)).floor() as usize).max(8);
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        if raw.trim().is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in raw.split_whitespace() {
            if current.is_empty() {
                current = word.to_string();
            } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current);
                current = word.to_string();
            }
        }
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Write blocks to a valid PDF (one font per size/bold pair, page breaks
/// automatic). Deterministic: same input → byte-identical output.
pub fn write_text_pdf(title: &str, blocks: &[TextBlock]) -> Vec<u8> {
    // Lay out lines with page breaks.
    let mut pages: Vec<Vec<(f64, bool, bool, String)>> = Vec::new();
    let mut current: Vec<(f64, bool, bool, String)> = Vec::new();
    let mut y = PAGE_H - MARGIN;
    fn push_line(
        line: (f64, bool, bool, String),
        y: &mut f64,
        current: &mut Vec<(f64, bool, bool, String)>,
        pages: &mut Vec<Vec<(f64, bool, bool, String)>>,
    ) {
        let advance = line.0 + LINE_GAP;
        if *y - advance < MARGIN {
            pages.push(std::mem::take(current));
            *y = PAGE_H - MARGIN;
        }
        current.push(line);
        *y -= advance;
    }
    for b in blocks {
        for line in wrap(&b.text, b.size) {
            push_line(
                (b.size, b.bold, b.mono, line),
                &mut y,
                &mut current,
                &mut pages,
            );
        }
        // Paragraph spacing.
        y -= b.size * 0.45;
    }
    if !current.is_empty() {
        pages.push(current);
    }
    if pages.is_empty() {
        pages.push(Vec::new());
    }

    // Content streams.
    let streams: Vec<String> = pages
        .iter()
        .map(|lines| {
            let mut s = String::new();
            let mut yy = PAGE_H - MARGIN;
            for (size, bold, mono, line) in lines {
                let font = match (*bold, *mono) {
                    (true, true) => "/F4",
                    (false, true) => "/F3",
                    (true, false) => "/F2",
                    (false, false) => "/F1",
                };
                let _ = std::fmt::Write::write_fmt(
                    &mut s,
                    format_args!(
                        "BT {} {} Tf 1 0 0 1 {:.2} {:.2} Tm ({}) Tj ET\n",
                        font,
                        size,
                        MARGIN,
                        yy - size,
                        escape(line)
                    ),
                );
                yy -= size + LINE_GAP;
            }
            s
        })
        .collect();

    // Assemble the objects: catalog, pages, page(s), fonts, streams.
    let mut objects: Vec<Vec<u8>> = Vec::new();
    let page_count = pages.len();
    let mut kids = String::new();
    for p in 0..page_count {
        kids.push_str(&format!("{} 0 R ", 4 + 2 * p));
    }
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()); // 1
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids, page_count).into_bytes()); // 2
    objects.push(
        "<< /Title (escaped-later) >>"
            .replace("escaped-later", &escape(title))
            .into_bytes(),
    ); // 3 (Info)
    for (i, stream) in streams.iter().enumerate() {
        let page_obj = 4 + 2 * i;
        let content_obj = page_obj + 1;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Resources << /Font << /F1 {} 0 R /F2 {} 0 R /F3 {} 0 R /F4 {} 0 R >> >> /Contents {content_obj} 0 R >>",
            4 + 2 * page_count,
            5 + 2 * page_count,
            6 + 2 * page_count,
            7 + 2 * page_count
        )
        .into_bytes());
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                stream.len(),
                stream
            )
            .into_bytes(),
        );
    }
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .as_bytes()
            .to_vec(),
    );
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .as_bytes()
            .to_vec(),
    );
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>"
            .as_bytes()
            .to_vec(),
    );
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier-Bold /Encoding /WinAnsiEncoding >>"
            .as_bytes()
            .to_vec(),
    );

    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for off in &offsets {
        let _ = write!(out, "{:010} 00000 n ", off);
        let _ = out.write_all(b"\n");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{}\n%%EOF\n",
        objects.len() + 1,
        xref_at
    );
    out
}

/// Blocks for a chat answer written in light Markdown, as models write
/// them: `#`/`##`/`###` headings, a line that is all `**bold**` as a
/// subheading, `-`/`*` bullets, ``` fences as monospace, inline emphasis
/// and code marks dropped, `[text](url)` as "text (url)". One column of
/// real text — the layout applicant-tracking systems parse best.
pub fn markdown_blocks(text: &str) -> Vec<TextBlock> {
    const BODY: f64 = 10.5;
    let mut blocks = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut in_code = false;
    let flush = |para: &mut Vec<String>, blocks: &mut Vec<TextBlock>, mono: bool| {
        if !para.is_empty() {
            blocks.push(TextBlock {
                size: if mono { 9.0 } else { BODY },
                bold: false,
                text: para.join("\n"),
                mono,
            });
            para.clear();
        }
    };
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim_start().starts_with("```") {
            flush(&mut para, &mut blocks, in_code);
            in_code = !in_code;
            continue;
        }
        if in_code {
            para.push(line.to_string());
            continue;
        }
        let t = line.trim();
        if t.is_empty() || t.chars().all(|c| c == '-' || c == '*' || c == '_') && t.len() >= 3 {
            flush(&mut para, &mut blocks, false);
            continue;
        }
        let heading = [("### ", 12.0), ("## ", 14.0), ("# ", 18.0)]
            .iter()
            .find_map(|(mark, size)| t.strip_prefix(mark).map(|h| (h, *size)));
        let bold_line = (t.len() > 4
            && ((t.starts_with("**") && t.ends_with("**"))
                || (t.starts_with("__") && t.ends_with("__")))
            && !t[2..t.len() - 2].contains("**"))
        .then(|| &t[2..t.len() - 2]);
        if let Some((h, size)) = heading.or(bold_line.map(|h| (h, 12.0))) {
            flush(&mut para, &mut blocks, false);
            blocks.push(TextBlock {
                size,
                bold: true,
                text: inline(h.trim_end_matches(':').trim()),
                mono: false,
            });
            continue;
        }
        let t = t.strip_prefix("> ").unwrap_or(t);
        let item = ["- ", "* ", "+ ", "• "]
            .iter()
            .find_map(|m| t.strip_prefix(m));
        para.push(match item {
            Some(rest) => format!("• {}", inline(rest)),
            None => inline(t),
        });
    }
    flush(&mut para, &mut blocks, in_code);
    blocks
}

/// Inline Markdown to plain text: emphasis and code marks removed, links
/// kept with their target.
fn inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' | '`' => i += 1,
            '_' if i + 1 < chars.len() && chars[i + 1] == '_' => i += 2,
            '[' => {
                let rest: String = chars[i + 1..].iter().collect();
                if let Some((label, after)) = rest.split_once("](") {
                    if let Some((url, _)) = after.split_once(')') {
                        if !label.contains(']') {
                            out.push_str(&format!("{label} ({url})"));
                            let used = 1 + label.chars().count() + 2 + url.chars().count() + 1;
                            i += used;
                            continue;
                        }
                    }
                }
                out.push('[');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_pdf_the_extractor_reads_back() {
        let blocks = vec![
            TextBlock { size: 20.0, bold: true, text: "Harbor Office Suite".into(), mono: false },
            TextBlock { size: 11.0, bold: false, text: "A paragraph with (parens), back\\slash and enough words to force at least one wrap across the line for the layout engine to exercise.".into(), mono: false },
        ];
        let pdf = write_text_pdf("Round trip", &blocks);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        // The qualified extractor reads it back: text survived the trip.
        let preview = crate::pdf::extract_pages(&pdf).unwrap();
        let all: String = preview.pages.iter().map(|p| p.text.clone()).collect();
        assert!(all.contains("Harbor Office Suite"));
        assert!(all.contains("back\\slash"), "escapes must round-trip");
        assert!(preview.page_count >= 1);
    }

    #[test]
    fn mono_blocks_round_trip() {
        let blocks = vec![
            TextBlock {
                size: 14.0,
                bold: true,
                text: "Sheet1".into(),
                mono: false,
            },
            TextBlock {
                size: 9.0,
                bold: false,
                text: "1  A             B".into(),
                mono: true,
            },
            TextBlock {
                size: 9.0,
                bold: false,
                text: "2  10             20".into(),
                mono: true,
            },
        ];
        let pdf = write_text_pdf("grid", &blocks);
        let preview = crate::pdf::extract_pages(&pdf).unwrap();
        let all: String = preview.pages.iter().map(|p| p.text.clone()).collect();
        assert!(all.contains("Sheet1"));
    }

    #[test]
    fn deterministic_bytes() {
        let blocks = vec![TextBlock {
            size: 12.0,
            bold: false,
            text: "same".into(),
            mono: false,
        }];
        let a = write_text_pdf("t", &blocks);
        let b = write_text_pdf("t", &blocks);
        assert_eq!(a, b);
    }

    #[test]
    fn accented_and_typographic_text_survives() {
        let blocks = vec![TextBlock {
            size: 11.0,
            bold: false,
            text: "José Müller – “quoted” • café".into(),
            mono: false,
        }];
        let pdf = write_text_pdf("Ü", &blocks);
        assert!(
            pdf.is_ascii() || !String::from_utf8_lossy(&pdf).contains("Ã"),
            "no UTF-8 bytes in content"
        );
        let content = String::from_utf8_lossy(&pdf);
        assert!(
            content.contains("Jos\\351 M\\374ller \\226 \\223quoted\\224 \\225 caf\\351"),
            "{content}"
        );
        assert_eq!(unsupported_chars("José – ok"), 0);
        assert_eq!(unsupported_chars("مرحبا ok"), 5);
    }

    #[test]
    fn markdown_answer_becomes_headings_bullets_and_paragraphs() {
        let md = "# Jane Doe\nSystems Analyst | jane@example.com\n\n## Experience\n**Acme Corp — Senior Analyst**\n- Led **ERP** rollout for `400` users\n* Cut costs 12%\n\nSee [portfolio](https://x.dev).\n---\n```\nSELECT 1;\n```";
        let b = markdown_blocks(md);
        let view: Vec<(f64, bool, bool, &str)> = b
            .iter()
            .map(|b| (b.size, b.bold, b.mono, b.text.as_str()))
            .collect();
        assert_eq!(
            view,
            vec![
                (18.0, true, false, "Jane Doe"),
                (10.5, false, false, "Systems Analyst | jane@example.com"),
                (14.0, true, false, "Experience"),
                (12.0, true, false, "Acme Corp — Senior Analyst"),
                (
                    10.5,
                    false,
                    false,
                    "• Led ERP rollout for 400 users\n• Cut costs 12%"
                ),
                (10.5, false, false, "See portfolio (https://x.dev)."),
                (9.0, false, true, "SELECT 1;"),
            ]
        );
        // And it is a real PDF the extractor reads back.
        let pdf = write_text_pdf("Jane Doe", &b);
        let text: String = crate::pdf::extract_pages(&pdf)
            .unwrap()
            .pages
            .iter()
            .map(|p| p.text.clone())
            .collect();
        assert!(
            text.contains("Experience") && text.contains("Led ERP rollout"),
            "{text}"
        );
    }
}
