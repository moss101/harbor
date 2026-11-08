//! Text bodies: list-style inheritance (presentation defaults → master text
//! styles → master placeholder → layout placeholder → the shape's own list
//! style → paragraph → run), resolved into scene paragraphs and runs.

use roxmltree::Node;

use super::color::{color_child, ColorCtx, ColorSpec, Rgba};
use super::style::Theme;
use crate::scene::{Bullet, Para, Run, TextBody};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RProps {
    /// Points.
    pub sz: Option<f64>,
    pub b: Option<bool>,
    pub i: Option<bool>,
    pub u: Option<bool>,
    pub strike: Option<bool>,
    pub fill: Option<ColorSpec>,
    pub latin: Option<String>,
    pub cs: Option<String>,
    pub baseline: Option<f64>,
    pub caps: Option<bool>,
    pub spc: Option<f64>,
}

impl RProps {
    pub fn merged(&self, over: &RProps) -> RProps {
        RProps {
            sz: over.sz.or(self.sz),
            b: over.b.or(self.b),
            i: over.i.or(self.i),
            u: over.u.or(self.u),
            strike: over.strike.or(self.strike),
            fill: over.fill.clone().or_else(|| self.fill.clone()),
            latin: over.latin.clone().or_else(|| self.latin.clone()),
            cs: over.cs.clone().or_else(|| self.cs.clone()),
            baseline: over.baseline.or(self.baseline),
            caps: over.caps.or(self.caps),
            spc: over.spc.or(self.spc),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Spacing {
    /// Fraction of the line (`spcPct`).
    Pct(f64),
    Pts(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BulletSpec {
    None,
    Char(String),
    Auto { ty: String, start: u32 },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PProps {
    pub mar_l: Option<f64>,
    pub indent: Option<f64>,
    pub algn: Option<String>,
    pub rtl: Option<bool>,
    pub ln: Option<Spacing>,
    pub spc_bef: Option<Spacing>,
    pub spc_aft: Option<Spacing>,
    pub bu: Option<BulletSpec>,
    pub bu_clr: Option<ColorSpec>,
    pub bu_sz_pct: Option<f64>,
    pub bu_font: Option<String>,
    pub def: RProps,
}

impl PProps {
    pub fn merged(&self, over: &PProps) -> PProps {
        PProps {
            mar_l: over.mar_l.or(self.mar_l),
            indent: over.indent.or(self.indent),
            algn: over.algn.clone().or_else(|| self.algn.clone()),
            rtl: over.rtl.or(self.rtl),
            ln: over.ln.clone().or_else(|| self.ln.clone()),
            spc_bef: over.spc_bef.clone().or_else(|| self.spc_bef.clone()),
            spc_aft: over.spc_aft.clone().or_else(|| self.spc_aft.clone()),
            bu: over.bu.clone().or_else(|| self.bu.clone()),
            bu_clr: over.bu_clr.clone().or_else(|| self.bu_clr.clone()),
            bu_sz_pct: over.bu_sz_pct.or(self.bu_sz_pct),
            bu_font: over.bu_font.clone().or_else(|| self.bu_font.clone()),
            def: self.def.merged(&over.def),
        }
    }
}

/// Nine levels of paragraph defaults (`a:lstStyle`, `p:titleStyle`, ...).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LevelStyles {
    pub levels: [PProps; 9],
}

pub(crate) fn parse_list_style(n: Node) -> LevelStyles {
    let mut out = LevelStyles::default();
    let base = n
        .children()
        .find(|c| c.is_element() && c.tag_name().name() == "defPPr")
        .map(parse_ppr)
        .unwrap_or_default();
    for lvl in 0..9 {
        out.levels[lvl] = base.clone();
    }
    for c in n.children().filter(|c| c.is_element()) {
        let name = c.tag_name().name();
        if let Some(num) = name
            .strip_prefix("lvl")
            .and_then(|r| r.strip_suffix("pPr"))
            .and_then(|d| d.parse::<usize>().ok())
        {
            if (1..=9).contains(&num) {
                out.levels[num - 1] = base.merged(&parse_ppr(c));
            }
        }
    }
    out
}

pub(crate) fn parse_ppr(n: Node) -> PProps {
    let f = |a: &str| n.attribute(a).and_then(|v| v.parse::<f64>().ok());
    let mut p = PProps {
        mar_l: f("marL"),
        indent: f("indent"),
        algn: n.attribute("algn").map(|a| match a {
            "ctr" => "ctr".to_string(),
            "r" => "r".to_string(),
            "just" | "justLow" | "dist" | "thaiDist" => "just".to_string(),
            _ => "l".to_string(),
        }),
        rtl: n.attribute("rtl").map(|v| v == "1" || v == "true"),
        ..Default::default()
    };
    for c in n.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "lnSpc" => p.ln = parse_spacing(c),
            "spcBef" => p.spc_bef = parse_spacing(c),
            "spcAft" => p.spc_aft = parse_spacing(c),
            "buNone" => p.bu = Some(BulletSpec::None),
            "buChar" => {
                if let Some(ch) = c.attribute("char") {
                    p.bu = Some(BulletSpec::Char(ch.to_string()));
                }
            }
            "buAutoNum" => {
                p.bu = Some(BulletSpec::Auto {
                    ty: c.attribute("type").unwrap_or("arabicPeriod").to_string(),
                    start: c
                        .attribute("startAt")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1),
                })
            }
            "buClr" => p.bu_clr = color_child(c),
            "buSzPct" => {
                p.bu_sz_pct = c
                    .attribute("val")
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|v| v / 100000.0)
            }
            "buFont" => p.bu_font = c.attribute("typeface").map(str::to_string),
            "defRPr" => p.def = parse_rpr(c),
            _ => {}
        }
    }
    p
}

fn parse_spacing(n: Node) -> Option<Spacing> {
    let c = n.children().find(|c| c.is_element())?;
    let v = c.attribute("val")?.parse::<f64>().ok()?;
    match c.tag_name().name() {
        "spcPct" => Some(Spacing::Pct(v / 100000.0)),
        "spcPts" => Some(Spacing::Pts(v / 100.0)),
        _ => None,
    }
}

pub(crate) fn parse_rpr(n: Node) -> RProps {
    let f = |a: &str| n.attribute(a).and_then(|v| v.parse::<f64>().ok());
    let flag = |a: &str| n.attribute(a).map(|v| v == "1" || v == "true");
    let mut r = RProps {
        sz: f("sz").map(|v| v / 100.0),
        b: flag("b"),
        i: flag("i"),
        u: n.attribute("u").map(|v| v != "none"),
        strike: n.attribute("strike").map(|v| v != "noStrike"),
        baseline: f("baseline").map(|v| v / 1000.0),
        caps: n.attribute("cap").map(|v| v != "none"),
        spc: f("spc").map(|v| v / 100.0),
        ..Default::default()
    };
    for c in n.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "solidFill" => r.fill = color_child(c),
            "latin" => r.latin = c.attribute("typeface").map(str::to_string),
            "cs" => r.cs = c.attribute("typeface").map(str::to_string),
            _ => {}
        }
    }
    r
}

/// `a:bodyPr` as stated (everything optional, merged down the chain).
#[derive(Debug, Clone, Default)]
pub(crate) struct BodyPr {
    pub ins: [Option<f64>; 4],
    pub anchor: Option<String>,
    pub wrap: Option<bool>,
    pub autofit: Option<String>,
    pub font_scale: Option<f64>,
    pub line_reduction: Option<f64>,
    pub vertical: bool,
}

impl BodyPr {
    pub fn merged(&self, over: &BodyPr) -> BodyPr {
        let mut ins = self.ins;
        for (i, o) in over.ins.iter().enumerate() {
            if o.is_some() {
                ins[i] = *o;
            }
        }
        BodyPr {
            ins,
            anchor: over.anchor.clone().or_else(|| self.anchor.clone()),
            wrap: over.wrap.or(self.wrap),
            autofit: over.autofit.clone().or_else(|| self.autofit.clone()),
            font_scale: over.font_scale.or(self.font_scale),
            line_reduction: over.line_reduction.or(self.line_reduction),
            vertical: over.vertical || self.vertical,
        }
    }
}

pub(crate) fn parse_body_pr(n: Node) -> BodyPr {
    let f = |a: &str| n.attribute(a).and_then(|v| v.parse::<f64>().ok());
    let mut b = BodyPr {
        ins: [f("lIns"), f("tIns"), f("rIns"), f("bIns")],
        anchor: n.attribute("anchor").map(|a| match a {
            "ctr" => "ctr".to_string(),
            "b" => "b".to_string(),
            _ => "t".to_string(),
        }),
        wrap: n.attribute("wrap").map(|w| w != "none"),
        vertical: n.attribute("vert").map(|v| v != "horz").unwrap_or(false),
        ..Default::default()
    };
    for c in n.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "normAutofit" => {
                b.autofit = Some("shrink".into());
                b.font_scale = c
                    .attribute("fontScale")
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|v| v / 100000.0);
                b.line_reduction = c
                    .attribute("lnSpcReduction")
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|v| v / 100000.0);
            }
            "spAutoFit" => b.autofit = Some("resize".into()),
            "noAutofit" => b.autofit = Some("none".into()),
            _ => {}
        }
    }
    b
}

/// What the resolver needs besides the XML.
pub(crate) struct TextCtx<'a> {
    pub colors: &'a ColorCtx<'a>,
    pub theme: &'a Theme,
    /// Colour of text nothing else colours (`tx1`, or a shape style's
    /// `fontRef` colour).
    pub default_color: Rgba,
    pub slide_index: usize,
}

fn resolve_font(name: &str, theme: &Theme) -> Option<String> {
    Some(match name {
        "+mj-lt" => theme.major_latin.clone(),
        "+mn-lt" => theme.minor_latin.clone(),
        "+mj-cs" => theme.major_cs.clone()?,
        "+mn-cs" => theme.minor_cs.clone()?,
        "+mj-ea" | "+mn-ea" => return None,
        other if other.is_empty() => return None,
        other => other.to_string(),
    })
}

/// Build the scene text for a `txBody`. `chain` lists the list styles that
/// apply, lowest priority first (the shape's own `a:lstStyle` last).
pub(crate) fn build_text_body(
    tx_body: Node,
    chain: &[&LevelStyles],
    body: &BodyPr,
    ctx: &TextCtx,
) -> TextBody {
    let font_scale = body.font_scale.unwrap_or(1.0);
    let mut out = TextBody {
        ins: [
            body.ins[0].unwrap_or(91440.0),
            body.ins[1].unwrap_or(45720.0),
            body.ins[2].unwrap_or(91440.0),
            body.ins[3].unwrap_or(45720.0),
        ],
        anchor: body.anchor.clone().unwrap_or_else(|| "t".into()),
        wrap: body.wrap.unwrap_or(true),
        autofit: body.autofit.clone().unwrap_or_else(|| "none".into()),
        font_scale,
        line_reduction: body.line_reduction.unwrap_or(0.0),
        paras: Vec::new(),
    };
    let mut auto_counters: Vec<(usize, u32)> = Vec::new();
    for p in tx_body
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "p")
    {
        let ppr_node = p
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "pPr");
        let own = ppr_node.map(parse_ppr).unwrap_or_default();
        let lvl = ppr_node
            .and_then(|n| n.attribute("lvl"))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
            .min(8);
        let mut eff = PProps::default();
        for s in chain {
            eff = eff.merged(&s.levels[lvl]);
        }
        eff = eff.merged(&own);

        // Run properties common to the paragraph.
        let base_rpr = RProps {
            sz: Some(18.0),
            ..Default::default()
        }
        .merged(&eff.def);
        let end_sz = p
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "endParaRPr")
            .map(parse_rpr)
            .and_then(|r| r.sz)
            .or(base_rpr.sz)
            .unwrap_or(18.0);

        let mut runs: Vec<Run> = Vec::new();
        for c in p.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "r" => {
                    let rpr = c
                        .children()
                        .find(|x| x.is_element() && x.tag_name().name() == "rPr")
                        .map(parse_rpr)
                        .unwrap_or_default();
                    let text = c
                        .children()
                        .find(|x| x.is_element() && x.tag_name().name() == "t")
                        .and_then(|t| t.text())
                        .unwrap_or("");
                    if !text.is_empty() {
                        runs.push(make_run(text, &base_rpr.merged(&rpr), ctx));
                    }
                }
                "br" => {
                    let rpr = c
                        .children()
                        .find(|x| x.is_element() && x.tag_name().name() == "rPr")
                        .map(parse_rpr)
                        .unwrap_or_default();
                    runs.push(make_run("\n", &base_rpr.merged(&rpr), ctx));
                }
                "fld" => {
                    let rpr = c
                        .children()
                        .find(|x| x.is_element() && x.tag_name().name() == "rPr")
                        .map(parse_rpr)
                        .unwrap_or_default();
                    let cached = c
                        .children()
                        .find(|x| x.is_element() && x.tag_name().name() == "t")
                        .and_then(|t| t.text())
                        .unwrap_or("");
                    let text = if c.attribute("type") == Some("slidenum") {
                        ctx.slide_index.to_string()
                    } else {
                        cached.to_string()
                    };
                    if !text.is_empty() {
                        runs.push(make_run(&text, &base_rpr.merged(&rpr), ctx));
                    }
                }
                _ => {}
            }
        }

        let has_text = runs.iter().any(|r| !r.t.trim().is_empty());
        let bullet = if has_text {
            match &eff.bu {
                Some(BulletSpec::Char(ch)) => Some(Bullet {
                    kind: "char".into(),
                    ch: ch.clone(),
                    num_type: String::new(),
                    start: 1,
                    c: eff
                        .bu_clr
                        .as_ref()
                        .map(|c| ctx.colors.resolve(c, Some(&ctx.default_color)).hex()),
                    sz_pct: eff.bu_sz_pct.unwrap_or(1.0),
                    font: eff.bu_font.clone().filter(|f| !f.is_empty()),
                }),
                Some(BulletSpec::Auto { ty, start }) => {
                    // Numbering continues across consecutive paragraphs of
                    // the same level and scheme.
                    let n = match auto_counters.iter_mut().find(|(l, _)| *l == lvl) {
                        Some((_, n)) => {
                            *n += 1;
                            *n
                        }
                        None => {
                            auto_counters.push((lvl, *start));
                            *start
                        }
                    };
                    Some(Bullet {
                        kind: "num".into(),
                        ch: String::new(),
                        num_type: ty.clone(),
                        start: n,
                        c: eff
                            .bu_clr
                            .as_ref()
                            .map(|c| ctx.colors.resolve(c, Some(&ctx.default_color)).hex()),
                        sz_pct: eff.bu_sz_pct.unwrap_or(1.0),
                        font: None,
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        if bullet.is_none() {
            // A bullet-less paragraph ends numbering at its level.
            auto_counters.retain(|(l, _)| *l != lvl);
        }

        let first_size = runs.first().map(|r| r.sz).unwrap_or(end_sz);
        let spacing = |s: &Option<Spacing>| match s {
            Some(Spacing::Pts(v)) => *v,
            Some(Spacing::Pct(v)) => v * first_size * 1.2,
            None => 0.0,
        };
        out.paras.push(Para {
            algn: eff.algn.clone().unwrap_or_else(|| "l".into()),
            rtl: eff.rtl.unwrap_or(false),
            mar_l: eff.mar_l.unwrap_or(0.0),
            indent: eff.indent.unwrap_or(0.0),
            bullet,
            spc_bef: spacing(&eff.spc_bef),
            spc_aft: spacing(&eff.spc_aft),
            ln_pct: match eff.ln {
                Some(Spacing::Pct(v)) => Some(v),
                _ => None,
            },
            ln_pts: match eff.ln {
                Some(Spacing::Pts(v)) => Some(v),
                _ => None,
            },
            runs,
            end_sz,
        });
    }
    out
}

fn make_run(text: &str, rpr: &RProps, ctx: &TextCtx) -> Run {
    let color = rpr
        .fill
        .as_ref()
        .map(|c| ctx.colors.resolve(c, Some(&ctx.default_color)))
        .unwrap_or_else(|| ctx.default_color.clone());
    let font = rpr
        .latin
        .as_deref()
        .and_then(|f| resolve_font(f, ctx.theme))
        .unwrap_or_else(|| ctx.theme.minor_latin.clone());
    let cs = rpr
        .cs
        .as_deref()
        .and_then(|f| resolve_font(f, ctx.theme))
        .or_else(|| ctx.theme.minor_cs.clone());
    Run {
        t: text.to_string(),
        sz: rpr.sz.unwrap_or(18.0),
        b: rpr.b.unwrap_or(false),
        i: rpr.i.unwrap_or(false),
        u: rpr.u.unwrap_or(false),
        strike: rpr.strike.unwrap_or(false),
        c: color.hex(),
        a: color.a,
        font,
        cs,
        baseline: rpr.baseline.unwrap_or(0.0),
        caps: rpr.caps.unwrap_or(false),
        spc: rpr.spc.unwrap_or(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::color::SchemeColors;
    use std::collections::HashMap;

    fn with_ctx<T>(f: impl FnOnce(&TextCtx) -> T) -> T {
        let mut scheme = HashMap::new();
        scheme.insert("dk1".to_string(), [0x10, 0x10, 0x10]);
        scheme.insert("accent1".to_string(), [0x1F, 0x5F, 0xCC]);
        let scheme = SchemeColors(scheme);
        let map: HashMap<String, String> = [("tx1".to_string(), "dk1".to_string())]
            .into_iter()
            .collect();
        let colors = ColorCtx {
            scheme: &scheme,
            map: &map,
        };
        let theme = Theme {
            major_latin: "Georgia".into(),
            minor_latin: "Calibri".into(),
            ..Default::default()
        };
        let ctx = TextCtx {
            colors: &colors,
            theme: &theme,
            default_color: Rgba {
                rgb: [0x10, 0x10, 0x10],
                a: 1.0,
            },
            slide_index: 7,
        };
        f(&ctx)
    }

    fn body(xml: &str, chain_xml: &[&str], ctx: &TextCtx) -> TextBody {
        let styles: Vec<LevelStyles> = chain_xml
            .iter()
            .map(|x| {
                let d = roxmltree::Document::parse(x).unwrap();
                parse_list_style(d.root_element())
            })
            .collect();
        let refs: Vec<&LevelStyles> = styles.iter().collect();
        let d = roxmltree::Document::parse(xml).unwrap();
        build_text_body(d.root_element(), &refs, &BodyPr::default(), ctx)
    }

    const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;

    #[test]
    fn inheritance_chain_later_wins_and_runs_override() {
        with_ctx(|ctx| {
            let master = format!(
                r#"<a:lstStyle {NS}><a:lvl1pPr marL="228600" indent="-228600" algn="l"><a:buChar char="•"/><a:defRPr sz="2400"><a:latin typeface="+mj-lt"/></a:defRPr></a:lvl1pPr></a:lstStyle>"#
            );
            let layout = format!(
                r#"<a:lstStyle {NS}><a:lvl1pPr algn="ctr"><a:defRPr sz="3200" b="1"/></a:lvl1pPr></a:lstStyle>"#
            );
            let tb = body(
                &format!(
                    r#"<p:txBody xmlns:p="p" {NS}><a:p><a:r><a:rPr i="1"/><a:t>Hi</a:t></a:r></a:p></p:txBody>"#
                ),
                &[&master, &layout],
                ctx,
            );
            let p = &tb.paras[0];
            assert_eq!(p.algn, "ctr", "layout beats master");
            assert_eq!(p.mar_l, 228600.0, "master value survives");
            let b = p.bullet.as_ref().unwrap();
            assert_eq!((b.kind.as_str(), b.ch.as_str()), ("char", "•"));
            let r = &p.runs[0];
            assert_eq!(r.sz, 32.0);
            assert!(r.b && r.i);
            assert_eq!(r.font, "Georgia", "+mj-lt resolves through the theme");
            assert_eq!(r.c, "#101010");
        });
    }

    #[test]
    fn numbering_continues_and_fields_and_breaks() {
        with_ctx(|ctx| {
            let tb = body(
                &format!(
                    r#"<p:txBody xmlns:p="p" {NS}>
                    <a:p><a:pPr><a:buAutoNum type="arabicPeriod"/></a:pPr><a:r><a:t>one</a:t></a:r></a:p>
                    <a:p><a:pPr><a:buAutoNum type="arabicPeriod"/></a:pPr><a:r><a:t>two</a:t></a:r><a:br/><a:r><a:t>x</a:t></a:r></a:p>
                    <a:p><a:pPr><a:buNone/></a:pPr><a:fld type="slidenum"><a:t>‹#›</a:t></a:fld></a:p>
                    <a:p><a:pPr><a:buAutoNum type="arabicPeriod"/></a:pPr><a:r><a:t>restarts</a:t></a:r></a:p>
                    </p:txBody>"#
                ),
                &[],
                ctx,
            );
            let n: Vec<u32> = tb
                .paras
                .iter()
                .filter_map(|p| p.bullet.as_ref().map(|b| b.start))
                .collect();
            assert_eq!(
                n,
                vec![1, 2, 1],
                "a bullet-less paragraph restarts numbering"
            );
            assert_eq!(tb.paras[1].runs[1].t, "\n");
            assert_eq!(tb.paras[2].runs[0].t, "7", "slide number field resolved");
        });
    }

    #[test]
    fn spacing_rtl_and_empty_paragraph_size() {
        with_ctx(|ctx| {
            let tb = body(
                &format!(
                    r#"<p:txBody xmlns:p="p" {NS}>
                    <a:p><a:pPr algn="r" rtl="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPts val="1200"/></a:spcBef></a:pPr><a:r><a:rPr sz="2000" lang="ar-SA"><a:cs typeface="Arial"/></a:rPr><a:t>مرحبا</a:t></a:r></a:p>
                    <a:p><a:endParaRPr sz="1400"/></a:p></p:txBody>"#
                ),
                &[],
                ctx,
            );
            let p = &tb.paras[0];
            assert!(p.rtl);
            assert_eq!(p.algn, "r");
            assert_eq!(p.ln_pct, Some(0.9));
            assert_eq!(p.spc_bef, 12.0);
            assert_eq!(p.runs[0].cs.as_deref(), Some("Arial"));
            assert_eq!(tb.paras[1].end_sz, 14.0);
            assert!(tb.paras[1].runs.is_empty());
        });
    }

    #[test]
    fn body_pr_merges_and_reads_autofit() {
        let xml = format!(
            r#"<a:bodyPr {NS} lIns="0" anchor="ctr" wrap="none"><a:normAutofit fontScale="92500" lnSpcReduction="10000"/></a:bodyPr>"#
        );
        let d = roxmltree::Document::parse(&xml).unwrap();
        let over = parse_body_pr(d.root_element());
        let base = BodyPr {
            ins: [Some(1.0), Some(2.0), Some(3.0), Some(4.0)],
            anchor: Some("t".into()),
            ..Default::default()
        };
        let m = base.merged(&over);
        assert_eq!(m.ins, [Some(0.0), Some(2.0), Some(3.0), Some(4.0)]);
        assert_eq!(m.anchor.as_deref(), Some("ctr"));
        assert_eq!(m.wrap, Some(false));
        assert_eq!(m.font_scale, Some(0.925));
        assert_eq!(m.line_reduction, Some(0.1));
        assert_eq!(m.autofit.as_deref(), Some("shrink"));
    }
}
