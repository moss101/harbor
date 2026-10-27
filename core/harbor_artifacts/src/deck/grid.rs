//! The layout grid and text-fit estimation.
//!
//! Every layout hangs off one grid (a 12-column field with fixed margins),
//! so titles, kickers, footers and content edges line up from slide to
//! slide. Text sizes are *fitted*, not just authored: `fit_pt` picks the
//! largest size at which the text fits its box using an average glyph
//! width, so generated slides do not depend on a viewer's autofit pass
//! (which Keynote, Google Slides and our own preview apply differently).

use super::xml::{R, SLIDE_H, SLIDE_W};

pub const MARGIN_X: i64 = 762_000; // 0.83 in
pub const CONTENT_W: i64 = SLIDE_W - 2 * MARGIN_X;
pub const GUTTER: i64 = 274_320; // 0.3 in

pub const KICKER_Y: i64 = 396_000;
pub const KICKER_H: i64 = 260_000;
pub const TITLE_Y: i64 = 640_000;
pub const TITLE_H: i64 = 1_040_000;
pub const BODY_Y: i64 = 1_850_000;
pub const BODY_BOTTOM: i64 = 6_180_000;
pub const BODY_H: i64 = BODY_BOTTOM - BODY_Y;
pub const FOOTER_Y: i64 = 6_400_000;
pub const FOOTER_H: i64 = 280_000;

pub const EMU_PER_PT: f64 = 12_700.0;

pub fn title_rect() -> R {
    R::new(MARGIN_X, TITLE_Y, CONTENT_W, TITLE_H)
}

pub fn kicker_rect() -> R {
    R::new(MARGIN_X, KICKER_Y, CONTENT_W, KICKER_H)
}

pub fn body_rect() -> R {
    R::new(MARGIN_X, BODY_Y, CONTENT_W, BODY_H)
}

/// `n` equal columns across `area` with `gap` between them.
pub fn columns(area: R, n: usize, gap: i64) -> Vec<R> {
    let n = n.max(1) as i64;
    let w = (area.w - gap * (n - 1)) / n;
    (0..n)
        .map(|i| R::new(area.x + i * (w + gap), area.y, w, area.h))
        .collect()
}

/// Average glyph width as a fraction of the font size for the faces the
/// themes use (measured on typical English text; Arabic runs close to it).
pub fn glyph_factor(font: &str) -> f64 {
    match font {
        "Calibri" | "Calibri Light" => 0.49,
        "Cambria" => 0.52,
        "Trebuchet MS" => 0.54,
        "Arial" => 0.55,
        "Georgia" => 0.56,
        "Century Gothic" => 0.60,
        _ => 0.55,
    }
}

/// Number of wrapped lines `text` needs in `width_emu` at `pt`.
pub fn line_count(text: &str, pt: f64, width_emu: i64, factor: f64) -> usize {
    let per_line = ((width_emu as f64 / EMU_PER_PT) / (pt * factor)).floor().max(1.0) as usize;
    let mut lines = 0usize;
    for hard in text.split('\n') {
        let mut cur = 0usize;
        let mut n = 1usize;
        for word in hard.split_whitespace() {
            let w = word.chars().count();
            if cur == 0 {
                cur = w;
            } else if cur + 1 + w <= per_line {
                cur += 1 + w;
            } else {
                n += 1;
                cur = w;
            }
            // A single word longer than a line breaks mid-word.
            while cur > per_line {
                n += 1;
                cur -= per_line;
            }
        }
        lines += n;
    }
    lines.max(1)
}

/// Height in EMU of paragraphs at `pt` with `leading` line spacing and
/// `gap_pt` between paragraphs.
pub fn stack_height(
    paras: &[String],
    pt: f64,
    width_emu: i64,
    factor: f64,
    leading: f64,
    gap_pt: f64,
) -> i64 {
    let mut total_pt = 0.0;
    for (i, p) in paras.iter().enumerate() {
        let lines = line_count(p, pt, width_emu, factor) as f64;
        total_pt += lines * pt * 1.2 * leading;
        if i + 1 < paras.len() {
            total_pt += gap_pt;
        }
    }
    (total_pt * EMU_PER_PT).round() as i64
}

/// The largest size in `[min_pt, max_pt]` (0.5 pt steps) at which `paras`
/// fit a box of `width_emu` x `height_emu`; `min_pt` when none does.
pub fn fit_pt(
    paras: &[String],
    width_emu: i64,
    height_emu: i64,
    max_pt: f64,
    min_pt: f64,
    factor: f64,
    leading: f64,
    gap_pt: f64,
) -> f64 {
    let mut pt = max_pt;
    while pt > min_pt {
        if stack_height(paras, pt, width_emu, factor, leading, gap_pt) <= height_emu {
            return pt;
        }
        pt -= 0.5;
    }
    min_pt
}

/// Slide centre helpers.
pub fn centre_x(w: i64) -> i64 {
    (SLIDE_W - w) / 2
}

pub fn centre_y(h: i64) -> i64 {
    (SLIDE_H - h) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_tile_the_area_without_overlap() {
        let area = R::new(100, 50, 1000, 200);
        let cols = columns(area, 3, 20);
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0].x, 100);
        for pair in cols.windows(2) {
            assert_eq!(pair[1].x - pair[0].right(), 20);
        }
        assert!(cols[2].right() <= area.right());
        assert_eq!(columns(area, 0, 20).len(), 1, "never zero columns");
    }

    #[test]
    fn wrapping_estimate_is_monotonic_and_breaks_long_words() {
        let w = 3 * 914_400; // 3 in
        let short = line_count("Revenue grew", 20.0, w, 0.5);
        let long = line_count(&"Revenue grew steadily across every region ".repeat(4), 20.0, w, 0.5);
        assert!(long > short);
        assert!(line_count(&"x".repeat(200), 20.0, w, 0.5) > 5, "an unbroken word still wraps");
        assert_eq!(line_count("", 20.0, w, 0.5), 1);
        assert_eq!(line_count("a\nb\nc", 20.0, w, 0.5), 3, "hard breaks count");
    }

    #[test]
    fn fit_picks_the_largest_size_that_fits_and_never_goes_below_the_floor() {
        let paras = vec!["Three enterprise wins this quarter".to_string(); 4];
        let roomy = fit_pt(&paras, 9_000_000, 3_000_000, 24.0, 12.0, 0.5, 1.0, 8.0);
        assert_eq!(roomy, 24.0);
        let tight = fit_pt(&paras, 3_000_000, 1_500_000, 24.0, 12.0, 0.5, 1.0, 8.0);
        assert!(tight < 24.0 && tight >= 12.0);
        assert!(stack_height(&paras, tight, 3_000_000, 0.5, 1.0, 8.0) <= 1_500_000 || tight == 12.0);
        let hopeless = fit_pt(&["word ".repeat(500)], 1_000_000, 500_000, 24.0, 12.0, 0.5, 1.0, 8.0);
        assert_eq!(hopeless, 12.0);
    }
}
