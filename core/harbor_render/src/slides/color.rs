//! DrawingML colours: parsing into a late-bound [`ColorSpec`] and
//! resolution against a theme and colour map, with the colour transforms
//! (`lumMod`, `lumOff`, `tint`, `shade`, `alpha`, ...) PowerPoint applies in
//! document order.
//!
//! A spec stays unresolved until painted because the same style can be used
//! under different colour maps (a dark layout overriding `bg1`/`tx1`) and
//! theme styles carry the `phClr` placeholder colour that only the using
//! shape supplies.

use std::collections::HashMap;

use roxmltree::Node;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ColorBase {
    Rgb([u8; 3]),
    /// `bg1`, `tx1`, `accent3`, `dk2`, ... (mapped through the colour map).
    Scheme(String),
    /// The placeholder colour a theme style leaves to its user.
    PhClr,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColorSpec {
    pub base: ColorBase,
    /// Transforms in document order: name and value (fractions of 1, hue in
    /// degrees).
    pub xf: Vec<(String, f64)>,
}

impl ColorSpec {
    pub fn rgb(rgb: [u8; 3]) -> Self {
        ColorSpec {
            base: ColorBase::Rgb(rgb),
            xf: Vec::new(),
        }
    }

    pub fn scheme(name: &str) -> Self {
        ColorSpec {
            base: ColorBase::Scheme(name.into()),
            xf: Vec::new(),
        }
    }
}

/// A resolved colour: `#RRGGBB` and alpha.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rgba {
    pub rgb: [u8; 3],
    pub a: f64,
}

impl Rgba {
    pub fn hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.rgb[0], self.rgb[1], self.rgb[2])
    }
}

/// The theme's twelve scheme colours, by their DrawingML names.
#[derive(Debug, Clone, Default)]
pub(crate) struct SchemeColors(pub HashMap<String, [u8; 3]>);

pub(crate) const COLOR_TAGS: [&str; 6] = [
    "srgbClr", "schemeClr", "sysClr", "prstClr", "hslClr", "scrgbClr",
];

/// The colour child of `parent` (`solidFill`, a gradient `gs`, `ln`'s fill,
/// ...), if it has one.
pub(crate) fn color_child(parent: Node) -> Option<ColorSpec> {
    parent
        .children()
        .find(|c| c.is_element() && COLOR_TAGS.contains(&c.tag_name().name()))
        .and_then(parse_color)
}

fn parse_color(n: Node) -> Option<ColorSpec> {
    let base = match n.tag_name().name() {
        "srgbClr" => ColorBase::Rgb(parse_hex(n.attribute("val")?)?),
        "sysClr" => {
            let fallback = match n.attribute("val") {
                Some("window") => [255, 255, 255],
                _ => [0, 0, 0],
            };
            ColorBase::Rgb(
                n.attribute("lastClr")
                    .and_then(parse_hex)
                    .unwrap_or(fallback),
            )
        }
        "schemeClr" => match n.attribute("val")? {
            "phClr" => ColorBase::PhClr,
            other => ColorBase::Scheme(other.to_string()),
        },
        "prstClr" => ColorBase::Rgb(preset_color(n.attribute("val")?)),
        "hslClr" => {
            let h = n.attribute("hue")?.parse::<f64>().ok()? / 60000.0;
            let s = n.attribute("sat")?.parse::<f64>().ok()? / 100000.0;
            let l = n.attribute("lum")?.parse::<f64>().ok()? / 100000.0;
            ColorBase::Rgb(hsl_to_rgb(h, s, l))
        }
        "scrgbClr" => {
            let c = |a: &str| -> Option<u8> {
                let v = n.attribute(a)?.parse::<f64>().ok()? / 100000.0;
                Some((linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8)
            };
            ColorBase::Rgb([c("r")?, c("g")?, c("b")?])
        }
        _ => return None,
    };
    let mut xf = Vec::new();
    for t in n.children().filter(|c| c.is_element()) {
        let name = t.tag_name().name();
        let Some(raw) = t.attribute("val").and_then(|v| v.parse::<f64>().ok()) else {
            continue;
        };
        match name {
            "hue" | "hueOff" => xf.push((name.to_string(), raw / 60000.0)),
            "lumMod" | "lumOff" | "lum" | "satMod" | "satOff" | "sat" | "hueMod" | "tint"
            | "shade" | "alpha" | "alphaMod" | "alphaOff" => {
                xf.push((name.to_string(), raw / 100000.0))
            }
            _ => {}
        }
    }
    Some(ColorSpec { base, xf })
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn preset_color(name: &str) -> [u8; 3] {
    match name {
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        "red" => [255, 0, 0],
        "green" => [0, 128, 0],
        "blue" => [0, 0, 255],
        "yellow" => [255, 255, 0],
        "gray" | "grey" => [128, 128, 128],
        "ltGray" | "lightGray" => [211, 211, 211],
        "dkGray" | "darkGray" => [169, 169, 169],
        "orange" => [255, 165, 0],
        "purple" => [128, 0, 128],
        "cyan" => [0, 255, 255],
        "magenta" => [255, 0, 255],
        _ => [0, 0, 0],
    }
}

/// Resolution context: scheme colours plus the active colour map
/// (`bg1` → `lt1`, ...).
pub(crate) struct ColorCtx<'a> {
    pub scheme: &'a SchemeColors,
    pub map: &'a HashMap<String, String>,
}

impl ColorCtx<'_> {
    /// Resolve a spec. `ph` is the placeholder colour a theme style leaves
    /// open (`phClr`); without one it resolves to black.
    pub fn resolve(&self, spec: &ColorSpec, ph: Option<&Rgba>) -> Rgba {
        let (mut rgb, mut alpha) = match &spec.base {
            ColorBase::Rgb(c) => (*c, 1.0),
            ColorBase::PhClr => ph
                .map(|p| (p.rgb, p.a))
                .unwrap_or(([0, 0, 0], 1.0)),
            ColorBase::Scheme(name) => {
                let mapped = self.map.get(name).map(String::as_str).unwrap_or(name);
                (
                    self.scheme.0.get(mapped).copied().unwrap_or([0, 0, 0]),
                    1.0,
                )
            }
        };
        for (name, v) in &spec.xf {
            match name.as_str() {
                "alpha" => alpha = *v,
                "alphaMod" => alpha *= *v,
                "alphaOff" => alpha += *v,
                "tint" => rgb = tint(rgb, *v),
                "shade" => rgb = shade(rgb, *v),
                _ => {
                    let (mut h, mut s, mut l) = rgb_to_hsl(rgb);
                    match name.as_str() {
                        "lumMod" => l *= *v,
                        "lumOff" => l += *v,
                        "lum" => l = *v,
                        "satMod" => s *= *v,
                        "satOff" => s += *v,
                        "sat" => s = *v,
                        "hueMod" => h *= *v,
                        "hueOff" => h += *v,
                        "hue" => h = *v,
                        _ => {}
                    }
                    rgb = hsl_to_rgb(h.rem_euclid(360.0), s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
                }
            }
        }
        Rgba {
            rgb,
            a: alpha.clamp(0.0, 1.0),
        }
    }
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// `tint` blends toward white in linear light (100% = unchanged).
fn tint(rgb: [u8; 3], t: f64) -> [u8; 3] {
    rgb.map(|c| {
        let lin = srgb_to_linear(c as f64 / 255.0);
        let out = lin * t + (1.0 - t);
        (linear_to_srgb(out.clamp(0.0, 1.0)) * 255.0).round() as u8
    })
}

/// `shade` blends toward black in linear light (100% = unchanged).
fn shade(rgb: [u8; 3], s: f64) -> [u8; 3] {
    rgb.map(|c| {
        let lin = srgb_to_linear(c as f64 / 255.0);
        (linear_to_srgb((lin * s).clamp(0.0, 1.0)) * 255.0).round() as u8
    })
}

/// Hue in degrees `0..360`, saturation and lightness in `0..=1`.
pub(crate) fn rgb_to_hsl(rgb: [u8; 3]) -> (f64, f64, f64) {
    let [r, g, b] = rgb.map(|c| c as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d.abs() < 1e-12 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < 1e-12 {
        ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-12 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

pub(crate) fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    if s.abs() < 1e-12 {
        let v = (l * 255.0).round() as u8;
        return [v, v, v];
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let hn = h / 360.0;
    [hue(hn + 1.0 / 3.0), hue(hn), hue(hn - 1.0 / 3.0)].map(|v| (v * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_for(accent1: [u8; 3]) -> (SchemeColors, HashMap<String, String>) {
        let mut m = HashMap::new();
        m.insert("accent1".to_string(), accent1);
        m.insert("lt1".to_string(), [255, 255, 255]);
        m.insert("dk1".to_string(), [0, 0, 0]);
        let map = [("bg1", "lt1"), ("tx1", "dk1")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        (SchemeColors(m), map)
    }

    /// PowerPoint's own HSL rounding differs from exact rounding by at
    /// most one step per channel, which no one can see.
    fn assert_within_one(c: &Rgba, want: [u8; 3]) {
        for (got, w) in c.rgb.iter().zip(want) {
            assert!(
                (*got as i32 - w as i32).abs() <= 1,
                "{} vs {:02X}{:02X}{:02X}",
                c.hex(),
                want[0],
                want[1],
                want[2]
            );
        }
    }

    fn spec(base: &str, xf: &[(&str, f64)]) -> ColorSpec {
        ColorSpec {
            base: ColorBase::Scheme(base.into()),
            xf: xf.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
        }
    }

    #[test]
    fn office_lighter_and_darker_swatches_match_powerpoint() {
        // Office accent 1 (4472C4): "Lighter 80%" is D9E2F3, "Darker 25%"
        // is 2F5597 — the values PowerPoint's own palette shows.
        let (scheme, map) = ctx_for([0x44, 0x72, 0xC4]);
        let ctx = ColorCtx {
            scheme: &scheme,
            map: &map,
        };
        let lighter = ctx.resolve(&spec("accent1", &[("lumMod", 0.2), ("lumOff", 0.8)]), None);
        assert_within_one(&lighter, [0xD9, 0xE2, 0xF3]);
        let darker = ctx.resolve(&spec("accent1", &[("lumMod", 0.75)]), None);
        assert_within_one(&darker, [0x2F, 0x55, 0x97]);
    }

    #[test]
    fn colour_map_and_alpha_and_placeholder() {
        let (scheme, mut map) = ctx_for([1, 2, 3]);
        let ctx = ColorCtx {
            scheme: &scheme,
            map: &map,
        };
        assert_eq!(ctx.resolve(&spec("bg1", &[]), None).hex(), "#FFFFFF");
        assert_eq!(ctx.resolve(&spec("tx1", &[]), None).hex(), "#000000");
        let a = ctx.resolve(&spec("tx1", &[("alpha", 0.4)]), None);
        assert!((a.a - 0.4).abs() < 1e-9);
        let ph = Rgba {
            rgb: [10, 20, 30],
            a: 1.0,
        };
        let p = ColorSpec {
            base: ColorBase::PhClr,
            xf: vec![("alphaMod".into(), 0.5)],
        };
        let r = ctx.resolve(&p, Some(&ph));
        assert_eq!(r.rgb, [10, 20, 30]);
        assert!((r.a - 0.5).abs() < 1e-9);
        // A dark layout swaps bg1/tx1.
        map.insert("bg1".into(), "dk1".into());
        map.insert("tx1".into(), "lt1".into());
        let ctx = ColorCtx {
            scheme: &scheme,
            map: &map,
        };
        assert_eq!(ctx.resolve(&spec("bg1", &[]), None).hex(), "#000000");
    }

    #[test]
    fn parses_xml_forms_and_ignores_garbage() {
        let doc = roxmltree::Document::parse(
            r#"<a:solidFill xmlns:a="x"><a:schemeClr val="accent2"><a:lumMod val="50000"/><a:alpha val="25000"/></a:schemeClr></a:solidFill>"#,
        )
        .unwrap();
        let c = color_child(doc.root_element()).unwrap();
        assert_eq!(c.base, ColorBase::Scheme("accent2".into()));
        assert_eq!(c.xf, vec![("lumMod".into(), 0.5), ("alpha".into(), 0.25)]);
        let bad = roxmltree::Document::parse(
            r#"<a:solidFill xmlns:a="x"><a:srgbClr val="ZZZ"/></a:solidFill>"#,
        )
        .unwrap();
        assert!(color_child(bad.root_element()).is_none());
    }
}
