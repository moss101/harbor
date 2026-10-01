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

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            // Base-14 WinAnsi: pass printable Latin-1; replace the rest.
            c if (c as u32) >= 32 && (c as u32) < 256 => out.push(c),
            _ => out.push('?'),
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
    objects.push(format!("<< /Type /Catalog /Pages 2 0 R >>").into_bytes()); // 1
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
        let _ = write!(out, "{:010} 00000 n \n", off);
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{}\n%%EOF\n",
        objects.len() + 1,
        xref_at
    );
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
}
