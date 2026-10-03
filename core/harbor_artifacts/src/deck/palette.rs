//! Colour palettes: a separate choice from the design.
//!
//! A design (`look.rs`) decides layout and shape; a palette decides colour
//! (and whether the deck is dark or light). Any palette can be put on any
//! design: `Design.palette` overrides the theme's own. Every combination is
//! contrast-checked (`theme.rs` tests), so a swap never produces unreadable
//! text.

use super::theme::{Hex, Mode, Palette};

#[derive(Debug, Clone, Copy)]
pub struct PaletteSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub mood: &'static str,
    pub mode: Mode,
    pub palette: Palette,
}

pub const HARBOR: Palette = Palette {
            dk1: "0B1B2E",
            lt1: "FFFFFF",
            dk2: "12304F",
            lt2: "EEF3FA",
            accents: ["1F5FCC", "0EA5A4", "F59E0B", "7C5CFF", "E5484D", "5B6B7A"],
};

pub const AURORA: Palette = Palette {
            dk1: "070B18",
            lt1: "F4F6FF",
            dk2: "141C38",
            lt2: "C9D2F2",
            accents: ["7C5CFF", "22D3EE", "F472B6", "34D399", "FBBF24", "94A3B8"],
};

pub const IVORY: Palette = Palette {
            dk1: "1D2A24",
            lt1: "FFFDF8",
            dk2: "24443A",
            lt2: "F4EFE4",
            accents: ["24604F", "B8893B", "8C3B2B", "3F6C8F", "6B7F5A", "7A7467"],
};

pub const SLATE: Palette = Palette {
            dk1: "1B2430",
            lt1: "FFFFFF",
            dk2: "2B3A4D",
            lt2: "F1F4F7",
            accents: ["0F766E", "2563EB", "D97706", "DC2626", "16A34A", "64748B"],
};

pub const SUNRISE: Palette = Palette {
            dk1: "231815",
            lt1: "FFFBF7",
            dk2: "3B2A24",
            lt2: "FFEFE3",
            accents: ["FF5A36", "FFB020", "8B5CF6", "06B6D4", "22C55E", "F43F5E"],
};

pub const OBSIDIAN: Palette = Palette {
            dk1: "0A0A0A",
            lt1: "FAFAFA",
            dk2: "1A1A1A",
            lt2: "D4D4D4",
            accents: ["C6F432", "38BDF8", "FB7185", "A78BFA", "FACC15", "A3A3A3"],
};

pub const OCEAN: Palette = Palette {
            dk1: "0B2530",
            lt1: "FFFFFF",
            dk2: "0E4A5C",
            lt2: "E8F4F7",
            accents: ["0891B2", "0E7490", "F59E0B", "10B981", "6366F1", "64748B"],
};

pub const FOREST: Palette = Palette {
            dk1: "14261C",
            lt1: "FBFAF5",
            dk2: "1F4D36",
            lt2: "EEF0E3",
            accents: ["2F7D4F", "D9A441", "B5533C", "4F7CAC", "7A9E5D", "80806F"],
};

pub const MIDNIGHT_GOLD: Palette = Palette {
            dk1: "0B1220",
            lt1: "F8F5EE",
            dk2: "16213A",
            lt2: "D9D2C0",
            accents: ["D4A94F", "7FB1E8", "E0B1A0", "9FD3B8", "C9CED8", "B08D57"],
};

pub const PLAYGROUND: Palette = Palette {
            dk1: "1E1B3A",
            lt1: "FFFFFF",
            dk2: "3730A3",
            lt2: "F3F0FF",
            accents: ["6D4AFF", "FF6B6B", "FFC233", "2DD4BF", "3B82F6", "EC4899"],
};

pub const MONO: Palette = Palette {
            dk1: "111111",
            lt1: "FFFFFF",
            dk2: "333333",
            lt2: "F2F2F2",
            accents: ["E11D48", "111111", "6B7280", "9CA3AF", "D1D5DB", "F59E0B"],
};

pub const TERRACOTTA: Palette = Palette {
            dk1: "2B1D17",
            lt1: "FFFBF6",
            dk2: "5A3A2C",
            lt2: "F6EADF",
            accents: ["C2562F", "2F6F73", "D9A441", "6B4F8A", "8C9A5B", "8A7B70"],
};

pub const ROYAL: Palette = Palette {
            dk1: "1A1033",
            lt1: "FFFFFF",
            dk2: "2D1B5E",
            lt2: "E4D9FF",
            accents: ["B06BFF", "FF4FA3", "22D3EE", "FFB020", "34D399", "A1A1AA"],
};

pub const BLUEPRINT: Palette = Palette {
            dk1: "071A33",
            lt1: "EAF2FF",
            dk2: "0D2B52",
            lt2: "B7CCEA",
            accents: ["4DA3FF", "5EEAD4", "FDE047", "FB923C", "F87171", "94A3B8"],
};

pub const GRAPHITE: Palette = Palette {
    dk1: "1C1C1E",
    lt1: "F5F5F7",
    dk2: "2C2C2E",
    lt2: "D1D1D6",
    accents: ["FF9F0A", "64D2FF", "30D158", "FF453A", "BF5AF2", "8E8E93"],
};

pub const ROSE: Palette = Palette {
    dk1: "2B1620",
    lt1: "FFF8FA",
    dk2: "5A2A3F",
    lt2: "FBE8EE",
    accents: ["D6336C", "7048E8", "F59F00", "2B8A9E", "E8590C", "868E96"],
};

pub const MINT: Palette = Palette {
    dk1: "0F2A24",
    lt1: "F6FFFC",
    dk2: "14524A",
    lt2: "E3F6F0",
    accents: ["0B8F6A", "1C7ED6", "F59F00", "E64980", "7950F2", "868E96"],
};

pub const SAND: Palette = Palette {
    dk1: "14213D",
    lt1: "FBF8F1",
    dk2: "1B3A6B",
    lt2: "F0E9D8",
    accents: ["1B3A6B", "C8963E", "B5483B", "3B7A57", "6F5BA8", "8A857A"],
};

pub const INDIGO: Palette = Palette {
    dk1: "1B1B3A",
    lt1: "FFFFFF",
    dk2: "312E81",
    lt2: "EEF0FF",
    accents: ["4F46E5", "0EA5E9", "F59E0B", "10B981", "EC4899", "64748B"],
};

pub const EMBER: Palette = Palette {
    dk1: "120C0A",
    lt1: "FFF6F0",
    dk2: "2A1712",
    lt2: "E7C9B8",
    accents: ["FF6B35", "FFB400", "E63946", "2EC4B6", "9D4EDD", "A8A29E"],
};

pub const PALETTES: &[PaletteSpec] = &[
    PaletteSpec { id: "harbor", name: "Harbor Blue", mood: "Clean corporate blue", mode: Mode::Light, palette: HARBOR },
    PaletteSpec { id: "aurora", name: "Aurora", mood: "Violet and cyan on near-black", mode: Mode::Dark, palette: AURORA },
    PaletteSpec { id: "ivory", name: "Ivory & Pine", mood: "Warm paper, pine green and gold", mode: Mode::Light, palette: IVORY },
    PaletteSpec { id: "slate", name: "Slate", mood: "Calm greys with signal colours", mode: Mode::Light, palette: SLATE },
    PaletteSpec { id: "sunrise", name: "Sunrise", mood: "Coral and amber on cream", mode: Mode::Light, palette: SUNRISE },
    PaletteSpec { id: "obsidian", name: "Obsidian", mood: "Black, white and electric lime", mode: Mode::Dark, palette: OBSIDIAN },
    PaletteSpec { id: "ocean", name: "Ocean", mood: "Deep teal and open water", mode: Mode::Light, palette: OCEAN },
    PaletteSpec { id: "forest", name: "Forest", mood: "Greens and sand", mode: Mode::Light, palette: FOREST },
    PaletteSpec { id: "midnight", name: "Midnight Gold", mood: "Navy and brushed gold", mode: Mode::Dark, palette: MIDNIGHT_GOLD },
    PaletteSpec { id: "playground", name: "Playground", mood: "Bright violet, coral and sun", mode: Mode::Light, palette: PLAYGROUND },
    PaletteSpec { id: "mono", name: "Mono", mood: "Black, white and one red", mode: Mode::Light, palette: MONO },
    PaletteSpec { id: "terracotta", name: "Terracotta", mood: "Earthy clay and teal", mode: Mode::Light, palette: TERRACOTTA },
    PaletteSpec { id: "royal", name: "Royal", mood: "Deep violet and hot pink", mode: Mode::Dark, palette: ROYAL },
    PaletteSpec { id: "blueprint", name: "Blueprint", mood: "Drafting blue and cyan", mode: Mode::Dark, palette: BLUEPRINT },
    PaletteSpec { id: "graphite", name: "Graphite", mood: "Neutral dark with amber", mode: Mode::Dark, palette: GRAPHITE },
    PaletteSpec { id: "rose", name: "Rose", mood: "Soft rose, plum and gold", mode: Mode::Light, palette: ROSE },
    PaletteSpec { id: "mint", name: "Mint", mood: "Fresh mint and deep green", mode: Mode::Light, palette: MINT },
    PaletteSpec { id: "sand", name: "Sand & Navy", mood: "Warm sand with navy and brass", mode: Mode::Light, palette: SAND },
    PaletteSpec { id: "indigo", name: "Indigo", mood: "Confident indigo and sky", mode: Mode::Light, palette: INDIGO },
    PaletteSpec { id: "ember", name: "Ember", mood: "Warm dark with orange fire", mode: Mode::Dark, palette: EMBER },
];

pub fn palette(id: &str) -> Option<&'static PaletteSpec> {
    PALETTES.iter().find(|p| p.id == id)
}

#[allow(dead_code)]
fn _hex(_: Hex) {}
