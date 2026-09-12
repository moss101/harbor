//! Field-level last-writer-wins: only explicitly safe fields.
//!
//! 11_Sync_Protocol.md: "Only appearance theme, display density and UI
//! language use field-level LWW. Privacy, capabilities, model routing
//! locks, approvals, enrollment and key settings never use LWW."

use chrono::{DateTime, Utc};

/// Fields eligible for LWW merge — the complete, closed set.
pub const LWW_SAFE_FIELDS: [&str; 3] = ["appearance.theme", "display.density", "ui.language"];

pub fn is_lww_safe_field(name: &str) -> bool {
    LWW_SAFE_FIELDS.contains(&name)
}

#[derive(Debug, Clone, PartialEq)]
pub struct LwwValue {
    pub value: String,
    /// Hybrid logical clock of the write.
    pub writer_hlc: (u64, u32),
    pub writer_device: String,
}

/// Marker enumerating the safe fields (compile-time intent for callers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LwwField {
    AppearanceTheme,
    DisplayDensity,
    UiLanguage,
}

impl LwwField {
    pub fn field_name(&self) -> &'static str {
        match self {
            LwwField::AppearanceTheme => "appearance.theme",
            LwwField::DisplayDensity => "display.density",
            LwwField::UiLanguage => "ui.language",
        }
    }

    /// Parse a field name into the safe marker; anything else returns None
    /// and MUST NOT enter the LWW path.
    pub fn from_field_name(name: &str) -> Option<LwwField> {
        match name {
            "appearance.theme" => Some(LwwField::AppearanceTheme),
            "display.density" => Some(LwwField::DisplayDensity),
            "ui.language" => Some(LwwField::UiLanguage),
            _ => None,
        }
    }
}

/// Merge one LWW-safe field: highest HLC wins; identical HLC resolves by
/// device id (deterministic on both peers).
pub fn lww_merge(local: Option<LwwValue>, remote: LwwValue) -> LwwValue {
    match local {
        None => remote,
        Some(l) => {
            if remote.writer_hlc > l.writer_hlc
                || (remote.writer_hlc == l.writer_hlc && remote.writer_device > l.writer_device)
            {
                remote
            } else {
                l
            }
        }
    }
}

/// Unused placeholder to keep chrono import if needed later.
pub fn _now(_: DateTime<Utc>) {}
