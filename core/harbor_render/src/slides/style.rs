//! Fills, lines, effects and the theme that supplies their defaults.
//!
//! Everything is parsed into owned, still-unresolved specs (colours stay
//! late-bound, see `color.rs`) and resolved to scene paint at the end, under
//! the colour map of the slide being drawn.

use std::collections::HashMap;

use roxmltree::Node;

use super::color::{color_child, ColorCtx, ColorSpec, Rgba, SchemeColors};
use crate::scene::{LineStyle, Paint, Shadow, Stop};

const DML_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FillSpec {
    None,
    Solid(ColorSpec),
    Grad {
        stops: Vec<(f64, ColorSpec)>,
        /// Degrees clockwise from +x; `None` for a path (radial) gradient.
        angle: Option<f64>,
        /// Path-gradient centre as fractions.
        center: (f64, f64),
    },
    /// Picture fill: relationship id of the image.
    Blip(String),
    /// `grpFill`: the enclosing group's fill (approximated as no fill).
    Group,
}

pub(crate) const FILL_TAGS: [&str; 6] = [
    "noFill",
    "solidFill",
    "gradFill",
    "blipFill",
    "pattFill",
    "grpFill",
];

/// The fill child of a property container, if it states one.
pub(crate) fn fill_child(parent: Node) -> Option<FillSpec> {
    let n = parent
        .children()
        .find(|c| c.is_element() && FILL_TAGS.contains(&c.tag_name().name()))?;
    parse_fill(n)
}

pub(crate) fn parse_fill(n: Node) -> Option<FillSpec> {
    match n.tag_name().name() {
        "noFill" => Some(FillSpec::None),
        "grpFill" => Some(FillSpec::Group),
        "solidFill" => color_child(n).map(FillSpec::Solid),
        "pattFill" => n
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "fgClr")
            .and_then(color_child)
            .map(FillSpec::Solid),
        "blipFill" => {
            let blip = n
                .children()
                .find(|c| c.is_element() && c.tag_name().name() == "blip")?;
            blip.attribute((DML_R, "embed"))
                .map(|r| FillSpec::Blip(r.to_string()))
        }
        "gradFill" => {
            let mut stops: Vec<(f64, ColorSpec)> = n
                .descendants()
                .filter(|c| c.is_element() && c.tag_name().name() == "gs")
                .filter_map(|gs| {
                    let pos = gs.attribute("pos")?.parse::<f64>().ok()? / 100000.0;
                    Some((pos.clamp(0.0, 1.0), color_child(gs)?))
                })
                .collect();
            if stops.is_empty() {
                return None;
            }
            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
            let lin = n
                .children()
                .find(|c| c.is_element() && c.tag_name().name() == "lin");
            let path = n
                .children()
                .find(|c| c.is_element() && c.tag_name().name() == "path");
            let (angle, center) = match (lin, path) {
                (Some(l), _) => (
                    Some(
                        l.attribute("ang")
                            .and_then(|a| a.parse::<f64>().ok())
                            .unwrap_or(0.0)
                            / 60000.0,
                    ),
                    (0.5, 0.5),
                ),
                (None, Some(p)) => {
                    let rect = p
                        .children()
                        .find(|c| c.is_element() && c.tag_name().name() == "fillToRect");
                    let g = |a: &str| {
                        rect.and_then(|r| r.attribute(a))
                            .and_then(|v| v.parse::<f64>().ok())
                            .unwrap_or(50000.0)
                    };
                    (
                        None,
                        (
                            (g("l") + (100000.0 - g("r"))) / 2.0 / 100000.0,
                            (g("t") + (100000.0 - g("b"))) / 2.0 / 100000.0,
                        ),
                    )
                }
                // No direction: PowerPoint draws a left→right gradient.
                (None, None) => (Some(0.0), (0.5, 0.5)),
            };
            Some(FillSpec::Grad {
                stops,
                angle,
                center,
            })
        }
        _ => None,
    }
}

pub(crate) fn resolve_fill(spec: &FillSpec, ctx: &ColorCtx, ph: Option<&Rgba>) -> Paint {
    match spec {
        FillSpec::None | FillSpec::Group | FillSpec::Blip(_) => Paint::None,
        FillSpec::Solid(c) => {
            let c = ctx.resolve(c, ph);
            Paint::Solid { c: c.hex(), a: c.a }
        }
        FillSpec::Grad {
            stops,
            angle,
            center,
        } => {
            let stops: Vec<Stop> = stops
                .iter()
                .map(|(p, c)| {
                    let c = ctx.resolve(c, ph);
                    Stop {
                        p: *p,
                        c: c.hex(),
                        a: c.a,
                    }
                })
                .collect();
            match angle {
                Some(a) => Paint::Linear { angle: *a, stops },
                None => Paint::Radial {
                    cx: center.0,
                    cy: center.1,
                    stops,
                },
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LineSpec {
    pub w: Option<f64>,
    pub fill: Option<FillSpec>,
    pub dash: Option<String>,
    pub cap: Option<String>,
    pub head: Option<String>,
    pub tail: Option<String>,
}

impl LineSpec {
    /// `over` wins field by field.
    pub fn merged(&self, over: &LineSpec) -> LineSpec {
        LineSpec {
            w: over.w.or(self.w),
            fill: over.fill.clone().or_else(|| self.fill.clone()),
            dash: over.dash.clone().or_else(|| self.dash.clone()),
            cap: over.cap.clone().or_else(|| self.cap.clone()),
            head: over.head.clone().or_else(|| self.head.clone()),
            tail: over.tail.clone().or_else(|| self.tail.clone()),
        }
    }
}

pub(crate) fn parse_line(ln: Node) -> LineSpec {
    let mut spec = LineSpec {
        w: ln.attribute("w").and_then(|v| v.parse().ok()),
        cap: ln.attribute("cap").map(|c| match c {
            "rnd" => "round".to_string(),
            "sq" => "square".to_string(),
            _ => "flat".to_string(),
        }),
        ..Default::default()
    };
    spec.fill = fill_child(ln);
    for c in ln.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "prstDash" => spec.dash = c.attribute("val").map(str::to_string),
            "headEnd" => spec.head = arrow(c),
            "tailEnd" => spec.tail = arrow(c),
            _ => {}
        }
    }
    spec
}

fn arrow(n: Node) -> Option<String> {
    match n.attribute("type") {
        Some("none") | None => None,
        Some(t) => Some(t.to_string()),
    }
}

/// Resolve to a drawable line; `None` when there is no visible stroke.
pub(crate) fn resolve_line(
    spec: &LineSpec,
    ctx: &ColorCtx,
    ph: Option<&Rgba>,
) -> Option<LineStyle> {
    let fill = spec.fill.as_ref()?;
    let paint = resolve_fill(fill, ctx, ph);
    if matches!(paint, Paint::None) {
        return None;
    }
    Some(LineStyle {
        w: spec.w.unwrap_or(9525.0),
        paint,
        dash: spec.dash.clone().unwrap_or_else(|| "solid".into()),
        cap: spec.cap.clone().unwrap_or_else(|| "flat".into()),
        head: spec.head.clone(),
        tail: spec.tail.clone(),
    })
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct EffectSpec {
    pub shadow: Option<ShadowSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ShadowSpec {
    pub blur: f64,
    pub dist: f64,
    /// Degrees clockwise from +x.
    pub dir: f64,
    pub color: ColorSpec,
}

pub(crate) fn parse_effects(list: Node) -> EffectSpec {
    let shadow = list
        .descendants()
        .find(|c| c.is_element() && c.tag_name().name() == "outerShdw")
        .map(|s| ShadowSpec {
            blur: s
                .attribute("blurRad")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0),
            dist: s
                .attribute("dist")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0),
            dir: s
                .attribute("dir")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
                / 60000.0,
            color: color_child(s).unwrap_or_else(|| ColorSpec::rgb([0, 0, 0])),
        });
    EffectSpec { shadow }
}

pub(crate) fn resolve_shadow(spec: &ShadowSpec, ctx: &ColorCtx, ph: Option<&Rgba>) -> Shadow {
    let c = ctx.resolve(&spec.color, ph);
    let rad = spec.dir.to_radians();
    Shadow {
        dx: spec.dist * rad.cos(),
        dy: spec.dist * rad.sin(),
        blur: spec.blur,
        c: c.hex(),
        a: c.a,
    }
}

/// A parsed theme: scheme colours, fonts and the three style lists shapes
/// reference by index (`fillRef idx="2"`).
#[derive(Debug, Clone, Default)]
pub(crate) struct Theme {
    pub scheme: SchemeColors,
    pub major_latin: String,
    pub minor_latin: String,
    pub major_cs: Option<String>,
    pub minor_cs: Option<String>,
    pub fills: Vec<FillSpec>,
    pub bg_fills: Vec<FillSpec>,
    pub lines: Vec<LineSpec>,
    pub effects: Vec<EffectSpec>,
}

pub(crate) fn parse_theme(xml: &str) -> Theme {
    let mut theme = Theme {
        major_latin: "Calibri".into(),
        minor_latin: "Calibri".into(),
        ..Default::default()
    };
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return theme;
    };
    fn by<'a, 'i>(parent: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
        parent
            .descendants()
            .find(|c| c.is_element() && c.tag_name().name() == name)
    }
    if let Some(cs) = by(doc.root_element(), "clrScheme") {
        let mut m: HashMap<String, [u8; 3]> = HashMap::new();
        for slot in cs.children().filter(|c| c.is_element()) {
            if let Some(spec) = color_child(slot) {
                let ctx_scheme = SchemeColors::default();
                let map = HashMap::new();
                let ctx = ColorCtx {
                    scheme: &ctx_scheme,
                    map: &map,
                };
                m.insert(
                    slot.tag_name().name().to_string(),
                    ctx.resolve(&spec, None).rgb,
                );
            }
        }
        theme.scheme = SchemeColors(m);
    }
    if let Some(fs) = by(doc.root_element(), "fontScheme") {
        let face = |group: &str, script: &str| {
            by(fs, group)
                .and_then(|g| by(g, script))
                .and_then(|n| n.attribute("typeface"))
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        if let Some(f) = face("majorFont", "latin") {
            theme.major_latin = f;
        }
        if let Some(f) = face("minorFont", "latin") {
            theme.minor_latin = f;
        }
        theme.major_cs = face("majorFont", "cs");
        theme.minor_cs = face("minorFont", "cs");
    }
    if let Some(fmt) = by(doc.root_element(), "fmtScheme") {
        let list = |name: &str| by(fmt, name);
        if let Some(l) = list("fillStyleLst") {
            theme.fills = l
                .children()
                .filter(|c| c.is_element())
                .map(|c| parse_fill(c).unwrap_or(FillSpec::None))
                .collect();
        }
        if let Some(l) = list("bgFillStyleLst") {
            theme.bg_fills = l
                .children()
                .filter(|c| c.is_element())
                .map(|c| parse_fill(c).unwrap_or(FillSpec::None))
                .collect();
        }
        if let Some(l) = list("lnStyleLst") {
            theme.lines = l
                .children()
                .filter(|c| c.is_element() && c.tag_name().name() == "ln")
                .map(parse_line)
                .collect();
        }
        if let Some(l) = list("effectStyleLst") {
            theme.effects = l
                .children()
                .filter(|c| c.is_element())
                .map(parse_effects)
                .collect();
        }
    }
    theme
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_angles_and_radial_centres() {
        let doc = roxmltree::Document::parse(
            r#"<p xmlns:a="x"><a:gradFill><a:gsLst><a:gs pos="100000"><a:srgbClr val="000000"/></a:gs><a:gs pos="0"><a:srgbClr val="FFFFFF"/></a:gs></a:gsLst><a:lin ang="5400000"/></a:gradFill></p>"#,
        )
        .unwrap();
        match fill_child(doc.root_element()).unwrap() {
            FillSpec::Grad { stops, angle, .. } => {
                assert_eq!(angle, Some(90.0));
                assert_eq!(stops[0].0, 0.0, "stops are sorted by position");
            }
            other => panic!("{other:?}"),
        }
        let radial = roxmltree::Document::parse(
            r#"<p xmlns:a="x"><a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FFFFFF"/></a:gs></a:gsLst><a:path path="circle"><a:fillToRect l="25000" t="75000" r="75000" b="25000"/></a:path></a:gradFill></p>"#,
        )
        .unwrap();
        match fill_child(radial.root_element()).unwrap() {
            FillSpec::Grad { angle, center, .. } => {
                assert_eq!(angle, None);
                assert_eq!(center, (0.25, 0.75));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn theme_fonts_colors_and_styles() {
        let t = parse_theme(
            r#"<a:theme xmlns:a="x"><a:themeElements>
            <a:clrScheme name="t"><a:dk1><a:sysClr val="windowText" lastClr="111111"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:accent1><a:srgbClr val="1F5FCC"/></a:accent1></a:clrScheme>
            <a:fontScheme name="f"><a:majorFont><a:latin typeface="Georgia"/><a:cs typeface="Arial"/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:cs typeface=""/></a:minorFont></a:fontScheme>
            <a:fmtScheme name="s"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst><a:outerShdw blurRad="50800" dist="38100" dir="5400000"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle></a:effectStyleLst></a:fmtScheme>
            </a:themeElements></a:theme>"#,
        );
        assert_eq!(t.scheme.0["dk1"], [0x11, 0x11, 0x11]);
        assert_eq!(t.scheme.0["accent1"], [0x1F, 0x5F, 0xCC]);
        assert_eq!(t.major_latin, "Georgia");
        assert_eq!(t.major_cs.as_deref(), Some("Arial"));
        assert_eq!(t.minor_cs, None, "an empty typeface is not a font");
        assert_eq!(t.fills.len(), 1);
        assert_eq!(t.lines[0].w, Some(12700.0));
        let sh = t.effects[0].shadow.as_ref().unwrap();
        assert_eq!(sh.blur, 50800.0);
        assert!((sh.dir - 90.0).abs() < 1e-9);
    }

    #[test]
    fn hostile_theme_never_panics() {
        for bad in ["", "<not xml", "<a:theme xmlns:a=\"x\"/>"] {
            let t = parse_theme(bad);
            assert_eq!(t.minor_latin, "Calibri");
        }
    }
}
