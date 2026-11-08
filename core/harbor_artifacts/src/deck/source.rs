//! The deck's source, carried inside the file.
//!
//! A designed deck embeds the content and design it was made from
//! (`harbor/deck.json`, linked from the package root by a Harbor
//! relationship). That makes "change the theme" or "change the motion"
//! lossless: the deck is re-rendered from its content under the new design
//! instead of being reverse-engineered from slide XML. The part is plain
//! JSON, readable and bounded; PowerPoint ignores relationships it does not
//! know, and a deck that PowerPoint (or anything else) has re-saved no
//! longer carries it — then there is no source and no restyle, which the
//! caller reports honestly rather than guessing.

use std::io::{Cursor, Read};

use base64::Engine as _;
use serde_json::{json, Value};

use super::model::{Block, Design, DesignedDeck, DesignedSlide, LayoutKind};
use super::motion::MotionPreset;
use crate::pptx::{ChartKind, ChartSpec, SlideImage};

pub const SOURCE_PART: &str = "harbor/deck.json";
pub const SOURCE_REL: &str = "http://schemas.harbor.dev/2026/relationships/deck-source";
const MAX_SOURCE_BYTES: u64 = 24 * 1024 * 1024;
const SOURCE_SCHEMA: &str = "harbor.deck-source/v1";

fn chart_kind_id(k: ChartKind) -> &'static str {
    match k {
        ChartKind::Bar => "bar",
        ChartKind::Line => "line",
        ChartKind::Pie => "pie",
        ChartKind::Scatter => "scatter",
    }
}

fn chart_kind_parse(s: &str) -> Option<ChartKind> {
    Some(match s {
        "bar" => ChartKind::Bar,
        "line" => ChartKind::Line,
        "pie" => ChartKind::Pie,
        "scatter" => ChartKind::Scatter,
        _ => return None,
    })
}

pub fn to_json(deck: &DesignedDeck) -> Value {
    let kinds = deck.resolved_layouts();
    let slides: Vec<Value> = deck
        .slides
        .iter()
        .zip(kinds.iter())
        .map(|(s, kind)| {
            json!({
                "title": s.title,
                "kicker": s.kicker,
                // The RESOLVED layout, so a restyle never re-guesses it.
                "layout": kind.id(),
                "bullets": s.bullets,
                "blocks": s.blocks.iter().map(|b| json!({
                    "heading": b.heading, "text": b.text, "value": b.value, "label": b.label,
                })).collect::<Vec<_>>(),
                "notes": s.notes,
                "attribution": s.attribution,
                "chart": s.chart.as_ref().map(|c| json!({
                    "kind": chart_kind_id(c.kind),
                    "title": c.title,
                    "categories": c.categories,
                    "series": c.series.iter().map(|(n, v)| json!({"name": n, "values": v})).collect::<Vec<_>>(),
                })),
                "image": s.image.as_ref().map(|i| json!({
                    "name": i.name,
                    "extension": i.extension,
                    "data_b64": base64::engine::general_purpose::STANDARD.encode(&i.bytes),
                })),
            })
        })
        .collect();
    json!({
        "schema": SOURCE_SCHEMA,
        "title": deck.title,
        "subtitle": deck.subtitle,
        "design": {
            "theme": deck.design.theme,
            "palette": deck.design.palette,
            "motion": deck.design.motion.id(),
            "rtl": deck.design.rtl,
            "footer": deck.design.footer,
            "slide_numbers": deck.design.slide_numbers,
            "progressive_bullets": deck.design.progressive_bullets,
        },
        "slides": slides,
    })
}

fn str_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn from_json(v: &Value) -> Result<DesignedDeck, String> {
    if v.get("schema").and_then(Value::as_str) != Some(SOURCE_SCHEMA) {
        return Err("not a Harbor deck source".into());
    }
    let d = v.get("design").ok_or("missing design")?;
    let theme = d.get("theme").and_then(Value::as_str).unwrap_or("harbor");
    if super::theme::theme(theme).is_none() {
        return Err(format!("the deck names an unknown theme {theme:?}"));
    }
    let design = Design {
        theme: theme.to_string(),
        palette: match d.get("palette").and_then(Value::as_str) {
            Some(p) if super::palette::palette(p).is_some() => Some(p.to_string()),
            Some(p) => return Err(format!("the deck names an unknown palette {p:?}")),
            None => None,
        },
        motion: d
            .get("motion")
            .and_then(Value::as_str)
            .and_then(MotionPreset::parse)
            .unwrap_or(MotionPreset::Subtle),
        rtl: d.get("rtl").and_then(Value::as_bool).unwrap_or(false),
        footer: d.get("footer").and_then(Value::as_str).map(str::to_string),
        slide_numbers: d
            .get("slide_numbers")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        progressive_bullets: d
            .get("progressive_bullets")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    let mut slides = Vec::new();
    for s in v
        .get("slides")
        .and_then(Value::as_array)
        .ok_or("missing slides")?
    {
        if slides.len() >= 200 {
            return Err("too many slides".into());
        }
        let chart = match s.get("chart").filter(|c| !c.is_null()) {
            Some(c) => Some(ChartSpec {
                kind: c
                    .get("kind")
                    .and_then(Value::as_str)
                    .and_then(chart_kind_parse)
                    .ok_or("unknown chart kind")?,
                title: c
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                categories: str_list(c.get("categories")),
                series: c
                    .get("series")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|s| {
                                (
                                    s.get("name")
                                        .and_then(Value::as_str)
                                        .unwrap_or("")
                                        .to_string(),
                                    s.get("values")
                                        .and_then(Value::as_array)
                                        .map(|v| v.iter().filter_map(Value::as_f64).collect())
                                        .unwrap_or_default(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }),
            None => None,
        };
        let image = match s.get("image").filter(|i| !i.is_null()) {
            Some(i) => Some(SlideImage {
                name: i
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("image")
                    .to_string(),
                extension: match i.get("extension").and_then(Value::as_str) {
                    Some("png") => "png".to_string(),
                    Some("jpg") => "jpg".to_string(),
                    _ => return Err("unsupported image type".into()),
                },
                bytes: base64::engine::general_purpose::STANDARD
                    .decode(i.get("data_b64").and_then(Value::as_str).unwrap_or(""))
                    .map_err(|e| format!("image data: {e}"))?,
            }),
            None => None,
        };
        slides.push(DesignedSlide {
            title: s
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            kicker: s.get("kicker").and_then(Value::as_str).map(str::to_string),
            layout: s
                .get("layout")
                .and_then(Value::as_str)
                .and_then(LayoutKind::parse),
            bullets: str_list(s.get("bullets")),
            blocks: s
                .get("blocks")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|b| Block {
                            heading: b
                                .get("heading")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            text: str_list(b.get("text")),
                            value: b.get("value").and_then(Value::as_str).map(str::to_string),
                            label: b.get("label").and_then(Value::as_str).map(str::to_string),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            notes: s.get("notes").and_then(Value::as_str).map(str::to_string),
            chart,
            image,
            attribution: s
                .get("attribution")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    Ok(DesignedDeck {
        title: v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        subtitle: v
            .get("subtitle")
            .and_then(Value::as_str)
            .map(str::to_string),
        slides,
        design,
    })
}

/// The source embedded in a package, if it has one.
pub fn read_source(bytes: &[u8]) -> Option<DesignedDeck> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    // The package must LINK the part: a stray file by that name is not a
    // source the deck declared.
    let rels = {
        let mut f = archive.by_name("_rels/.rels").ok()?;
        let mut s = String::new();
        f.read_to_string(&mut s).ok()?;
        s
    };
    if !rels.contains(SOURCE_REL) {
        return None;
    }
    let f = archive.by_name(SOURCE_PART).ok()?;
    if f.size() > MAX_SOURCE_BYTES {
        return None;
    }
    let mut buf = Vec::new();
    f.take(MAX_SOURCE_BYTES + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > MAX_SOURCE_BYTES {
        return None;
    }
    from_json(&serde_json::from_slice::<Value>(&buf).ok()?).ok()
}

/// Re-render a Harbor-made deck under a new design. `None` keeps a choice.
/// Errors name why a deck cannot be restyled.
pub fn restyle(
    bytes: &[u8],
    theme: Option<&str>,
    palette: Option<&str>,
    motion: Option<MotionPreset>,
    rtl: Option<bool>,
) -> Result<Vec<u8>, String> {
    let mut deck = read_source(bytes).ok_or_else(|| {
        "this deck does not carry its Harbor design source (it was not made by Harbor, or another app saved over it), so its design cannot be changed here".to_string()
    })?;
    if let Some(t) = theme {
        if super::theme::theme(t).is_none() {
            return Err(format!("unknown theme {t:?}"));
        }
        deck.design.theme = t.to_string();
    }
    if let Some(p) = palette {
        // An empty id returns to the theme's own colours.
        if p.is_empty() {
            deck.design.palette = None;
        } else if super::palette::palette(p).is_some() {
            deck.design.palette = Some(p.to_string());
        } else {
            return Err(format!("unknown palette {p:?}"));
        }
    }
    if let Some(m) = motion {
        deck.design.motion = m;
    }
    if let Some(r) = rtl {
        deck.design.rtl = r;
    }
    deck.to_pptx_bytes().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::starter::starter_deck;
    use super::*;

    #[test]
    fn the_source_round_trips_exactly() {
        for purpose in ["pitch_deck", "project_report", "board"] {
            let deck = starter_deck(purpose, None, None, false, None).unwrap();
            let json = to_json(&deck);
            let back = from_json(&json).unwrap();
            assert_eq!(back.title, deck.title);
            assert_eq!(back.slides.len(), deck.slides.len());
            // Layouts are stored RESOLVED, so the round trip is stable.
            assert_eq!(back.resolved_layouts(), deck.resolved_layouts());
            assert_eq!(to_json(&back), json, "{purpose}");
        }
    }

    #[test]
    fn a_designed_file_carries_its_source_and_restyle_changes_only_the_design() {
        let deck = starter_deck("pitch_deck", None, None, false, Some("Acme")).unwrap();
        let bytes = deck.to_pptx_bytes().unwrap();
        let read = read_source(&bytes).expect("the file embeds its source");
        assert_eq!(read.design.theme, "aurora");

        let out = restyle(&bytes, Some("ivory"), None, Some(MotionPreset::None), None).unwrap();
        let after = read_source(&out).unwrap();
        assert_eq!(after.design.theme, "ivory");
        assert_eq!(after.design.motion, MotionPreset::None);
        // Content untouched; layouts unchanged.
        assert_eq!(after.slides.len(), deck.slides.len());
        assert_eq!(after.slides[1].title, deck.slides[1].title);
        assert_eq!(after.resolved_layouts(), deck.resolved_layouts());
        // The theme part really changed, and motion really went.
        let theme = {
            let mut ar = zip::ZipArchive::new(Cursor::new(&out)).unwrap();
            let mut s = String::new();
            ar.by_name("ppt/theme/theme1.xml")
                .unwrap()
                .read_to_string(&mut s)
                .unwrap();
            s
        };
        assert!(theme.contains("24604F"));
        let slide = {
            let mut ar = zip::ZipArchive::new(Cursor::new(&out)).unwrap();
            let mut s = String::new();
            ar.by_name("ppt/slides/slide3.xml")
                .unwrap()
                .read_to_string(&mut s)
                .unwrap();
            s
        };
        assert!(!slide.contains("<p:timing>") && !slide.contains("<p:transition"));
        // Restyle is itself deterministic.
        assert_eq!(
            out,
            restyle(&bytes, Some("ivory"), None, Some(MotionPreset::None), None).unwrap()
        );
    }

    #[test]
    fn decks_without_a_source_are_refused_with_a_reason_and_hostile_sources_are_ignored() {
        // A legacy deck has no source.
        let legacy = crate::pptx::PptxDeck {
            title: "Old".into(),
            slides: vec![crate::pptx::SlideContent {
                title: "A".into(),
                bullets: vec![],
                notes: None,
                chart: None,
                image: None,
            }],
        }
        .to_pptx_bytes()
        .unwrap();
        assert!(read_source(&legacy).is_none());
        let e = restyle(&legacy, Some("ivory"), None, None, None).unwrap_err();
        assert!(e.contains("does not carry"), "{e}");
        assert!(restyle(b"junk", None, None, None, None).is_err());
        // An unknown theme in a source is refused, not drawn.
        let mut json = to_json(&starter_deck("general", None, None, false, None).unwrap());
        json["design"]["theme"] = "neon".into();
        assert!(from_json(&json).is_err());
        json["schema"] = "other/v9".into();
        assert!(from_json(&json)
            .unwrap_err()
            .contains("not a Harbor deck source"));
    }

    #[test]
    fn charts_and_images_survive_the_trip() {
        let mut deck = starter_deck("quarterly_review", None, None, false, None).unwrap();
        deck.slides[2].image = Some(SlideImage {
            name: "shot".into(),
            extension: "png".into(),
            bytes: vec![1, 2, 3, 4],
        });
        let back = from_json(&to_json(&deck)).unwrap();
        assert_eq!(
            back.slides[2].image.as_ref().unwrap().bytes,
            vec![1, 2, 3, 4]
        );
        let chart_slide = deck.slides.iter().position(|s| s.chart.is_some()).unwrap();
        assert_eq!(
            back.slides[chart_slide].chart,
            deck.slides[chart_slide].chart
        );
    }
}
