//! The theme catalogue. A theme is a palette, a font pair and a handful of
//! style decisions (how the cover is composed, what ornaments the layouts
//! carry, how cards look). Layouts consult these knobs instead of
//! hard-coding looks, so fourteen themes read as fourteen designs from one
//! set of layout builders.
//!
//! Fonts are the families every Office install ships on Windows and macOS
//! (Calibri, Georgia, Arial, Trebuchet MS, Century Gothic, Cambria): a
//! generated deck opened elsewhere must not reflow because a face is
//! missing. No font is embedded.

use super::look::{
    AgendaStyle, CardsStyle, CompareStyle, Header, Look, SectionStyle, StatsStyle, TeamStyle,
    TimelineStyle,
};
pub use super::look::{CardStyle, CoverStyle, Deco};
use super::palette;
use super::xml::Clr;

/// How the deck's colours map: a dark deck swaps `bg1`/`tx1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

/// Hex without `#`.
pub type Hex = &'static str;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dk1: Hex,
    pub lt1: Hex,
    pub dk2: Hex,
    pub lt2: Hex,
    pub accents: [Hex; 6],
}

#[derive(Debug, Clone, Copy)]
pub struct Fonts {
    pub major: &'static str,
    pub minor: &'static str,
    /// Complex-script (Arabic) face.
    pub cs: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub tagline: &'static str,
    pub mode: Mode,
    pub palette: Palette,
    pub fonts: Fonts,
    pub cover: CoverStyle,
    pub deco: Deco,
    pub card: CardStyle,
    /// The design language: header composition and the arrangement each
    /// layout takes (see `look.rs`).
    pub look: Look,
    /// Cover, section and closing slides render inverted (light themes on
    /// their dark ink) for drama.
    pub inverse_hero: bool,
    /// Content pages sit on `bg2` so raised cards read.
    pub tinted_page: bool,
    /// Title weight: heavy faces look best bold, light ones regular.
    pub bold_titles: bool,
    /// Rounded-corner radius for cards, percent of the short side.
    pub radius: u32,
    /// Purposes this theme was designed for (see `purpose.rs`).
    pub suits: &'static [&'static str],
}

impl ThemeSpec {
    /// The same design in another palette (colour, and dark or light).
    pub fn with_palette(&self, p: &super::palette::PaletteSpec) -> ThemeSpec {
        ThemeSpec {
            palette: p.palette,
            mode: p.mode,
            ..*self
        }
    }

    pub fn accent(&self, n: u8) -> Clr {
        Clr::accent(n)
    }

    pub fn is_dark(&self) -> bool {
        self.mode == Mode::Dark
    }
}

const CAL: Fonts = Fonts {
    major: "Calibri",
    minor: "Calibri",
    cs: "Arial",
};

#[allow(clippy::too_many_arguments)]
const fn look(
    header: Header,
    stats: StatsStyle,
    cards: CardsStyle,
    timeline: TimelineStyle,
    compare: CompareStyle,
    agenda: AgendaStyle,
    team: TeamStyle,
    section: SectionStyle,
) -> Look {
    Look {
        header,
        stats,
        cards,
        timeline,
        compare,
        agenda,
        team,
        section,
    }
}

use AgendaStyle as Ag;
use CardsStyle as Cs;
use CompareStyle as Cp;
use SectionStyle as Sc;
use StatsStyle as St;
use TeamStyle as Tm;
use TimelineStyle as Tl;

const LOOK_AURORA: Look = look(
    Header::TopLeft,
    St::Tiles,
    Cs::Grid,
    Tl::Spine,
    Cp::Panels,
    Ag::Rows,
    Tm::Cards,
    Sc::Numeral,
);
const LOOK_HARBOR: Look = look(
    Header::Banner,
    St::Tiles,
    Cs::Grid,
    Tl::Chevrons,
    Cp::Panels,
    Ag::Rows,
    Tm::Cards,
    Sc::Band,
);
const LOOK_IVORY: Look = look(
    Header::Rail,
    St::Rows,
    Cs::Rows,
    Tl::Vertical,
    Cp::Rows,
    Ag::Rows,
    Tm::Inline,
    Sc::Numeral,
);
const LOOK_SLATE: Look = look(
    Header::TopLeft,
    St::Hero,
    Cs::Columns,
    Tl::Spine,
    Cp::Fields,
    Ag::Grid,
    Tm::Cards,
    Sc::Split,
);
const LOOK_SUNRISE: Look = look(
    Header::TopLeft,
    St::Tiles,
    Cs::Stickers,
    Tl::Chevrons,
    Cp::Panels,
    Ag::Grid,
    Tm::Cards,
    Sc::Band,
);
const LOOK_OBSIDIAN: Look = look(
    Header::TopLeft,
    St::Hero,
    Cs::Bento,
    Tl::Chevrons,
    Cp::Panels,
    Ag::Grid,
    Tm::Cards,
    Sc::Split,
);
const LOOK_OCEAN: Look = look(
    Header::Sidebar { right: false },
    St::Rows,
    Cs::Rows,
    Tl::Vertical,
    Cp::Rows,
    Ag::Rows,
    Tm::Inline,
    Sc::Split,
);
const LOOK_FOREST: Look = look(
    Header::Centered,
    St::Columns,
    Cs::Columns,
    Tl::Spine,
    Cp::Panels,
    Ag::Grid,
    Tm::Inline,
    Sc::Numeral,
);
const LOOK_MIDNIGHT: Look = look(
    Header::Rail,
    St::Columns,
    Cs::Columns,
    Tl::Spine,
    Cp::Fields,
    Ag::Grid,
    Tm::Cards,
    Sc::Band,
);
const LOOK_PLAYGROUND: Look = look(
    Header::Centered,
    St::Tiles,
    Cs::Stickers,
    Tl::Chevrons,
    Cp::Fields,
    Ag::Grid,
    Tm::Cards,
    Sc::Band,
);
const LOOK_MONO: Look = look(
    Header::Poster,
    St::Hero,
    Cs::Columns,
    Tl::Vertical,
    Cp::Fields,
    Ag::Rows,
    Tm::Inline,
    Sc::Band,
);
const LOOK_TERRACOTTA: Look = look(
    Header::Sidebar { right: true },
    St::Tiles,
    Cs::Grid,
    Tl::Spine,
    Cp::Fields,
    Ag::Grid,
    Tm::Cards,
    Sc::Numeral,
);
const LOOK_ROYAL: Look = look(
    Header::Poster,
    St::Tiles,
    Cs::Bento,
    Tl::Spine,
    Cp::Panels,
    Ag::Grid,
    Tm::Cards,
    Sc::Split,
);
const LOOK_BLUEPRINT: Look = look(
    Header::Banner,
    St::Columns,
    Cs::Columns,
    Tl::Vertical,
    Cp::Rows,
    Ag::Grid,
    Tm::Inline,
    Sc::Numeral,
);

pub const THEMES: &[ThemeSpec] = &[
    ThemeSpec {
        id: "harbor",
        name: "Harbor",
        tagline: "Clean, confident corporate blue",
        mode: Mode::Light,
        palette: palette::HARBOR,
        fonts: CAL,
        cover: CoverStyle::Band,
        deco: Deco::None,
        card: CardStyle::Filled,
        look: LOOK_HARBOR,
        inverse_hero: true,
        tinted_page: false,
        bold_titles: true,
        radius: 8,
        suits: &["project_report", "quarterly_review", "general"],
    },
    ThemeSpec {
        id: "aurora",
        name: "Aurora",
        tagline: "Dark, luminous gradients for a launch-night feel",
        mode: Mode::Dark,
        palette: palette::AURORA,
        fonts: Fonts {
            major: "Century Gothic",
            minor: "Calibri",
            cs: "Arial",
        },
        cover: CoverStyle::Aurora,
        deco: Deco::Orbs,
        card: CardStyle::Glass,
        look: LOOK_AURORA,
        inverse_hero: false,
        tinted_page: false,
        bold_titles: true,
        radius: 10,
        suits: &["pitch_deck", "product_launch", "keynote"],
    },
    ThemeSpec {
        id: "ivory",
        name: "Ivory",
        tagline: "Warm paper, deep green and gold; serif authority",
        mode: Mode::Light,
        palette: palette::IVORY,
        fonts: Fonts {
            major: "Georgia",
            minor: "Calibri",
            cs: "Arial",
        },
        cover: CoverStyle::Editorial,
        deco: Deco::None,
        card: CardStyle::Rule,
        look: LOOK_IVORY,
        inverse_hero: false,
        tinted_page: false,
        bold_titles: false,
        radius: 4,
        suits: &["investor_update", "annual_report", "board"],
    },
    ThemeSpec {
        id: "slate",
        name: "Slate",
        tagline: "Calm greys with signal colours for status reporting",
        mode: Mode::Light,
        palette: palette::SLATE,
        fonts: CAL,
        cover: CoverStyle::Split,
        deco: Deco::Bar,
        card: CardStyle::Raised,
        look: LOOK_SLATE,
        inverse_hero: true,
        tinted_page: true,
        bold_titles: true,
        radius: 6,
        suits: &["project_report", "status_update", "technical_review"],
    },
    ThemeSpec {
        id: "sunrise",
        name: "Sunrise",
        tagline: "Warm coral-to-amber energy for consumer launches",
        mode: Mode::Light,
        palette: palette::SUNRISE,
        fonts: Fonts {
            major: "Trebuchet MS",
            minor: "Calibri",
            cs: "Arial",
        },
        cover: CoverStyle::Aurora,
        deco: Deco::Orbs,
        card: CardStyle::Sticker,
        look: LOOK_SUNRISE,
        inverse_hero: false,
        tinted_page: true,
        bold_titles: true,
        radius: 14,
        suits: &["product_launch", "marketing_plan", "all_hands"],
    },
    ThemeSpec {
        id: "obsidian",
        name: "Obsidian",
        tagline: "Black, white and one electric lime accent",
        mode: Mode::Dark,
        palette: palette::OBSIDIAN,
        fonts: Fonts {
            major: "Arial",
            minor: "Arial",
            cs: "Arial",
        },
        cover: CoverStyle::Bento,
        deco: Deco::None,
        card: CardStyle::Bento,
        look: LOOK_OBSIDIAN,
        inverse_hero: false,
        tinted_page: false,
        bold_titles: true,
        radius: 2,
        suits: &["pitch_deck", "keynote", "product_launch"],
    },
    ThemeSpec {
        id: "ocean",
        name: "Ocean",
        tagline: "Deep teal and open water for proposals and strategy",
        mode: Mode::Light,
        palette: palette::OCEAN,
        fonts: CAL,
        cover: CoverStyle::Split,
        deco: Deco::Arcs,
        card: CardStyle::Filled,
        look: LOOK_OCEAN,
        inverse_hero: true,
        tinted_page: false,
        bold_titles: true,
        radius: 10,
        suits: &["client_proposal", "strategy", "general"],
    },
    ThemeSpec {
        id: "forest",
        name: "Forest",
        tagline: "Grounded greens and sand for impact and learning",
        mode: Mode::Light,
        palette: palette::FOREST,
        fonts: Fonts {
            major: "Cambria",
            minor: "Calibri",
            cs: "Arial",
        },
        cover: CoverStyle::Centered,
        deco: Deco::Arcs,
        card: CardStyle::Outlined,
        look: LOOK_FOREST,
        inverse_hero: true,
        tinted_page: false,
        bold_titles: true,
        radius: 12,
        suits: &["impact_report", "training", "research"],
    },
    ThemeSpec {
        id: "midnight_gold",
        name: "Midnight Gold",
        tagline: "Navy and brushed gold for investor audiences",
        mode: Mode::Dark,
        palette: palette::MIDNIGHT_GOLD,
        fonts: Fonts {
            major: "Georgia",
            minor: "Calibri",
            cs: "Arial",
        },
        cover: CoverStyle::Centered,
        deco: Deco::Rule,
        card: CardStyle::Glass,
        look: LOOK_MIDNIGHT,
        inverse_hero: false,
        tinted_page: false,
        bold_titles: false,
        radius: 4,
        suits: &["pitch_deck", "investor_update", "board"],
    },
    ThemeSpec {
        id: "playground",
        name: "Playground",
        tagline: "Bright, friendly colour for workshops and teaching",
        mode: Mode::Light,
        palette: palette::PLAYGROUND,
        fonts: Fonts {
            major: "Trebuchet MS",
            minor: "Calibri",
            cs: "Arial",
        },
        cover: CoverStyle::Aurora,
        deco: Deco::Orbs,
        card: CardStyle::Sticker,
        look: LOOK_PLAYGROUND,
        inverse_hero: false,
        tinted_page: true,
        bold_titles: true,
        radius: 16,
        suits: &["training", "workshop", "all_hands"],
    },
    ThemeSpec {
        id: "mono",
        name: "Mono",
        tagline: "Editorial black and white with one red accent",
        mode: Mode::Light,
        palette: palette::MONO,
        fonts: Fonts {
            major: "Georgia",
            minor: "Arial",
            cs: "Arial",
        },
        cover: CoverStyle::Poster,
        deco: Deco::None,
        card: CardStyle::Rule,
        look: LOOK_MONO,
        inverse_hero: false,
        tinted_page: false,
        bold_titles: true,
        radius: 0,
        suits: &["keynote", "case_study", "research"],
    },
    ThemeSpec {
        id: "terracotta",
        name: "Terracotta",
        tagline: "Earthy warmth for case studies and stories",
        mode: Mode::Light,
        palette: palette::TERRACOTTA,
        fonts: Fonts {
            major: "Georgia",
            minor: "Calibri",
            cs: "Arial",
        },
        cover: CoverStyle::Editorial,
        deco: Deco::Arcs,
        card: CardStyle::Filled,
        look: LOOK_TERRACOTTA,
        inverse_hero: true,
        tinted_page: false,
        bold_titles: false,
        radius: 8,
        suits: &["case_study", "research", "client_proposal"],
    },
    ThemeSpec {
        id: "royal",
        name: "Royal",
        tagline: "Deep violet and hot pink for creative occasions",
        mode: Mode::Dark,
        palette: palette::ROYAL,
        fonts: Fonts {
            major: "Century Gothic",
            minor: "Calibri",
            cs: "Arial",
        },
        cover: CoverStyle::Bento,
        deco: Deco::Orbs,
        card: CardStyle::Bento,
        look: LOOK_ROYAL,
        inverse_hero: false,
        tinted_page: false,
        bold_titles: true,
        radius: 14,
        suits: &["marketing_plan", "keynote", "product_launch"],
    },
    ThemeSpec {
        id: "blueprint",
        name: "Blueprint",
        tagline: "Engineering blue with a drafting grid",
        mode: Mode::Dark,
        palette: palette::BLUEPRINT,
        fonts: CAL,
        cover: CoverStyle::Split,
        deco: Deco::Grid,
        card: CardStyle::Outlined,
        look: LOOK_BLUEPRINT,
        inverse_hero: false,
        tinted_page: false,
        bold_titles: true,
        radius: 2,
        suits: &["technical_review", "project_report", "strategy"],
    },
];

pub fn theme(id: &str) -> Option<&'static ThemeSpec> {
    THEMES.iter().find(|t| t.id == id)
}

/// The theme used when none is chosen.
pub fn default_theme() -> &'static ThemeSpec {
    &THEMES[0]
}

// ---------------------------------------------------------------------------
// Colour maths for contrast gates (WCAG 2.x relative luminance)

pub fn hex_rgb(h: &str) -> [f64; 3] {
    let v = u32::from_str_radix(h, 16).unwrap_or(0);
    [
        ((v >> 16) & 255) as f64 / 255.0,
        ((v >> 8) & 255) as f64 / 255.0,
        (v & 255) as f64 / 255.0,
    ]
}

fn lin(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn luminance(h: &str) -> f64 {
    let [r, g, b] = hex_rgb(h);
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

pub fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

impl ThemeSpec {
    /// The page colour (`bg1` after the colour map) and ink (`tx1`).
    pub fn page_and_ink(&self) -> (Hex, Hex) {
        match self.mode {
            Mode::Light => (self.palette.lt1, self.palette.dk1),
            Mode::Dark => (self.palette.dk1, self.palette.lt1),
        }
    }
}

fn hsl_of(h: &str) -> (f64, f64, f64) {
    let [r, g, b] = hex_rgb(h);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-12 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let hue = if (max - r).abs() < 1e-12 {
        ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-12 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (hue * 60.0, s, l)
}

fn hex_of_hsl(h: f64, s: f64, l: f64) -> String {
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let f = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    let hn = h / 360.0;
    if s < 1e-12 {
        let v = (l * 255.0).round() as u8;
        return format!("{v:02X}{v:02X}{v:02X}");
    }
    format!(
        "{:02X}{:02X}{:02X}",
        f(hn + 1.0 / 3.0),
        f(hn),
        f(hn - 1.0 / 3.0)
    )
}

impl ThemeSpec {
    /// Resolve a theme colour reference to `RRGGBB` as PowerPoint would
    /// paint it in this deck: the slot through this theme's colour map
    /// (`bg1`/`tx1` swap in a dark deck), then the luminance transforms.
    /// Alpha is returned separately (0..=1) and NOT applied. For tests that
    /// check text contrast against a fill under any palette.
    pub fn resolve(&self, c: &Clr) -> (String, f64) {
        use super::xml::Slot;
        let p = &self.palette;
        let dark = self.is_dark();
        let hex: &str = match c.slot {
            Slot::Bg1 => {
                if dark {
                    p.dk1
                } else {
                    p.lt1
                }
            }
            Slot::Tx1 => {
                if dark {
                    p.lt1
                } else {
                    p.dk1
                }
            }
            Slot::Bg2 => {
                if dark {
                    p.dk2
                } else {
                    p.lt2
                }
            }
            Slot::Tx2 => {
                if dark {
                    p.lt2
                } else {
                    p.dk2
                }
            }
            Slot::Accent(n) => p.accents[(n as usize).clamp(1, 6) - 1],
            Slot::Dk1 => p.dk1,
            Slot::Lt1 => p.lt1,
            Slot::Dk2 => p.dk2,
            Slot::Lt2 => p.lt2,
        };
        let mut out = hex.to_string();
        if c.lum_mod.is_some() || c.lum_off.is_some() {
            let (h, s, l) = hsl_of(hex);
            let l = (l * c.lum_mod.unwrap_or(100) as f64 / 100.0
                + c.lum_off.unwrap_or(0) as f64 / 100.0)
                .clamp(0.0, 1.0);
            out = hex_of_hsl(h, s, l);
        }
        (out, c.alpha.map_or(1.0, |a| a as f64 / 100.0))
    }

    /// `resolve`, with the colour's alpha composited over `behind`
    /// (`RRGGBB`): what the eye sees for a translucent fill.
    pub fn resolve_over(&self, c: &Clr, behind: &str) -> String {
        let (hex, a) = self.resolve(c);
        let (x, y) = (hex_rgb(&hex), hex_rgb(behind));
        format!(
            "{:02X}{:02X}{:02X}",
            ((x[0] * a + y[0] * (1.0 - a)) * 255.0).round() as u8,
            ((x[1] * a + y[1] * (1.0 - a)) * 255.0).round() as u8,
            ((x[2] * a + y[2] * (1.0 - a)) * 255.0).round() as u8
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_every_theme_names_its_purposes() {
        let mut ids: Vec<_> = THEMES.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), THEMES.len());
        assert!(THEMES.len() >= 14);
        for t in THEMES {
            assert!(!t.suits.is_empty(), "{}", t.id);
            for h in [t.palette.dk1, t.palette.lt1, t.palette.dk2, t.palette.lt2]
                .iter()
                .chain(t.palette.accents.iter())
            {
                assert_eq!(h.len(), 6, "{} {}", t.id, h);
                assert!(u32::from_str_radix(h, 16).is_ok(), "{} {}", t.id, h);
            }
        }
    }

    /// Body text must be comfortably readable in EVERY design x palette
    /// combination: ink on the page at AAA (7:1), muted ink at AA (4.5:1) on
    /// both page tones, and the primary accent at the large-text bar (3:1).
    /// A palette is a free choice on any design, so every pairing is gated.
    #[test]
    fn every_design_in_every_palette_clears_contrast_gates() {
        use super::super::palette::PALETTES;
        for base in THEMES {
            for pal in PALETTES {
                let t = base.with_palette(pal);
                let (page, ink) = t.page_and_ink();
                let tint = match t.mode {
                    Mode::Light => t.palette.lt2,
                    Mode::Dark => t.palette.dk2,
                };
                let who = format!("{} in {}", base.id, pal.id);
                assert!(
                    contrast(ink, page) >= 7.0,
                    "{who}: ink/page {:.2}",
                    contrast(ink, page)
                );
                assert!(
                    contrast(ink, tint) >= 7.0,
                    "{who}: ink/tint {:.2}",
                    contrast(ink, tint)
                );
                let mix = |a: &str, b: &str, k: f64| {
                    let (x, y) = (hex_rgb(a), hex_rgb(b));
                    format!(
                        "{:02X}{:02X}{:02X}",
                        ((x[0] * k + y[0] * (1.0 - k)) * 255.0).round() as u8,
                        ((x[1] * k + y[1] * (1.0 - k)) * 255.0).round() as u8,
                        ((x[2] * k + y[2] * (1.0 - k)) * 255.0).round() as u8
                    )
                };
                for bg in [page, tint] {
                    let muted = mix(ink, bg, 0.7);
                    assert!(
                        contrast(&muted, bg) >= 4.5,
                        "{who}: muted on {bg}: {:.2}",
                        contrast(&muted, bg)
                    );
                }
                assert!(
                    contrast(t.palette.accents[0], page) >= 3.0,
                    "{who}: accent1 on page {:.2}",
                    contrast(t.palette.accents[0], page)
                );
            }
        }
    }

    /// The point of a design language: no two themes may be the same
    /// composition in different colours. Two themes may share a header, but
    /// never the same header AND figure/card/compare arrangement.
    #[test]
    fn no_two_themes_share_a_design() {
        for (i, a) in THEMES.iter().enumerate() {
            for b in &THEMES[i + 1..] {
                let same = a.look == b.look && a.cover == b.cover && a.card == b.card;
                assert!(!same, "{} and {} are one design in two colours", a.id, b.id);
                if a.look.header == b.look.header {
                    let shared = [
                        a.look.stats == b.look.stats,
                        a.look.cards == b.look.cards,
                        a.look.timeline == b.look.timeline,
                        a.look.compare == b.look.compare,
                        a.cover == b.cover,
                        a.card == b.card,
                    ]
                    .iter()
                    .filter(|x| **x)
                    .count();
                    assert!(
                        shared <= 3,
                        "{} and {} share a header and {shared}/6 other choices",
                        a.id,
                        b.id
                    );
                }
            }
        }
        // And the catalogue spans the full range of header compositions.
        let mut headers: Vec<String> = THEMES
            .iter()
            .map(|t| format!("{:?}", t.look.header))
            .collect();
        headers.sort();
        headers.dedup();
        assert!(headers.len() >= 6, "{headers:?}");
    }

    #[test]
    fn contrast_helper_matches_wcag_reference_values() {
        assert!((contrast("000000", "FFFFFF") - 21.0).abs() < 0.01);
        assert!((contrast("777777", "FFFFFF") - 4.48).abs() < 0.02);
    }

    #[test]
    fn colour_references_resolve_like_powerpoint_would_paint_them() {
        use super::super::xml::Clr;
        let harbor = theme("harbor").unwrap();
        assert_eq!(harbor.resolve(&Clr::tx1()).0, harbor.palette.dk1);
        assert_eq!(harbor.resolve(&Clr::bg1()).0, harbor.palette.lt1);
        let aurora = theme("aurora").unwrap();
        // Dark deck: tx1 is the LIGHT colour.
        assert_eq!(aurora.resolve(&Clr::tx1()).0, aurora.palette.lt1);
        assert_eq!(aurora.resolve(&Clr::bg1()).0, aurora.palette.dk1);
        // Office "Lighter 80%" of blue is a pale blue, and alpha is reported, not applied.
        let (lighter, a) = harbor.resolve(&Clr::accent(1).lum(20, 80).alpha(40));
        assert!(
            contrast(&lighter, "FFFFFF") < 1.5,
            "a pale tint, got {lighter}"
        );
        assert!((a - 0.4).abs() < 1e-9);
        // A 50% ink over the page lands halfway.
        let mid = harbor.resolve_over(&Clr::tx1().alpha(50), harbor.palette.lt1);
        assert!(
            contrast(&mid, harbor.palette.lt1) > 1.5 && contrast(&mid, harbor.palette.lt1) < 12.0
        );
    }
}
