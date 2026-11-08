//! PPTX → [`DeckScene`]: the reader behind real slide previews.
//!
//! One path serves every deck — Harbor-made, PowerPoint-made or from
//! anywhere else — because it reads the *package*, not any Harbor model:
//! slide size, the slide → layout → master → theme chain, placeholder
//! inheritance (geometry, fills, lines, text styles, body properties), the
//! colour map (including a layout's dark override), theme colours with
//! their transforms, shapes (preset and custom geometry, groups with
//! transforms, connectors), pictures with crops, tables, charts, and the
//! authored transition and animations.
//!
//! What it cannot draw it says so: each slide lists `unsupported` features
//! (SmartArt, OLE objects, vertical text, unreadable images, ...) instead
//! of silently dropping them. It reads untrusted files: every part read is
//! size-bounded, counts are capped, and nothing here panics on bad input.

mod color;
mod motion;
mod style;
mod text;

use std::collections::HashMap;
use std::io::{Cursor, Read};

use base64::Engine as _;
use roxmltree::{Document, Node};

use self::color::{color_child, ColorCtx, ColorSpec, Rgba};
use self::style::{
    fill_child, parse_effects, parse_line, parse_theme, resolve_fill, resolve_line, resolve_shadow,
    EffectSpec, FillSpec, LineSpec, Theme,
};
use self::text::{build_text_body, parse_body_pr, parse_list_style, BodyPr, LevelStyles, TextCtx};
use crate::scene::*;

const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MAX_XML_PART: u64 = 16 * 1024 * 1024;
const MAX_MEDIA: u64 = 8 * 1024 * 1024;
const MAX_MEDIA_TOTAL: usize = 48 * 1024 * 1024;
const MAX_SLIDES: usize = 500;
const MAX_ITEMS_PER_SLIDE: usize = 4000;
const MAX_GROUP_DEPTH: usize = 12;
const MAX_TABLE_CELLS: usize = 5000;
const MAX_XML_DEPTH: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error("not a readable pptx package: {0}")]
    Package(String),
    #[error("malformed pptx: {0}")]
    Malformed(String),
}

/// Resolve every slide of a PPTX package into scenes.
pub fn deck_scene(bytes: &[u8]) -> Result<DeckScene, SceneError> {
    let mut pkg = Package::open(bytes)?;
    let pres_xml = pkg
        .text("ppt/presentation.xml")
        .ok_or_else(|| SceneError::Malformed("no ppt/presentation.xml".into()))?;
    let pres = Document::parse(&pres_xml).map_err(|e| SceneError::Malformed(e.to_string()))?;
    let pres_root = pres.root_element();
    let (width, height) = descendant(pres_root, "sldSz")
        .and_then(|s| {
            Some((
                s.attribute("cx")?.parse::<f64>().ok()?,
                s.attribute("cy")?.parse::<f64>().ok()?,
            ))
        })
        .filter(|(w, h)| *w > 0.0 && *h > 0.0 && *w <= 1.0e8 && *h <= 1.0e8)
        .unwrap_or((12_192_000.0, 6_858_000.0));
    let pres_default = descendant(pres_root, "defaultTextStyle")
        .map(parse_list_style)
        .unwrap_or_default();

    let pres_rels = pkg.rels("ppt/presentation.xml");
    let slide_parts: Vec<String> = descendant(pres_root, "sldIdLst")
        .map(|l| {
            l.children()
                .filter(|c| c.is_element() && c.tag_name().name() == "sldId")
                .filter_map(|s| s.attribute((REL_NS, "id")))
                .filter_map(|rid| pres_rels.get(rid))
                .map(|r| r.target.clone())
                .collect()
        })
        .unwrap_or_default();
    if slide_parts.is_empty() {
        return Err(SceneError::Malformed("the deck lists no slides".into()));
    }

    let mut budget = MediaBudget { used: 0 };
    let mut slides = Vec::new();
    for (i, part) in slide_parts.iter().take(MAX_SLIDES).enumerate() {
        slides.push(build_slide(
            &mut pkg,
            part,
            i + 1,
            &pres_default,
            &mut budget,
        ));
    }
    Ok(DeckScene {
        width,
        height,
        slides,
    })
}

fn descendant<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.descendants()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn attr_f(n: Node, name: &str) -> Option<f64> {
    n.attribute(name).and_then(|v| v.parse::<f64>().ok())
}

// ---------------------------------------------------------------------------
// Package access

struct Rel {
    ty: String,
    target: String,
}

struct MediaBudget {
    used: usize,
}

struct Package<'b> {
    archive: zip::ZipArchive<Cursor<&'b [u8]>>,
}

impl<'b> Package<'b> {
    fn open(bytes: &'b [u8]) -> Result<Self, SceneError> {
        if bytes.len() < 4 || !bytes.starts_with(b"PK") {
            return Err(SceneError::Package("not a zip archive".into()));
        }
        let archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| SceneError::Package(e.to_string()))?;
        Ok(Package { archive })
    }

    fn bytes(&mut self, name: &str, limit: u64) -> Option<Vec<u8>> {
        let f = self.archive.by_name(name).ok()?;
        if f.size() > limit {
            return None;
        }
        let mut out = Vec::new();
        f.take(limit + 1).read_to_end(&mut out).ok()?;
        if out.len() as u64 > limit {
            return None;
        }
        Some(out)
    }

    /// A text part, refused when its XML nests deeper than the parser can
    /// safely recurse (roxmltree has no depth limit: a few hundred nested
    /// elements overflow the stack, and this reads untrusted files).
    fn text(&mut self, name: &str) -> Option<String> {
        let s = String::from_utf8(self.bytes(name, MAX_XML_PART)?).ok()?;
        crate::scene_chart::check_xml_depth(&s, MAX_XML_DEPTH).ok()?;
        Some(s)
    }

    /// Relationships of a part, keyed by id, with absolute part targets.
    fn rels(&mut self, part: &str) -> HashMap<String, Rel> {
        let (dir, file) = match part.rsplit_once('/') {
            Some((d, f)) => (d.to_string(), f.to_string()),
            None => (String::new(), part.to_string()),
        };
        let rels_name = format!("{dir}/_rels/{file}.rels");
        let rels_name = rels_name.trim_start_matches('/').to_string();
        let mut out = HashMap::new();
        let Some(xml) = self.text(&rels_name) else {
            return out;
        };
        let Ok(doc) = Document::parse(&xml) else {
            return out;
        };
        for r in doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "Relationship")
        {
            if r.attribute("TargetMode") == Some("External") {
                continue;
            }
            let (Some(id), Some(target), Some(ty)) = (
                r.attribute("Id"),
                r.attribute("Target"),
                r.attribute("Type"),
            ) else {
                continue;
            };
            let abs = match target.strip_prefix('/') {
                Some(a) => a.to_string(),
                None => normalize(&dir, target),
            };
            out.insert(
                id.to_string(),
                Rel {
                    ty: ty.rsplit('/').next().unwrap_or(ty).to_string(),
                    target: abs,
                },
            );
        }
        out
    }
}

fn normalize(dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn rel_of_type<'r>(rels: &'r HashMap<String, Rel>, ty: &str) -> Option<&'r Rel> {
    let mut found: Vec<(&String, &Rel)> = rels.iter().filter(|(_, r)| r.ty == ty).collect();
    found.sort_by(|a, b| a.0.cmp(b.0));
    found.first().map(|(_, r)| *r)
}

// ---------------------------------------------------------------------------
// Slide assembly

#[derive(Clone, Copy, PartialEq)]
enum LayerKind {
    Master,
    Layout,
    Slide,
}

struct Layer<'d> {
    kind: LayerKind,
    rels: &'d HashMap<String, Rel>,
    clr_map: HashMap<String, String>,
}

/// Where a slide shape looks for what it does not state itself.
struct PhSources<'d> {
    layout_tree: Option<Node<'d, 'd>>,
    master_tree: Option<Node<'d, 'd>>,
}

struct MasterStyles {
    title: LevelStyles,
    body: LevelStyles,
    other: LevelStyles,
}

struct Builder<'p, 'b, 'd> {
    pkg: &'p mut Package<'b>,
    theme: &'d Theme,
    pres_default: &'d LevelStyles,
    master_styles: &'d MasterStyles,
    budget: &'p mut MediaBudget,
    slide_index: usize,
    unsupported: Vec<String>,
    items: usize,
}

fn cctx<'c>(theme: &'c Theme, layer: &'c Layer) -> ColorCtx<'c> {
    ColorCtx {
        scheme: &theme.scheme,
        map: &layer.clr_map,
    }
}

fn default_clr_map() -> HashMap<String, String> {
    [
        ("bg1", "lt1"),
        ("tx1", "dk1"),
        ("bg2", "lt2"),
        ("tx2", "dk2"),
        ("accent1", "accent1"),
        ("accent2", "accent2"),
        ("accent3", "accent3"),
        ("accent4", "accent4"),
        ("accent5", "accent5"),
        ("accent6", "accent6"),
        ("hlink", "hlink"),
        ("folHlink", "folHlink"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect()
}

fn read_clr_map(node: Node) -> HashMap<String, String> {
    node.attributes()
        .map(|a| (a.name().to_string(), a.value().to_string()))
        .collect()
}

fn build_slide(
    pkg: &mut Package,
    slide_part: &str,
    index: usize,
    pres_default: &LevelStyles,
    budget: &mut MediaBudget,
) -> SlideScene {
    let mut scene = SlideScene {
        index,
        background: Paint::Solid {
            c: "#FFFFFF".into(),
            a: 1.0,
        },
        ..Default::default()
    };
    let Some(slide_xml) = pkg.text(slide_part) else {
        scene
            .unsupported
            .push("slide part could not be read".into());
        return scene;
    };
    let Ok(slide_doc) = Document::parse(&slide_xml) else {
        scene.unsupported.push("slide XML is malformed".into());
        return scene;
    };
    let slide_rels = pkg.rels(slide_part);
    let layout_part = rel_of_type(&slide_rels, "slideLayout").map(|r| r.target.clone());
    let layout_xml = layout_part.as_deref().and_then(|p| pkg.text(p));
    let layout_doc = layout_xml.as_deref().and_then(|x| Document::parse(x).ok());
    let layout_rels = layout_part
        .as_deref()
        .map(|p| pkg.rels(p))
        .unwrap_or_default();
    let master_part = rel_of_type(&layout_rels, "slideMaster").map(|r| r.target.clone());
    let master_xml = master_part.as_deref().and_then(|p| pkg.text(p));
    let master_doc = master_xml.as_deref().and_then(|x| Document::parse(x).ok());
    let master_rels = master_part
        .as_deref()
        .map(|p| pkg.rels(p))
        .unwrap_or_default();
    let theme_part = rel_of_type(&master_rels, "theme").map(|r| r.target.clone());
    let theme_xml = theme_part.as_deref().and_then(|p| pkg.text(p));
    let theme = theme_xml
        .as_deref()
        .map(parse_theme)
        .unwrap_or_else(|| parse_theme(""));

    let slide_root = slide_doc.root_element();
    let master_root = master_doc.as_ref().map(|d| d.root_element());
    let layout_root = layout_doc.as_ref().map(|d| d.root_element());

    // Master text styles.
    let tx_styles = master_root.and_then(|m| descendant(m, "txStyles"));
    let style_of = |name: &str| {
        tx_styles
            .and_then(|t| child(t, name))
            .map(parse_list_style)
            .unwrap_or_default()
    };
    let master_styles = MasterStyles {
        title: style_of("titleStyle"),
        body: style_of("bodyStyle"),
        other: style_of("otherStyle"),
    };

    // Colour maps: master, overridden by the layout, overridden by the slide.
    let master_map = master_root
        .and_then(|m| child(m, "clrMap"))
        .map(read_clr_map)
        .unwrap_or_else(default_clr_map);
    let override_of = |root: Option<Node>| {
        root.and_then(|r| child(r, "clrMapOvr"))
            .and_then(|o| child(o, "overrideClrMapping"))
            .map(read_clr_map)
    };
    let layout_map = override_of(layout_root).unwrap_or_else(|| master_map.clone());
    let slide_map = override_of(Some(slide_root)).unwrap_or_else(|| layout_map.clone());

    let mut b = Builder {
        pkg,
        theme: &theme,
        pres_default,
        master_styles: &master_styles,
        budget,
        slide_index: index,
        unsupported: Vec::new(),
        items: 0,
    };

    // Background: slide, else layout, else master.
    // The background is inherited, but its colours resolve under the
    // slide's EFFECTIVE colour map: a dark layout's bg1→dk1 override must
    // darken a master background that says "bg1".
    for root in [Some(slide_root), layout_root, master_root] {
        let Some(root) = root else { continue };
        if let Some(paint) = b.background(root, &slide_map) {
            scene.background = paint;
            break;
        }
    }

    // Shapes: master, then layout, then slide.
    let ph_sources = PhSources {
        layout_tree: layout_root.and_then(|l| descendant(l, "spTree")),
        master_tree: master_root.and_then(|m| descendant(m, "spTree")),
    };
    let hide_master = slide_root.attribute("showMasterSp") == Some("0");
    let hide_master_from_layout =
        layout_root.and_then(|l| l.attribute("showMasterSp")) == Some("0");
    let mut items: Vec<Item> = Vec::new();
    if !hide_master && !hide_master_from_layout {
        if let (Some(tree), Some(_)) = (ph_sources.master_tree, master_root) {
            let layer = Layer {
                kind: LayerKind::Master,
                rels: &master_rels,
                clr_map: master_map.clone(),
            };
            b.walk(tree, &layer, &ph_sources, &[], &mut items);
        }
    }
    if !hide_master {
        if let Some(tree) = ph_sources.layout_tree {
            let layer = Layer {
                kind: LayerKind::Layout,
                rels: &layout_rels,
                clr_map: layout_map.clone(),
            };
            b.walk(tree, &layer, &ph_sources, &[], &mut items);
        }
    }
    if let Some(tree) = descendant(slide_root, "spTree") {
        let layer = Layer {
            kind: LayerKind::Slide,
            rels: &slide_rels,
            clr_map: slide_map.clone(),
        };
        b.walk(tree, &layer, &ph_sources, &[], &mut items);
    }
    scene.items = items;
    scene.transition = motion::parse_transition(slide_root);
    scene.animations = motion::parse_animations(slide_root);
    let mut unsupported = std::mem::take(&mut b.unsupported);
    unsupported.sort();
    unsupported.dedup();
    scene.unsupported = unsupported;
    scene.notes = rel_of_type(&slide_rels, "notesSlide")
        .map(|r| r.target.clone())
        .and_then(|p| b.pkg.text(&p))
        .and_then(|x| notes_text(&x));
    scene
}

fn notes_text(xml: &str) -> Option<String> {
    let doc = Document::parse(xml).ok()?;
    // The body placeholder (not the slide image / slide number).
    let body = doc.descendants().find(|n| {
        n.is_element()
            && n.tag_name().name() == "sp"
            && n.descendants().any(|p| {
                p.is_element() && p.tag_name().name() == "ph" && p.attribute("type") == Some("body")
            })
    })?;
    let lines: Vec<String> = body
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "p")
        .map(|p| {
            p.descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "t")
                .filter_map(|t| t.text())
                .collect::<String>()
        })
        .collect();
    let text = lines.join("\n").trim().to_string();
    (!text.is_empty()).then_some(text)
}

// ---------------------------------------------------------------------------
// Geometry helpers

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Clone, Copy, Debug)]
struct GroupXf {
    off: (f64, f64),
    ext: (f64, f64),
    ch_off: (f64, f64),
    ch_ext: (f64, f64),
    rot: f64,
}

/// Map a child rectangle (in the group's child space) to its parent space.
fn through_group(g: &GroupXf, r: Rect, rot: f64) -> (Rect, f64) {
    let sx = if g.ch_ext.0.abs() > 1e-9 {
        g.ext.0 / g.ch_ext.0
    } else {
        1.0
    };
    let sy = if g.ch_ext.1.abs() > 1e-9 {
        g.ext.1 / g.ch_ext.1
    } else {
        1.0
    };
    let mut out = Rect {
        x: g.off.0 + (r.x - g.ch_off.0) * sx,
        y: g.off.1 + (r.y - g.ch_off.1) * sy,
        w: r.w * sx,
        h: r.h * sy,
    };
    let mut rot = rot;
    if g.rot.abs() > 1e-9 {
        // Rotate the child's centre about the group's centre.
        let gc = (g.off.0 + g.ext.0 / 2.0, g.off.1 + g.ext.1 / 2.0);
        let cc = (out.x + out.w / 2.0, out.y + out.h / 2.0);
        let (s, c) = g.rot.to_radians().sin_cos();
        let (dx, dy) = (cc.0 - gc.0, cc.1 - gc.1);
        let nc = (gc.0 + dx * c - dy * s, gc.1 + dx * s + dy * c);
        out.x = nc.0 - out.w / 2.0;
        out.y = nc.1 - out.h / 2.0;
        rot += g.rot;
    }
    (out, rot)
}

struct Xfrm {
    rect: Rect,
    rot: f64,
    flip_h: bool,
    flip_v: bool,
}

fn parse_xfrm(n: Node) -> Option<Xfrm> {
    let off = child(n, "off")?;
    let ext = child(n, "ext")?;
    let (x, y) = (attr_f(off, "x")?, attr_f(off, "y")?);
    let (w, h) = (attr_f(ext, "cx")?, attr_f(ext, "cy")?);
    if !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) {
        return None;
    }
    Some(Xfrm {
        rect: Rect { x, y, w, h },
        rot: attr_f(n, "rot").unwrap_or(0.0) / 60000.0,
        flip_h: n.attribute("flipH") == Some("1"),
        flip_v: n.attribute("flipV") == Some("1"),
    })
}

fn parse_geom(sp_pr: Node) -> Option<Geom> {
    if let Some(p) = child(sp_pr, "prstGeom") {
        let adj = child(p, "avLst")
            .map(|l| {
                l.children()
                    .filter(|g| g.is_element() && g.tag_name().name() == "gd")
                    .filter_map(|g| {
                        g.attribute("fmla")?
                            .strip_prefix("val ")?
                            .trim()
                            .parse::<f64>()
                            .ok()
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Some(Geom::Preset {
            name: p.attribute("prst").unwrap_or("rect").to_string(),
            adj,
        });
    }
    let cust = child(sp_pr, "custGeom")?;
    let path = child(child(cust, "pathLst")?, "path")?;
    let (w, h) = (
        attr_f(path, "w").filter(|v| *v > 0.0).unwrap_or(100.0),
        attr_f(path, "h").filter(|v| *v > 0.0).unwrap_or(100.0),
    );
    let pt = |n: Node| -> Option<(f64, f64)> {
        let p = child(n, "pt")?;
        Some((attr_f(p, "x")?, attr_f(p, "y")?))
    };
    let mut cmds = Vec::new();
    let mut cur = (0.0, 0.0);
    for c in path.children().filter(|c| c.is_element()) {
        if cmds.len() > 20_000 {
            break;
        }
        match c.tag_name().name() {
            "moveTo" => {
                if let Some((x, y)) = pt(c) {
                    cmds.push(PathCmd::M { x, y });
                    cur = (x, y);
                }
            }
            "lnTo" => {
                if let Some((x, y)) = pt(c) {
                    cmds.push(PathCmd::L { x, y });
                    cur = (x, y);
                }
            }
            "cubicBezTo" => {
                let pts: Vec<(f64, f64)> = c
                    .children()
                    .filter(|p| p.is_element() && p.tag_name().name() == "pt")
                    .filter_map(|p| Some((attr_f(p, "x")?, attr_f(p, "y")?)))
                    .collect();
                if pts.len() == 3 {
                    cmds.push(PathCmd::C {
                        x1: pts[0].0,
                        y1: pts[0].1,
                        x2: pts[1].0,
                        y2: pts[1].1,
                        x: pts[2].0,
                        y: pts[2].1,
                    });
                    cur = pts[2];
                }
            }
            "quadBezTo" => {
                let pts: Vec<(f64, f64)> = c
                    .children()
                    .filter(|p| p.is_element() && p.tag_name().name() == "pt")
                    .filter_map(|p| Some((attr_f(p, "x")?, attr_f(p, "y")?)))
                    .collect();
                if pts.len() == 2 {
                    cmds.push(PathCmd::Q {
                        x1: pts[0].0,
                        y1: pts[0].1,
                        x: pts[1].0,
                        y: pts[1].1,
                    });
                    cur = pts[1];
                }
            }
            "arcTo" => {
                if let (Some(wr), Some(hr), Some(st), Some(sw)) = (
                    attr_f(c, "wR"),
                    attr_f(c, "hR"),
                    attr_f(c, "stAng"),
                    attr_f(c, "swAng"),
                ) {
                    arc_to(&mut cmds, &mut cur, wr, hr, st / 60000.0, sw / 60000.0);
                }
            }
            "close" => cmds.push(PathCmd::Z),
            _ => {}
        }
    }
    if cmds.is_empty() {
        return None;
    }
    Some(Geom::Path { w, h, cmds })
}

/// Append an elliptical arc as cubic Béziers (at most 90° per segment).
fn arc_to(cmds: &mut Vec<PathCmd>, cur: &mut (f64, f64), wr: f64, hr: f64, st: f64, sw: f64) {
    if wr.abs() < 1e-9 || hr.abs() < 1e-9 || sw.abs() < 1e-9 {
        return;
    }
    let st_r = st.to_radians();
    let center = (cur.0 - wr * st_r.cos(), cur.1 - hr * st_r.sin());
    let segments = ((sw.abs() / 90.0).ceil() as usize).clamp(1, 8);
    let step = (sw / segments as f64).to_radians();
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut a = st_r;
    for _ in 0..segments {
        let a2 = a + step;
        let p0 = (center.0 + wr * a.cos(), center.1 + hr * a.sin());
        let p3 = (center.0 + wr * a2.cos(), center.1 + hr * a2.sin());
        let c1 = (p0.0 - k * wr * a.sin(), p0.1 + k * hr * a.cos());
        let c2 = (p3.0 + k * wr * a2.sin(), p3.1 - k * hr * a2.cos());
        cmds.push(PathCmd::C {
            x1: c1.0,
            y1: c1.1,
            x2: c2.0,
            y2: c2.1,
            x: p3.0,
            y: p3.1,
        });
        a = a2;
        *cur = p3;
    }
}

// ---------------------------------------------------------------------------
// Placeholder resolution

#[derive(Clone, Debug, PartialEq)]
struct PhKey {
    ty: String,
    idx: Option<u32>,
}

fn ph_of(sp: Node) -> Option<PhKey> {
    let ph = sp
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "ph")?;
    Some(PhKey {
        ty: ph.attribute("type").unwrap_or("obj").to_string(),
        idx: ph.attribute("idx").and_then(|v| v.parse().ok()),
    })
}

/// The kind of text style a placeholder type draws from.
fn style_class(ty: &str) -> &'static str {
    match ty {
        "title" | "ctrTitle" => "title",
        "dt" | "ftr" | "sldNum" | "hdr" => "other",
        _ => "body",
    }
}

fn normalize_ph_type(ty: &str) -> &str {
    match ty {
        "ctrTitle" => "title",
        "subTitle" | "obj" | "tbl" | "chart" | "dgm" | "media" | "clipArt" | "pic" => "body",
        other => other,
    }
}

fn find_ph<'a>(tree: Node<'a, 'a>, key: &PhKey, by_idx_first: bool) -> Option<Node<'a, 'a>> {
    let mut by_type = None;
    for sp in tree
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "sp")
    {
        let Some(k) = ph_of(sp) else { continue };
        if by_idx_first {
            if let (Some(a), Some(b)) = (key.idx, k.idx) {
                if a == b {
                    return Some(sp);
                }
                continue;
            }
        }
        if normalize_ph_type(&k.ty) == normalize_ph_type(&key.ty) && by_type.is_none() {
            by_type = Some(sp);
        }
    }
    by_type
}

impl<'p, 'b, 'd> Builder<'p, 'b, 'd> {
    fn note(&mut self, what: impl Into<String>) {
        self.unsupported.push(what.into());
    }

    fn background(&mut self, root: Node, map: &HashMap<String, String>) -> Option<Paint> {
        let bg = descendant(root, "bg")?;
        let ctx = ColorCtx {
            scheme: &self.theme.scheme,
            map,
        };
        if let Some(pr) = child(bg, "bgPr") {
            return match fill_child(pr)? {
                FillSpec::Blip(_) => {
                    self.note("picture background");
                    None
                }
                f => Some(resolve_fill(&f, &ctx, None)),
            };
        }
        let r = child(bg, "bgRef")?;
        let idx = r.attribute("idx")?.parse::<usize>().ok()?;
        let ph = color_child(r).map(|c| ctx.resolve(&c, None));
        let list = if idx >= 1001 {
            &self.theme.bg_fills
        } else {
            &self.theme.fills
        };
        let at = if idx >= 1001 {
            idx - 1001
        } else {
            idx.checked_sub(1)?
        };
        let spec = list.get(at)?;
        Some(resolve_fill(spec, &ctx, ph.as_ref()))
    }

    fn walk(
        &mut self,
        tree: Node<'d, 'd>,
        layer: &Layer,
        src: &PhSources<'d>,
        groups: &[GroupXf],
        out: &mut Vec<Item>,
    ) {
        if groups.len() > MAX_GROUP_DEPTH {
            self.note("deeply nested groups");
            return;
        }
        for n in tree.children().filter(|c| c.is_element()) {
            if self.items >= MAX_ITEMS_PER_SLIDE {
                self.note("slide has too many shapes to preview");
                return;
            }
            match n.tag_name().name() {
                "sp" => self.shape(n, layer, src, groups, out),
                "cxnSp" => self.shape(n, layer, src, groups, out),
                "pic" => self.picture(n, layer, groups, out),
                "graphicFrame" => self.graphic_frame(n, layer, groups, out),
                "grpSp" => {
                    let Some(pr) = child(n, "grpSpPr") else {
                        continue;
                    };
                    let Some(xf) = child(pr, "xfrm") else {
                        continue;
                    };
                    let (Some(off), Some(ext)) = (child(xf, "off"), child(xf, "ext")) else {
                        continue;
                    };
                    let ch_off = child(xf, "chOff");
                    let ch_ext = child(xf, "chExt");
                    let g = GroupXf {
                        off: (
                            attr_f(off, "x").unwrap_or(0.0),
                            attr_f(off, "y").unwrap_or(0.0),
                        ),
                        ext: (
                            attr_f(ext, "cx").unwrap_or(0.0),
                            attr_f(ext, "cy").unwrap_or(0.0),
                        ),
                        ch_off: ch_off
                            .map(|c| (attr_f(c, "x").unwrap_or(0.0), attr_f(c, "y").unwrap_or(0.0)))
                            .unwrap_or((
                                attr_f(off, "x").unwrap_or(0.0),
                                attr_f(off, "y").unwrap_or(0.0),
                            )),
                        ch_ext: ch_ext
                            .map(|c| {
                                (
                                    attr_f(c, "cx").unwrap_or(0.0),
                                    attr_f(c, "cy").unwrap_or(0.0),
                                )
                            })
                            .unwrap_or((
                                attr_f(ext, "cx").unwrap_or(0.0),
                                attr_f(ext, "cy").unwrap_or(0.0),
                            )),
                        rot: attr_f(xf, "rot").unwrap_or(0.0) / 60000.0,
                    };
                    let mut nested = groups.to_vec();
                    // Innermost last: a child maps through its own group
                    // first, then each enclosing one.
                    nested.push(g);
                    self.walk(n, layer, src, &nested, out);
                }
                "AlternateContent" => {
                    // Prefer the fallback branch (pictures/shapes every
                    // reader understands); a Choice-only object is noted.
                    if let Some(fb) = child(n, "Fallback") {
                        self.walk(fb, layer, src, groups, out);
                    } else {
                        self.note("alternate content without a fallback");
                    }
                }
                _ => {}
            }
        }
    }

    /// Absolute rect and rotation of a shape's own xfrm through its groups.
    fn place(&self, xf: &Xfrm, groups: &[GroupXf]) -> (Rect, f64) {
        let mut rect = xf.rect;
        let mut rot = xf.rot;
        for g in groups.iter().rev() {
            let (r, ro) = through_group(g, rect, rot);
            rect = r;
            rot = ro;
        }
        (rect, rot)
    }

    fn frame_id(&self, layer: &Layer, c_nv_pr: Option<Node>) -> (u32, String, Option<String>) {
        let id = if layer.kind == LayerKind::Slide {
            c_nv_pr
                .and_then(|n| n.attribute("id"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        } else {
            0
        };
        (
            id,
            c_nv_pr
                .and_then(|n| n.attribute("name"))
                .unwrap_or_default()
                .to_string(),
            c_nv_pr
                .and_then(|n| n.attribute("descr"))
                .filter(|d| !d.is_empty())
                .map(str::to_string),
        )
    }

    fn shape(
        &mut self,
        sp: Node<'d, 'd>,
        layer: &Layer,
        src: &PhSources<'d>,
        groups: &[GroupXf],
        out: &mut Vec<Item>,
    ) {
        let key = ph_of(sp);
        // Master and layout placeholders are templates, not drawn content.
        if layer.kind != LayerKind::Slide && key.is_some() {
            return;
        }
        let nv = child(sp, "nvSpPr").or_else(|| child(sp, "nvCxnSpPr"));
        let c_nv_pr = nv.and_then(|n| child(n, "cNvPr"));
        if c_nv_pr.and_then(|n| n.attribute("hidden")) == Some("1") {
            return;
        }
        let sp_pr = child(sp, "spPr");

        // Inherited placeholder shapes (layout first, then master).
        let layout_ph = match (&key, src.layout_tree, layer.kind) {
            (Some(k), Some(t), LayerKind::Slide) => find_ph(t, k, true),
            _ => None,
        };
        let master_key = layout_ph.and_then(ph_of).or_else(|| key.clone());
        let master_ph = match (&master_key, src.master_tree, layer.kind) {
            (Some(k), Some(t), LayerKind::Slide) => find_ph(t, k, false),
            _ => None,
        };
        let ph_chain: Vec<Node> = [master_ph, layout_ph].iter().flatten().copied().collect();

        // Geometry.
        let own_xfrm = sp_pr.and_then(|p| child(p, "xfrm")).and_then(parse_xfrm);
        let xfrm = own_xfrm.or_else(|| {
            ph_chain.iter().rev().find_map(|n| {
                child(*n, "spPr")
                    .and_then(|p| child(p, "xfrm"))
                    .and_then(parse_xfrm)
            })
        });
        let Some(xfrm) = xfrm else { return };
        let geom = sp_pr
            .and_then(parse_geom)
            .or_else(|| {
                ph_chain
                    .iter()
                    .rev()
                    .find_map(|n| child(*n, "spPr").and_then(parse_geom))
            })
            .unwrap_or_default();

        // Style references.
        let style = child(sp, "style");
        let style_ref = |name: &str| -> (Option<usize>, Option<ColorSpec>) {
            match style.and_then(|s| child(s, name)) {
                Some(r) => (
                    r.attribute("idx").and_then(|v| v.parse().ok()),
                    color_child(r),
                ),
                None => (None, None),
            }
        };
        let colors = cctx(self.theme, layer);
        let (fill_idx, fill_clr) = style_ref("fillRef");
        let (ln_idx, ln_clr) = style_ref("lnRef");
        let (eff_idx, eff_clr) = style_ref("effectRef");
        let (_, font_clr) = style_ref("fontRef");

        // Fill: own, then the placeholder chain, then the style.
        let spec_fill = sp_pr.and_then(fill_child).or_else(|| {
            ph_chain
                .iter()
                .rev()
                .find_map(|n| child(*n, "spPr").and_then(fill_child))
        });
        let fill_ph = fill_clr.as_ref().map(|c| colors.resolve(c, None));
        let mut fill = Paint::None;
        let mut blip_rid = None;
        match spec_fill {
            Some(FillSpec::Blip(rid)) => blip_rid = Some(rid),
            Some(f) => fill = resolve_fill(&f, &colors, fill_ph.as_ref()),
            None => {
                if let Some(i) = fill_idx.filter(|i| *i > 0) {
                    let list = if i >= 1001 {
                        &self.theme.bg_fills
                    } else {
                        &self.theme.fills
                    };
                    let at = if i >= 1001 { i - 1001 } else { i - 1 };
                    if let Some(f) = list.get(at) {
                        fill = resolve_fill(f, &colors, fill_ph.as_ref());
                    }
                }
            }
        }

        // Line: theme line style merged under own / inherited.
        let mut line_spec = LineSpec::default();
        if let Some(i) = ln_idx.filter(|i| *i > 0) {
            if let Some(base) = self.theme.lines.get(i - 1) {
                line_spec = line_spec.merged(base);
            }
        }
        for n in &ph_chain {
            if let Some(ln) = child(*n, "spPr").and_then(|p| child(p, "ln")) {
                line_spec = line_spec.merged(&parse_line(ln));
            }
        }
        if let Some(ln) = sp_pr.and_then(|p| child(p, "ln")) {
            line_spec = line_spec.merged(&parse_line(ln));
        }
        let ln_ph = ln_clr.as_ref().map(|c| colors.resolve(c, None));
        let line = resolve_line(&line_spec, &colors, ln_ph.as_ref());

        // Shadow.
        let eff_ph = eff_clr.as_ref().map(|c| colors.resolve(c, None));
        let effect: Option<EffectSpec> = sp_pr
            .and_then(|p| child(p, "effectLst"))
            .map(parse_effects)
            .or_else(|| {
                eff_idx
                    .filter(|i| *i > 0)
                    .and_then(|i| self.theme.effects.get(i - 1).cloned())
            });
        let shadow = effect
            .and_then(|e| e.shadow)
            .map(|s| resolve_shadow(&s, &colors, eff_ph.as_ref()));

        // Text.
        let text = child(sp, "txBody").and_then(|tb| {
            let has_text = tb.descendants().any(|n| {
                n.is_element()
                    && n.tag_name().name() == "t"
                    && n.text().map(|t| !t.is_empty()).unwrap_or(false)
            });
            if !has_text {
                return None;
            }
            let class = key.as_ref().map(|k| style_class(&k.ty));
            let mut chain: Vec<&LevelStyles> = vec![self.pres_default];
            match class {
                Some("title") => chain.push(&self.master_styles.title),
                Some("other") => chain.push(&self.master_styles.other),
                Some(_) => chain.push(&self.master_styles.body),
                None => {}
            }
            let own_lst;
            let mut owned: Vec<LevelStyles> = Vec::new();
            for n in &ph_chain {
                if let Some(l) = child(*n, "txBody").and_then(|t| child(t, "lstStyle")) {
                    owned.push(parse_list_style(l));
                }
            }
            if let Some(l) = child(tb, "lstStyle") {
                own_lst = parse_list_style(l);
                owned.push(own_lst);
            }
            for s in &owned {
                chain.push(s);
            }
            let mut body = BodyPr::default();
            for n in &ph_chain {
                if let Some(bp) = child(*n, "txBody").and_then(|t| child(t, "bodyPr")) {
                    body = body.merged(&parse_body_pr(bp));
                }
            }
            if let Some(bp) = child(tb, "bodyPr") {
                body = body.merged(&parse_body_pr(bp));
            }
            if body.vertical {
                self.unsupported
                    .push("vertical text (drawn horizontally)".into());
            }
            let default_color = font_clr
                .as_ref()
                .map(|c| colors.resolve(c, None))
                .unwrap_or_else(|| colors.resolve(&ColorSpec::scheme("tx1"), None));
            let tctx = TextCtx {
                colors: &colors,
                theme: self.theme,
                default_color,
                slide_index: self.slide_index,
            };
            Some(build_text_body(tb, &chain, &body, &tctx))
        });

        let (rect, rot) = self.place(&xfrm, groups);
        let (id, name, alt) = self.frame_id(layer, c_nv_pr);
        let frame = Frame {
            id,
            name,
            alt,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
            rot,
            flip_h: xfrm.flip_h,
            flip_v: xfrm.flip_v,
        };
        // A picture fill becomes a picture under the shape's text.
        if let Some(rid) = blip_rid {
            if let Some(pic) = self.blip_picture(&rid, layer, &frame, &geom, None) {
                self.items += 1;
                out.push(Item::Picture(pic));
            }
        }
        // Skip invisible, textless, strokeless shapes (empty placeholders).
        if matches!(fill, Paint::None) && line.is_none() && text.is_none() && shadow.is_none() {
            return;
        }
        self.items += 1;
        out.push(Item::Shape(ShapeItem {
            frame,
            geom,
            fill,
            line,
            shadow,
            text,
        }));
    }

    fn blip_picture(
        &mut self,
        rid: &str,
        layer: &Layer,
        frame: &Frame,
        geom: &Geom,
        crop: Option<[f64; 4]>,
    ) -> Option<PictureItem> {
        let rel = layer.rels.get(rid)?;
        let ext = rel
            .target
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "webp" => "image/webp",
            other => {
                self.note(format!("image format .{other}"));
                return Some(PictureItem {
                    frame: frame.clone(),
                    mime: String::new(),
                    data_b64: String::new(),
                    crop: crop.unwrap_or([0.0; 4]),
                    geom: geom.clone(),
                    line: None,
                    shadow: None,
                });
            }
        };
        let data = self.pkg.bytes(&rel.target, MAX_MEDIA);
        let data_b64 = match data {
            Some(d) if self.budget.used + d.len() <= MAX_MEDIA_TOTAL => {
                self.budget.used += d.len();
                base64::engine::general_purpose::STANDARD.encode(d)
            }
            _ => {
                self.note("image too large to preview");
                String::new()
            }
        };
        Some(PictureItem {
            frame: frame.clone(),
            mime: mime.into(),
            data_b64,
            crop: crop.unwrap_or([0.0; 4]),
            geom: geom.clone(),
            line: None,
            shadow: None,
        })
    }

    fn picture(
        &mut self,
        pic: Node<'d, 'd>,
        layer: &Layer,
        groups: &[GroupXf],
        out: &mut Vec<Item>,
    ) {
        let c_nv_pr = child(pic, "nvPicPr").and_then(|n| child(n, "cNvPr"));
        if c_nv_pr.and_then(|n| n.attribute("hidden")) == Some("1") {
            return;
        }
        let Some(sp_pr) = child(pic, "spPr") else {
            return;
        };
        let Some(xfrm) = child(sp_pr, "xfrm").and_then(parse_xfrm) else {
            return;
        };
        let bf = child(pic, "blipFill");
        let rid = bf
            .and_then(|b| child(b, "blip"))
            .and_then(|b| b.attribute((REL_NS, "embed")))
            .map(str::to_string);
        let Some(rid) = rid else { return };
        let crop = bf.and_then(|b| child(b, "srcRect")).map(|r| {
            let f = |a: &str| attr_f(r, a).unwrap_or(0.0) / 100000.0;
            [f("l"), f("t"), f("r"), f("b")]
        });
        let (rect, rot) = self.place(&xfrm, groups);
        let (id, name, alt) = self.frame_id(layer, c_nv_pr);
        let frame = Frame {
            id,
            name,
            alt,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
            rot,
            flip_h: xfrm.flip_h,
            flip_v: xfrm.flip_v,
        };
        let geom = parse_geom(sp_pr).unwrap_or_default();
        let colors = cctx(self.theme, layer);
        let line = child(sp_pr, "ln")
            .map(parse_line)
            .and_then(|l| resolve_line(&l, &colors, None));
        let shadow = child(sp_pr, "effectLst")
            .map(parse_effects)
            .and_then(|e| e.shadow)
            .map(|s| resolve_shadow(&s, &colors, None));
        if let Some(mut p) = self.blip_picture(&rid, layer, &frame, &geom, crop) {
            p.line = line;
            p.shadow = shadow;
            self.items += 1;
            out.push(Item::Picture(p));
        }
    }

    fn graphic_frame(
        &mut self,
        gf: Node<'d, 'd>,
        layer: &Layer,
        groups: &[GroupXf],
        out: &mut Vec<Item>,
    ) {
        let c_nv_pr = child(gf, "nvGraphicFramePr").and_then(|n| child(n, "cNvPr"));
        let Some(xf) = child(gf, "xfrm").and_then(parse_xfrm) else {
            return;
        };
        let (rect, rot) = self.place(&xf, groups);
        let (id, name, alt) = self.frame_id(layer, c_nv_pr);
        let frame = Frame {
            id,
            name,
            alt,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
            rot,
            flip_h: false,
            flip_v: false,
        };
        let Some(data) = descendant(gf, "graphicData") else {
            return;
        };
        let uri = data.attribute("uri").unwrap_or("");
        if uri.ends_with("/chart") {
            let Some(chart) = descendant(data, "chart") else {
                return;
            };
            let Some(rid) = chart.attribute((REL_NS, "id")) else {
                return;
            };
            let Some(rel) = layer.rels.get(rid) else {
                return;
            };
            let target = rel.target.clone();
            let Some(xml) = self.pkg.text(&target) else {
                self.note("chart part could not be read");
                return;
            };
            let scheme = &self.theme.scheme;
            let map = &layer.clr_map;
            let lookup = |name: &str| -> Option<String> {
                let mapped = map.get(name).map(String::as_str).unwrap_or(name);
                scheme
                    .0
                    .get(mapped)
                    .map(|c| format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]))
            };
            match chart_from_xml(&xml, &lookup) {
                Ok(data) => {
                    self.items += 1;
                    out.push(Item::Chart(ChartItem { frame, chart: data }));
                }
                Err(e) => self.note(e),
            }
        } else if uri.ends_with("/table") {
            if let Some(tbl) = child(data, "tbl") {
                if let Some(t) = self.table(tbl, layer, frame) {
                    self.items += 1;
                    out.push(Item::Table(t));
                }
            }
        } else {
            let kind = uri.rsplit('/').next().unwrap_or("object");
            self.note(format!("embedded object ({kind})"));
        }
    }

    fn table(&mut self, tbl: Node<'d, 'd>, layer: &Layer, frame: Frame) -> Option<TableItem> {
        let colors = cctx(self.theme, layer);
        let cols: Vec<f64> = child(tbl, "tblGrid")
            .map(|g| {
                g.children()
                    .filter(|c| c.is_element() && c.tag_name().name() == "gridCol")
                    .map(|c| attr_f(c, "w").unwrap_or(0.0))
                    .collect()
            })
            .unwrap_or_default();
        if cols.is_empty() {
            return None;
        }
        // Table style approximation: header row on accent1, banded rows.
        let pr = child(tbl, "tblPr");
        let first_row = pr.and_then(|p| p.attribute("firstRow")) == Some("1");
        let band_row = pr.and_then(|p| p.attribute("bandRow")) == Some("1");
        let accent = ColorSpec::scheme("accent1");
        let white = Rgba {
            rgb: [255, 255, 255],
            a: 1.0,
        };
        let mut rows = Vec::new();
        let mut cells_seen = 0usize;
        for (ri, tr) in tbl
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "tr")
            .enumerate()
        {
            let mut cells = Vec::new();
            for tc in tr
                .children()
                .filter(|c| c.is_element() && c.tag_name().name() == "tc")
            {
                cells_seen += 1;
                if cells_seen > MAX_TABLE_CELLS {
                    self.note("table too large to preview in full");
                    break;
                }
                let tc_pr = child(tc, "tcPr");
                let is_header = first_row && ri == 0;
                let own_fill = tc_pr.and_then(fill_child);
                let fill = match own_fill {
                    Some(f) => resolve_fill(&f, &colors, None),
                    None if is_header => {
                        resolve_fill(&FillSpec::Solid(accent.clone()), &colors, None)
                    }
                    None if band_row && ri % 2 == 1 => {
                        let mut c = accent.clone();
                        c.xf.push(("tint".into(), 0.4));
                        resolve_fill(&FillSpec::Solid(c), &colors, None)
                    }
                    None if band_row => {
                        let mut c = accent.clone();
                        c.xf.push(("tint".into(), 0.2));
                        resolve_fill(&FillSpec::Solid(c), &colors, None)
                    }
                    None => Paint::None,
                };
                let border = |name: &str| {
                    tc_pr
                        .and_then(|p| child(p, name))
                        .map(parse_line)
                        .and_then(|l| resolve_line(&l, &colors, None))
                };
                let default_color = if is_header && tc_pr.and_then(fill_child).is_none() {
                    white.clone()
                } else {
                    colors.resolve(&ColorSpec::scheme("tx1"), None)
                };
                let tctx = TextCtx {
                    colors: &colors,
                    theme: self.theme,
                    default_color,
                    slide_index: self.slide_index,
                };
                let mut body = BodyPr::default();
                body.ins = [Some(91440.0), Some(45720.0), Some(91440.0), Some(45720.0)];
                if let Some(p) = tc_pr {
                    body.ins = [
                        attr_f(p, "marL").or(body.ins[0]),
                        attr_f(p, "marT").or(body.ins[1]),
                        attr_f(p, "marR").or(body.ins[2]),
                        attr_f(p, "marB").or(body.ins[3]),
                    ];
                    body.anchor = p.attribute("anchor").map(|a| match a {
                        "ctr" => "ctr".to_string(),
                        "b" => "b".to_string(),
                        _ => "t".to_string(),
                    });
                }
                let mut text = child(tc, "txBody")
                    .map(|tb| build_text_body(tb, &[self.pres_default], &body, &tctx))
                    .unwrap_or_default();
                if is_header {
                    for p in &mut text.paras {
                        for r in &mut p.runs {
                            r.b = true;
                        }
                    }
                }
                cells.push(TableCell {
                    text,
                    fill,
                    borders: [border("lnL"), border("lnT"), border("lnR"), border("lnB")],
                    grid_span: attr_f(tc, "gridSpan").map(|v| v as u32).unwrap_or(1).max(1),
                    row_span: attr_f(tc, "rowSpan").map(|v| v as u32).unwrap_or(1).max(1),
                    merged_away: tc.attribute("hMerge") == Some("1")
                        || tc.attribute("vMerge") == Some("1"),
                });
            }
            rows.push(TableRow {
                h: attr_f(tr, "h").unwrap_or(0.0),
                cells,
            });
        }
        Some(TableItem { frame, cols, rows })
    }
}

fn chart_from_xml(xml: &str, scheme: &dyn Fn(&str) -> Option<String>) -> Result<ChartData, String> {
    crate::scene_chart::parse_chart(xml, scheme).map_err(|e| {
        // "unsupported chart: ..." is a feature note; anything else is a
        // damaged part.
        if e.starts_with("unsupported chart") {
            e
        } else {
            format!("chart could not be read ({e})")
        }
    })
}

#[cfg(test)]
mod tests;
