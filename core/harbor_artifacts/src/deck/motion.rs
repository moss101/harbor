//! Authored motion: slide transitions and entrance animations.
//!
//! Motion is part of the design, not an afterthought, and it is *bounded*:
//! a preset chooses from a small set of effects PowerPoint, Keynote and our
//! own preview all play; entrance effects start by themselves in a fixed
//! order (no click needed to reveal a slide's content), total motion per
//! slide stays under about three seconds, nothing loops, nothing makes a
//! sound. Transitions that need a newer PowerPoint ride in
//! `mc:AlternateContent` with a plain `fade` fallback, so an older viewer
//! sees a quieter version of the same slide, never a broken one.
//!
//! Matrix classification (`21_Office_Feature_Matrix.csv`): authored motion
//! is the PPTX "Author: transitions and entrance animations" row, produced
//! only by this module; motion already present in a file Harbor did not
//! make stays PRESERVE_ONLY and is never rewritten.

use std::fmt::Write as _;

use super::model::LayoutKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionPreset {
    /// No transitions, no animations.
    None,
    /// Fades only; nothing travels. Board, investor and report decks.
    Subtle,
    /// Pushes, floats and zooms. Launches, workshops, all-hands.
    Dynamic,
    /// Slower, theatrical: ripple/vortex/morph transitions, staged builds.
    Cinematic,
}

impl MotionPreset {
    pub const ALL: [MotionPreset; 4] = [
        MotionPreset::None,
        MotionPreset::Subtle,
        MotionPreset::Dynamic,
        MotionPreset::Cinematic,
    ];

    pub fn id(self) -> &'static str {
        match self {
            MotionPreset::None => "none",
            MotionPreset::Subtle => "subtle",
            MotionPreset::Dynamic => "dynamic",
            MotionPreset::Cinematic => "cinematic",
        }
    }

    pub fn parse(s: &str) -> Option<MotionPreset> {
        MotionPreset::ALL.iter().copied().find(|m| m.id() == s)
    }
}

/// What a slide element is for; the plan animates by role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Deco,
    Kicker,
    Title,
    Subtitle,
    /// The main text body (bullets).
    Body,
    /// The n-th card / stat / milestone / person (a card is several
    /// elements sharing one index).
    Item(usize),
    Chart,
    Image,
    Footer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElKind {
    /// A shape with a text body.
    Text,
    /// A shape without text (card background, marker).
    Plain,
    Picture,
    Chart,
}

/// One animatable element of a built slide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub id: u32,
    pub role: Role,
    pub kind: ElKind,
    /// Paragraph count (for by-paragraph builds).
    pub paras: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    fn wipe(self) -> &'static str {
        match self {
            Dir::Left => "wipe(left)",
            Dir::Right => "wipe(right)",
            Dir::Up => "wipe(up)",
            Dir::Down => "wipe(down)",
        }
    }
    fn subtype(self) -> u32 {
        match self {
            Dir::Up => 1,
            Dir::Right => 2,
            Dir::Down => 4,
            Dir::Left => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Appear,
    Fade,
    /// Rise into place while fading in (PowerPoint "Float In").
    Float,
    Wipe(Dir),
    Zoom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Starts by itself, after the slide appears / the previous step.
    AfterPrev,
    /// Starts together with the previous step.
    WithPrev,
    /// Waits for the presenter.
    Click,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anim {
    pub target: u32,
    /// Animate only this paragraph of the target.
    pub para: Option<u32>,
    pub effect: Effect,
    pub trigger: Trigger,
    pub delay_ms: u32,
    pub dur_ms: u32,
}

/// Total time a plan keeps the slide busy.
pub fn total_ms(plan: &[Anim]) -> u32 {
    plan.iter()
        .map(|a| a.delay_ms + a.dur_ms)
        .max()
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Planning

/// Entrance animations for a slide, by preset. Deterministic.
pub fn plan(
    preset: MotionPreset,
    layout: LayoutKind,
    targets: &[Target],
    progressive: bool,
) -> Vec<Anim> {
    if preset == MotionPreset::None {
        return Vec::new();
    }
    let (title_fx, title_ms, item_fx, item_ms, stagger, para_fx, para_ms, para_stagger, chart_ms) =
        match preset {
            MotionPreset::None => unreachable!(),
            MotionPreset::Subtle => (
                Effect::Fade,
                450,
                Effect::Fade,
                450,
                140,
                Effect::Fade,
                380,
                170,
                700,
            ),
            MotionPreset::Dynamic => (
                Effect::Float,
                500,
                Effect::Float,
                460,
                130,
                Effect::Float,
                420,
                150,
                800,
            ),
            MotionPreset::Cinematic => (
                Effect::Float,
                700,
                Effect::Zoom,
                620,
                180,
                Effect::Float,
                560,
                210,
                1000,
            ),
        };
    let mut out: Vec<Anim> = Vec::new();
    let mut first = true;
    let mut push = |out: &mut Vec<Anim>, mut a: Anim| {
        a.trigger = if first { Trigger::AfterPrev } else { a.trigger };
        first = false;
        out.push(a);
    };
    let hero = matches!(
        layout,
        LayoutKind::Cover | LayoutKind::Closing | LayoutKind::Section
    );
    let by = |role: Role| targets.iter().filter(move |t| t.role == role);

    for t in by(Role::Kicker) {
        push(
            &mut out,
            Anim {
                target: t.id,
                para: None,
                effect: Effect::Fade,
                trigger: Trigger::WithPrev,
                delay_ms: 0,
                dur_ms: 400,
            },
        );
    }
    for t in by(Role::Title) {
        let effect = if hero && preset == MotionPreset::Cinematic {
            Effect::Wipe(Dir::Left)
        } else {
            title_fx
        };
        push(
            &mut out,
            Anim {
                target: t.id,
                para: None,
                effect,
                trigger: Trigger::WithPrev,
                delay_ms: 60,
                dur_ms: title_ms,
            },
        );
    }
    for t in by(Role::Subtitle) {
        push(
            &mut out,
            Anim {
                target: t.id,
                para: None,
                effect: Effect::Fade,
                trigger: Trigger::WithPrev,
                delay_ms: title_ms / 2 + 200,
                dur_ms: 500,
            },
        );
    }
    // Body text: paragraph by paragraph.
    for t in by(Role::Body).filter(|t| t.kind == ElKind::Text) {
        let n = t.paras.max(1);
        for p in 0..n.min(10) {
            push(
                &mut out,
                Anim {
                    target: t.id,
                    para: if n > 1 { Some(p) } else { None },
                    effect: para_fx,
                    trigger: if progressive {
                        Trigger::Click
                    } else {
                        Trigger::WithPrev
                    },
                    delay_ms: if progressive {
                        0
                    } else {
                        260 + p * para_stagger
                    },
                    dur_ms: para_ms,
                },
            );
        }
    }
    // Cards, stats, milestones: each index reveals its elements together.
    let max_item = targets
        .iter()
        .filter_map(|t| match t.role {
            Role::Item(i) => Some(i),
            _ => None,
        })
        .max();
    if let Some(max_item) = max_item {
        for i in 0..=max_item {
            let delay = 260 + i as u32 * stagger;
            for t in targets.iter().filter(|t| t.role == Role::Item(i)) {
                let effect = match (t.kind, preset) {
                    (ElKind::Picture, _) => Effect::Fade,
                    _ => item_fx,
                };
                push(
                    &mut out,
                    Anim {
                        target: t.id,
                        para: None,
                        effect,
                        trigger: Trigger::WithPrev,
                        delay_ms: delay,
                        dur_ms: item_ms,
                    },
                );
            }
        }
    }
    for t in by(Role::Chart) {
        push(
            &mut out,
            Anim {
                target: t.id,
                para: None,
                effect: Effect::Wipe(Dir::Left),
                trigger: Trigger::WithPrev,
                delay_ms: 300,
                dur_ms: chart_ms,
            },
        );
    }
    for t in by(Role::Image) {
        push(
            &mut out,
            Anim {
                target: t.id,
                para: None,
                effect: if preset == MotionPreset::Cinematic {
                    Effect::Zoom
                } else {
                    Effect::Fade
                },
                trigger: Trigger::WithPrev,
                delay_ms: 200,
                dur_ms: 650,
            },
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Transitions

const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const P14: &str = "http://schemas.microsoft.com/office/powerpoint/2010/main";
const P159: &str = "http://schemas.microsoft.com/office/powerpoint/2015/09/main";

fn plain(dur: u32, inner: &str) -> String {
    let spd = if dur >= 900 {
        "slow"
    } else if dur <= 500 {
        "fast"
    } else {
        "med"
    };
    format!("<p:transition spd=\"{spd}\">{inner}</p:transition>")
}

/// A transition that needs an extension namespace, with a `fade` fallback.
/// `p14:dur` is declared once: a repeated xmlns on one element is not
/// well-formed XML and PowerPoint refuses the slide.
fn extended(ns_prefix: &str, ns_uri: &str, dur: u32, inner: &str) -> String {
    let spd = if dur >= 900 { "slow" } else { "med" };
    let decl = if ns_prefix == "p14" {
        format!("xmlns:p14=\"{P14}\"")
    } else {
        format!("xmlns:{ns_prefix}=\"{ns_uri}\" xmlns:p14=\"{P14}\"")
    };
    format!(
        "<mc:AlternateContent xmlns:mc=\"{MC}\"><mc:Choice {decl} Requires=\"{ns_prefix}\"><p:transition spd=\"{spd}\" p14:dur=\"{dur}\">{inner}</p:transition></mc:Choice><mc:Fallback><p:transition spd=\"{spd}\"><p:fade/></p:transition></mc:Fallback></mc:AlternateContent>"
    )
}

/// The transition into the slide at `index` (0-based) with `layout`.
/// Empty string = none.
pub fn transition_xml(preset: MotionPreset, layout: LayoutKind, index: usize, rtl: bool) -> String {
    let hero = matches!(layout, LayoutKind::Cover | LayoutKind::Closing);
    let travel = if rtl { "r" } else { "l" };
    match preset {
        MotionPreset::None => String::new(),
        MotionPreset::Subtle => plain(
            if hero || layout == LayoutKind::Section {
                900
            } else {
                600
            },
            "<p:fade/>",
        ),
        MotionPreset::Dynamic => match layout {
            LayoutKind::Cover | LayoutKind::Closing => plain(900, "<p:fade/>"),
            LayoutKind::Section => plain(750, "<p:zoom dir=\"in\"/>"),
            LayoutKind::Chart => plain(700, "<p:fade/>"),
            // Alternate push and wipe so a long deck does not feel mechanical.
            _ if index.is_multiple_of(2) => plain(650, &format!("<p:push dir=\"{travel}\"/>")),
            _ => plain(
                650,
                &format!("<p:wipe dir=\"{}\"/>", if rtl { "l" } else { "r" }),
            ),
        },
        MotionPreset::Cinematic => match layout {
            LayoutKind::Cover => extended("p14", P14, 1400, "<p14:ripple/>"),
            LayoutKind::Closing => extended("p14", P14, 1400, "<p14:reveal/>"),
            LayoutKind::Section => extended(
                "p14",
                P14,
                1200,
                &format!("<p14:vortex dir=\"{}\"/>", if rtl { "l" } else { "r" }),
            ),
            LayoutKind::Chart => plain(800, "<p:fade/>"),
            _ => extended("p159", P159, 1000, "<p159:morph option=\"byObject\"/>"),
        },
    }
}

// ---------------------------------------------------------------------------
// Timing tree

struct Ids(u32);
impl Ids {
    fn next(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

fn tgt(a: &Anim) -> String {
    match a.para {
        Some(p) => format!(
            "<p:tgtEl><p:spTgt spid=\"{}\"><p:txEl><p:pRg st=\"{p}\" end=\"{p}\"/></p:txEl></p:spTgt></p:tgtEl>",
            a.target
        ),
        None => format!("<p:tgtEl><p:spTgt spid=\"{}\"/></p:tgtEl>", a.target),
    }
}

fn set_visible(ids: &mut Ids, a: &Anim) -> String {
    format!(
        "<p:set><p:cBhvr><p:cTn id=\"{}\" dur=\"1\" fill=\"hold\"><p:stCondLst><p:cond delay=\"0\"/></p:stCondLst></p:cTn>{}<p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val=\"visible\"/></p:to></p:set>",
        ids.next(),
        tgt(a)
    )
}

fn anim_effect(ids: &mut Ids, a: &Anim, filter: &str) -> String {
    format!(
        "<p:animEffect transition=\"in\" filter=\"{filter}\"><p:cBhvr><p:cTn id=\"{}\" dur=\"{}\"/>{}</p:cBhvr></p:animEffect>",
        ids.next(),
        a.dur_ms,
        tgt(a)
    )
}

fn anim_prop(ids: &mut Ids, a: &Anim, attr: &str, from: &str, to: &str) -> String {
    format!(
        "<p:anim calcmode=\"lin\" valueType=\"num\"><p:cBhvr additive=\"base\"><p:cTn id=\"{}\" dur=\"{}\" fill=\"hold\"/>{}<p:attrNameLst><p:attrName>{attr}</p:attrName></p:attrNameLst></p:cBhvr><p:tavLst><p:tav tm=\"0\"><p:val><p:strVal val=\"{from}\"/></p:val></p:tav><p:tav tm=\"100000\"><p:val><p:strVal val=\"{to}\"/></p:val></p:tav></p:tavLst></p:anim>",
        ids.next(),
        a.dur_ms,
        tgt(a)
    )
}

fn effect_xml(ids: &mut Ids, a: &Anim) -> String {
    let node = match a.trigger {
        Trigger::AfterPrev => "afterEffect",
        Trigger::WithPrev => "withEffect",
        Trigger::Click => "clickEffect",
    };
    let (preset_id, subtype) = match a.effect {
        Effect::Appear => (1, 0),
        Effect::Fade => (10, 0),
        Effect::Float => (42, 0),
        Effect::Wipe(d) => (22, d.subtype()),
        Effect::Zoom => (53, 16),
    };
    let ctn = ids.next();
    let mut inner = set_visible(ids, a);
    match a.effect {
        Effect::Appear => {}
        Effect::Fade => inner.push_str(&anim_effect(ids, a, "fade")),
        Effect::Float => {
            inner.push_str(&anim_prop(ids, a, "ppt_y", "#ppt_y+.1", "#ppt_y"));
            inner.push_str(&anim_effect(ids, a, "fade"));
        }
        Effect::Wipe(d) => inner.push_str(&anim_effect(ids, a, d.wipe())),
        Effect::Zoom => {
            inner.push_str(&anim_prop(ids, a, "ppt_w", "0", "#ppt_w"));
            inner.push_str(&anim_prop(ids, a, "ppt_h", "0", "#ppt_h"));
            inner.push_str(&anim_effect(ids, a, "fade"));
        }
    }
    format!(
        "<p:par><p:cTn id=\"{ctn}\" presetID=\"{preset_id}\" presetClass=\"entr\" presetSubtype=\"{subtype}\" fill=\"hold\" grpId=\"0\" nodeType=\"{node}\"><p:stCondLst><p:cond delay=\"{}\"/></p:stCondLst><p:childTnLst>{inner}</p:childTnLst></p:cTn></p:par>",
        a.delay_ms
    )
}

/// The `p:timing` element for a plan (empty string for an empty plan).
/// `targets` supplies the shape kinds the build list needs.
pub fn timing_xml(plan: &[Anim], targets: &[Target]) -> String {
    if plan.is_empty() {
        return String::new();
    }
    let mut ids = Ids(2);
    // Group the plan into click groups: a Click anim starts a new group;
    // everything else belongs to the current one.
    let mut groups: Vec<Vec<&Anim>> = Vec::new();
    for a in plan {
        if groups.is_empty() || a.trigger == Trigger::Click {
            groups.push(vec![a]);
        } else {
            groups.last_mut().unwrap().push(a);
        }
    }
    let mut body = String::new();
    for (gi, g) in groups.iter().enumerate() {
        let auto = g[0].trigger == Trigger::AfterPrev && gi == 0;
        let outer = ids.next();
        let slot = ids.next();
        let mut effects = String::new();
        for a in g {
            effects.push_str(&effect_xml(&mut ids, a));
        }
        let start = if auto {
            "<p:cond delay=\"indefinite\"/><p:cond evt=\"onBegin\" delay=\"0\"><p:tn val=\"2\"/></p:cond>"
        } else {
            "<p:cond delay=\"indefinite\"/>"
        };
        let _ = write!(
            body,
            "<p:par><p:cTn id=\"{outer}\" fill=\"hold\"><p:stCondLst>{start}</p:stCondLst><p:childTnLst><p:par><p:cTn id=\"{slot}\" fill=\"hold\"><p:stCondLst><p:cond delay=\"0\"/></p:stCondLst><p:childTnLst>{effects}</p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>"
        );
    }
    // Build list: text shapes (and charts) PowerPoint animates need a build
    // entry; by-paragraph builds say so.
    let mut bld = String::new();
    let mut seen: Vec<u32> = Vec::new();
    for a in plan {
        if seen.contains(&a.target) {
            continue;
        }
        seen.push(a.target);
        let Some(t) = targets.iter().find(|t| t.id == a.target) else {
            continue;
        };
        match t.kind {
            ElKind::Text => {
                let by_para = plan
                    .iter()
                    .any(|x| x.target == a.target && x.para.is_some());
                let _ = write!(
                    bld,
                    "<p:bldP spid=\"{}\" grpId=\"0\"{}{}/>",
                    a.target,
                    if matches!(t.role, Role::Item(_)) {
                        " animBg=\"1\""
                    } else {
                        ""
                    },
                    if by_para { " build=\"p\"" } else { "" }
                );
            }
            ElKind::Chart => {
                let _ = write!(
                    bld,
                    "<p:bldGraphic spid=\"{}\" grpId=\"0\"><p:bldAsOne/></p:bldGraphic>",
                    a.target
                );
            }
            ElKind::Plain => {
                let _ = write!(
                    bld,
                    "<p:bldP spid=\"{}\" grpId=\"0\" animBg=\"1\"/>",
                    a.target
                );
            }
            ElKind::Picture => {}
        }
    }
    let bld = if bld.is_empty() {
        String::new()
    } else {
        format!("<p:bldLst>{bld}</p:bldLst>")
    };
    format!(
        "<p:timing><p:tnLst><p:par><p:cTn id=\"1\" dur=\"indefinite\" restart=\"never\" nodeType=\"tmRoot\"><p:childTnLst><p:seq concurrent=\"1\" nextAc=\"seek\"><p:cTn id=\"2\" dur=\"indefinite\" nodeType=\"mainSeq\"><p:childTnLst>{body}</p:childTnLst></p:cTn><p:prevCondLst><p:cond evt=\"onPrev\" delay=\"0\"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt=\"onNext\" delay=\"0\"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst>{bld}</p:timing>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets() -> Vec<Target> {
        vec![
            Target {
                id: 2,
                role: Role::Title,
                kind: ElKind::Text,
                paras: 1,
            },
            Target {
                id: 3,
                role: Role::Body,
                kind: ElKind::Text,
                paras: 3,
            },
            Target {
                id: 4,
                role: Role::Item(0),
                kind: ElKind::Plain,
                paras: 0,
            },
            Target {
                id: 5,
                role: Role::Item(0),
                kind: ElKind::Text,
                paras: 2,
            },
            Target {
                id: 6,
                role: Role::Item(1),
                kind: ElKind::Text,
                paras: 2,
            },
            Target {
                id: 7,
                role: Role::Chart,
                kind: ElKind::Chart,
                paras: 0,
            },
        ]
    }

    #[test]
    fn none_means_nothing_and_every_other_preset_starts_by_itself() {
        assert!(plan(MotionPreset::None, LayoutKind::Cards, &targets(), false).is_empty());
        assert_eq!(timing_xml(&[], &targets()), "");
        for p in [
            MotionPreset::Subtle,
            MotionPreset::Dynamic,
            MotionPreset::Cinematic,
        ] {
            let plan = plan(p, LayoutKind::Cards, &targets(), false);
            assert!(!plan.is_empty());
            assert_eq!(
                plan[0].trigger,
                Trigger::AfterPrev,
                "{p:?}: content shows without a click"
            );
            assert!(
                plan.iter().all(|a| a.trigger != Trigger::Click),
                "{p:?}: nothing waits for a click unless progressive"
            );
        }
    }

    #[test]
    fn motion_is_bounded() {
        for p in [
            MotionPreset::Subtle,
            MotionPreset::Dynamic,
            MotionPreset::Cinematic,
        ] {
            for l in LayoutKind::ALL {
                let t = plan(p, l, &targets(), false);
                assert!(total_ms(&t) <= 3200, "{p:?} {l:?}: {} ms", total_ms(&t));
            }
        }
        // And a long bullet list stays inside the budget too (ten paragraphs).
        let long = vec![Target {
            id: 3,
            role: Role::Body,
            kind: ElKind::Text,
            paras: 12,
        }];
        for p in [
            MotionPreset::Subtle,
            MotionPreset::Dynamic,
            MotionPreset::Cinematic,
        ] {
            assert!(
                total_ms(&plan(p, LayoutKind::Bullets, &long, false)) <= 3200,
                "{p:?}"
            );
        }
    }

    #[test]
    fn items_reveal_in_order_and_elements_of_one_card_start_together() {
        let p = plan(MotionPreset::Dynamic, LayoutKind::Cards, &targets(), false);
        let delay = |id| p.iter().find(|a| a.target == id).unwrap().delay_ms;
        assert_eq!(delay(4), delay(5), "card background and text together");
        assert!(delay(6) > delay(5), "second card after the first");
    }

    #[test]
    fn progressive_bullets_wait_for_clicks_one_paragraph_each() {
        let p = plan(MotionPreset::Subtle, LayoutKind::Bullets, &targets(), true);
        let body: Vec<_> = p.iter().filter(|a| a.target == 3).collect();
        assert_eq!(body.len(), 3);
        assert!(body.iter().all(|a| a.trigger == Trigger::Click));
        assert_eq!(
            body.iter().map(|a| a.para).collect::<Vec<_>>(),
            vec![Some(0), Some(1), Some(2)]
        );
        let xml = timing_xml(&p, &targets());
        assert_eq!(xml.matches("nodeType=\"clickEffect\"").count(), 3);
        assert!(xml.contains("build=\"p\""));
    }

    #[test]
    fn timing_tree_is_well_formed_with_unique_increasing_ids() {
        for p in [
            MotionPreset::Subtle,
            MotionPreset::Dynamic,
            MotionPreset::Cinematic,
        ] {
            let pl = plan(p, LayoutKind::Cards, &targets(), false);
            let xml = timing_xml(&pl, &targets());
            let doc = format!(
                "<p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:a=\"a\">{xml}</p:sld>"
            );
            let parsed =
                roxmltree::Document::parse(&doc).unwrap_or_else(|e| panic!("{p:?}: {e}\n{xml}"));
            let ids: Vec<u32> = parsed
                .descendants()
                .filter(|n| n.has_tag_name("cTn"))
                .map(|n| n.attribute("id").unwrap().parse().unwrap())
                .collect();
            let mut sorted = ids.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), ids.len(), "{p:?}: cTn ids unique");
            // The first group auto-starts off the main sequence node (id 2).
            assert!(xml.contains("<p:cond evt=\"onBegin\" delay=\"0\"><p:tn val=\"2\"/></p:cond>"));
            // Every build-list target is animated.
            assert!(xml.contains("<p:bldLst>"));
            assert!(xml.contains("<p:bldGraphic spid=\"7\""));
        }
    }

    #[test]
    fn transitions_are_valid_and_extensions_carry_a_fallback() {
        for preset in MotionPreset::ALL {
            for layout in LayoutKind::ALL {
                for rtl in [false, true] {
                    let x = transition_xml(preset, layout, 3, rtl);
                    if x.is_empty() {
                        assert_eq!(preset, MotionPreset::None);
                        continue;
                    }
                    let doc = format!(
                        "<p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">{x}</p:sld>"
                    );
                    roxmltree::Document::parse(&doc)
                        .unwrap_or_else(|e| panic!("{preset:?} {layout:?}: {e}\n{x}"));
                    if x.contains("mc:Choice") {
                        assert!(
                            x.contains("<mc:Fallback><p:transition spd=\"")
                                && x.contains("<p:fade/>")
                        );
                        assert!(x.contains("Requires=\"p14\"") || x.contains("Requires=\"p159\""));
                    }
                }
            }
        }
        assert!(transition_xml(MotionPreset::None, LayoutKind::Cover, 0, false).is_empty());
    }

    #[test]
    fn rtl_flips_travel_direction() {
        let ltr = transition_xml(MotionPreset::Dynamic, LayoutKind::Bullets, 2, false);
        let rtl = transition_xml(MotionPreset::Dynamic, LayoutKind::Bullets, 2, true);
        assert!(ltr.contains("dir=\"l\"") && rtl.contains("dir=\"r\""));
    }

    #[test]
    fn preset_ids_round_trip() {
        for p in MotionPreset::ALL {
            assert_eq!(MotionPreset::parse(p.id()), Some(p));
        }
        assert_eq!(MotionPreset::parse("wild"), None);
    }
}
