//! Slide transitions (`p:transition`, incl. the `p14`/`p15`/`p159`
//! extension forms inside `mc:AlternateContent`) and entrance/exit/emphasis
//! animations (`p:timing`) read into the scene's motion types.
//!
//! The timing tree is PowerPoint's: a main sequence of *click groups*, each
//! holding *time slots*, each holding *effects*. We flatten it to the play
//! order a presenter needs (`trigger` + `delay_ms` + `dur_ms`). Effects are
//! classified from the behaviours they contain first (what actually moves
//! on screen), and from `presetID` only as a fallback, so authored files
//! that use an unfamiliar preset id still preview as what they do.

use roxmltree::Node;

use crate::scene::{Animation, Transition};

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

/// The transition of a slide, preferring the extension form (`Choice`) a
/// modern PowerPoint writes, then the plain one (`Fallback` or bare).
pub(crate) fn parse_transition(slide_root: Node) -> Option<Transition> {
    let mut candidates: Vec<Node> = slide_root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "transition")
        .collect();
    // Choice-branch transitions first.
    candidates.sort_by_key(|n| {
        let in_choice = n
            .ancestors()
            .any(|a| a.is_element() && a.tag_name().name() == "Choice");
        if in_choice {
            0
        } else {
            1
        }
    });
    for t in candidates {
        if let Some(parsed) = transition_from(t) {
            return Some(parsed);
        }
    }
    None
}

fn transition_from(t: Node) -> Option<Transition> {
    let spd_ms = match t.attribute("spd") {
        Some("slow") => 1000,
        Some("fast") => 500,
        _ => 750,
    };
    let dur_ms = t
        .attributes()
        .find(|a| a.name() == "dur")
        .and_then(|a| a.value().parse::<u32>().ok())
        .unwrap_or(spd_ms)
        .clamp(100, 10_000);
    let on_click = t.attribute("advClick").map(|v| v != "0").unwrap_or(true);
    let after_ms = t.attribute("advTm").and_then(|v| v.parse::<u32>().ok());

    let effect = t.children().find(|c| c.is_element())?;
    let name = effect.tag_name().name();
    let dir = effect.attribute("dir").map(str::to_string);
    let mut kind = match name {
        "fade" => {
            if effect.attribute("thruBlk") == Some("1") {
                "fade_through_black"
            } else {
                "fade"
            }
        }
        "push" => "push",
        "wipe" => "wipe",
        "split" => "split",
        "cover" => "cover",
        "pull" => "uncover",
        "zoom" => "zoom",
        "random" => "random",
        "morph" => "morph",
        "ripple" => "ripple",
        "reveal" => "reveal",
        "flip" => "flip",
        "vortex" => "vortex",
        "prism" => "prism",
        "glitter" => "glitter",
        "honeycomb" => "honeycomb",
        "doors" => "doors",
        "window" => "window",
        "ferris" => "ferris",
        "gallery" => "gallery",
        "conveyor" => "conveyor",
        "pan" => "pan",
        "warp" => "warp",
        "flythrough" => "flythrough",
        "flash" => "flash",
        "shred" => "shred",
        "prstTrans" => "other",
        _ => "other",
    }
    .to_string();
    if name == "split" {
        // `orient` is the split axis; keep it as the direction.
        return Some(Transition {
            kind,
            dir: effect
                .attribute("orient")
                .map(|o| if o == "vert" { "vert" } else { "horz" }.to_string())
                .or(dir),
            dur_ms,
            on_click,
            after_ms,
        });
    }
    if name == "prstTrans" {
        kind = "other".into();
    }
    Some(Transition {
        kind,
        dir,
        dur_ms,
        on_click,
        after_ms,
    })
}

/// Play-ordered animations of a slide.
pub(crate) fn parse_animations(slide_root: Node) -> Vec<Animation> {
    let mut out = Vec::new();
    let Some(timing) = slide_root
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "timing")
    else {
        return out;
    };
    // The main sequence: the `cTn` marked nodeType="mainSeq".
    let Some(main) = timing.descendants().find(|n| {
        n.is_element() && n.tag_name().name() == "cTn" && n.attribute("nodeType") == Some("mainSeq")
    }) else {
        return out;
    };
    let Some(children) = child(main, "childTnLst") else {
        return out;
    };
    for click_group in children
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "par")
    {
        // click_group > cTn > childTnLst > par (time slot) > cTn > childTnLst > par (effect) > cTn
        for effect_ctn in click_group.descendants().filter(|n| {
            n.is_element() && n.tag_name().name() == "cTn" && n.attribute("presetClass").is_some()
        }) {
            if let Some(a) = animation_from(effect_ctn) {
                out.push(a);
            }
            if out.len() >= 256 {
                return out;
            }
        }
    }
    out
}

fn animation_from(ctn: Node) -> Option<Animation> {
    let phase = match ctn.attribute("presetClass")? {
        "entr" => "entrance",
        "exit" => "exit",
        "emph" => "emphasis",
        _ => return None,
    };
    let trigger = match ctn.attribute("nodeType") {
        Some("withEffect") => "with_prev",
        Some("afterEffect") => "after_prev",
        _ => "click",
    };
    // Target shape (and optional paragraph range) from the first spTgt.
    let sp_tgt = ctn
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "spTgt")?;
    let shape_id = sp_tgt.attribute("spid")?.parse::<u32>().ok()?;
    let para = child(sp_tgt, "txEl")
        .and_then(|t| child(t, "pRg"))
        .and_then(|r| {
            Some([
                r.attribute("st")?.parse::<u32>().ok()?,
                r.attribute("end")?.parse::<u32>().ok()?,
            ])
        });
    // The effect's own start delay.
    let delay_ms = child(ctn, "stCondLst")
        .and_then(|l| child(l, "cond"))
        .and_then(|c| c.attribute("delay"))
        .and_then(|d| d.parse::<u32>().ok())
        .unwrap_or(0);
    // Duration: the longest behaviour duration inside.
    let dur_ms = ctn
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "cTn")
        .filter_map(|n| n.attribute("dur").and_then(|d| d.parse::<u32>().ok()))
        .max()
        .unwrap_or(0)
        .min(30_000);

    // Classify from behaviours.
    let mut effect = None::<&str>;
    let mut dir = None::<String>;
    let mut moves_x = false;
    let mut moves_y = false;
    let mut scales = false;
    for n in ctn.descendants().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "animEffect" => {
                let filter = n.attribute("filter").unwrap_or("");
                if filter.starts_with("fade") {
                    effect.get_or_insert("fade");
                } else if let Some(rest) = filter.strip_prefix("wipe(") {
                    effect = Some("wipe");
                    // The contract's direction vocabulary: the edge the
                    // reveal starts from, as `l r u d`.
                    dir = Some(
                        match rest.trim_end_matches(')') {
                            "left" => "l",
                            "right" => "r",
                            "up" => "u",
                            "down" => "d",
                            other => other,
                        }
                        .to_string(),
                    );
                } else if filter.starts_with("split") {
                    effect = Some("split");
                } else if filter.starts_with("wheel") {
                    effect = Some("wheel");
                }
            }
            "anim" => {
                let attrs: String = n
                    .descendants()
                    .filter(|a| a.is_element() && a.tag_name().name() == "attrName")
                    .filter_map(|a| a.text())
                    .collect::<Vec<_>>()
                    .join(",");
                if attrs.contains("ppt_x") {
                    moves_x = true;
                }
                if attrs.contains("ppt_y") {
                    moves_y = true;
                }
                if attrs.contains("ppt_w") || attrs.contains("ppt_h") {
                    scales = true;
                }
            }
            "animRot" => {
                effect.get_or_insert("spin");
            }
            "animScale" => scales = true,
            _ => {}
        }
    }
    let by_preset = match ctn
        .attribute("presetID")
        .and_then(|v| v.parse::<u32>().ok())
    {
        Some(1) => Some("appear"),
        Some(2) => Some("fly"),
        Some(10) => Some("fade"),
        Some(16) => Some("split"),
        Some(21) => Some("wheel"),
        Some(22) => Some("wipe"),
        Some(23) | Some(53) => Some("zoom"),
        Some(31) => Some("grow"),
        Some(42) => Some("float"),
        _ => None,
    };
    let effect = match effect {
        Some("fade") if moves_y && !moves_x => "float",
        Some("fade") if scales => "zoom",
        Some(e) => e,
        None if moves_x || moves_y => "fly",
        None if scales => "zoom",
        None => by_preset.unwrap_or(if dur_ms == 0 { "appear" } else { "other" }),
    };
    if dir.is_none() {
        // presetSubtype carries the direction for fly/wipe: 1 up, 2 right,
        // 4 down, 8 left (the side it comes from / moves toward).
        dir = match ctn.attribute("presetSubtype") {
            Some("1") => Some("u".to_string()),
            Some("2") => Some("r".to_string()),
            Some("4") => Some("d".to_string()),
            Some("8") => Some("l".to_string()),
            _ => None,
        };
    }
    Some(Animation {
        shape_id,
        para,
        effect: effect.to_string(),
        phase: phase.to_string(),
        dir,
        trigger: trigger.to_string(),
        delay_ms,
        dur_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="a" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main""#;

    #[test]
    fn transitions_prefer_the_extension_form_and_read_timing() {
        let xml = format!(
            r#"<p:sld {P}><mc:AlternateContent><mc:Choice Requires="p14"><p:transition spd="med" p14:dur="900" advTm="4000"><p14:vortex dir="r"/></p:transition></mc:Choice><mc:Fallback><p:transition spd="med"><p:fade/></p:transition></mc:Fallback></mc:AlternateContent></p:sld>"#
        );
        let d = roxmltree::Document::parse(&xml).unwrap();
        let t = parse_transition(d.root_element()).unwrap();
        assert_eq!(t.kind, "vortex");
        assert_eq!(t.dir.as_deref(), Some("r"));
        assert_eq!(t.dur_ms, 900);
        assert_eq!(t.after_ms, Some(4000));
        assert!(t.on_click);
    }

    #[test]
    fn plain_transitions_and_speeds() {
        for (inner, spd, kind, dur) in [
            (
                "<p:fade thruBlk=\"1\"/>",
                "slow",
                "fade_through_black",
                1000,
            ),
            ("<p:push dir=\"u\"/>", "fast", "push", 500),
            ("<p:pull dir=\"l\"/>", "med", "uncover", 750),
            ("<p:wheel spokes=\"4\"/>", "med", "other", 750),
        ] {
            let xml =
                format!(r#"<p:sld {P}><p:transition spd="{spd}">{inner}</p:transition></p:sld>"#);
            let d = roxmltree::Document::parse(&xml).unwrap();
            let t = parse_transition(d.root_element()).unwrap();
            assert_eq!((t.kind.as_str(), t.dur_ms), (kind, dur), "{inner}");
        }
        let none_xml = format!("<p:sld {P}/>");
        let none = roxmltree::Document::parse(&none_xml).unwrap();
        assert!(parse_transition(none.root_element()).is_none());
    }

    fn timing(effects: &str) -> String {
        format!(
            r#"<p:sld {P}><p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst>
            <p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>{effects}</p:childTnLst></p:cTn></p:seq>
            </p:childTnLst></p:cTn></p:par></p:tnLst></p:timing></p:sld>"#
        )
    }

    fn fade_effect(id: u32, spid: u32, node: &str, delay: u32) -> String {
        format!(
            r#"<p:par><p:cTn id="{id}" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="{}" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>
            <p:par><p:cTn id="{}" presetID="10" presetClass="entr" presetSubtype="0" fill="hold" nodeType="{node}"><p:stCondLst><p:cond delay="{delay}"/></p:stCondLst><p:childTnLst>
            <p:set><p:cBhvr><p:cTn id="{}" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="visible"/></p:to></p:set>
            <p:animEffect transition="in" filter="fade"><p:cBhvr><p:cTn id="{}" dur="500"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cBhvr></p:animEffect>
            </p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>"#,
            id + 1,
            id + 2,
            id + 3,
            id + 4,
        )
    }

    #[test]
    fn click_and_automatic_fades_flatten_in_play_order() {
        let xml = timing(&format!(
            "{}{}",
            fade_effect(3, 4, "afterEffect", 0),
            fade_effect(20, 5, "clickEffect", 250)
        ));
        let d = roxmltree::Document::parse(&xml).unwrap();
        let a = parse_animations(d.root_element());
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].shape_id, a[0].trigger.as_str()), (4, "after_prev"));
        assert_eq!((a[1].shape_id, a[1].trigger.as_str()), (5, "click"));
        assert_eq!(a[1].delay_ms, 250);
        assert_eq!(a[0].effect, "fade");
        assert_eq!(a[0].phase, "entrance");
        assert_eq!(a[0].dur_ms, 500);
    }

    #[test]
    fn classifies_by_behaviour_and_reads_paragraph_builds() {
        let wipe = r#"<p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="4" fill="hold"><p:childTnLst>
            <p:par><p:cTn id="5" presetID="22" presetClass="entr" presetSubtype="8" fill="hold" nodeType="clickEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>
            <p:animEffect transition="in" filter="wipe(left)"><p:cBhvr><p:cTn id="6" dur="400"/><p:tgtEl><p:spTgt spid="3"><p:txEl><p:pRg st="1" end="2"/></p:txEl></p:spTgt></p:tgtEl></p:cBhvr></p:animEffect>
            </p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>"#;
        let float = r#"<p:par><p:cTn id="13" fill="hold"><p:childTnLst><p:par><p:cTn id="14" fill="hold"><p:childTnLst>
            <p:par><p:cTn id="15" presetID="42" presetClass="entr" presetSubtype="0" fill="hold" nodeType="withEffect"><p:childTnLst>
            <p:anim calcmode="lin" valueType="num"><p:cBhvr><p:cTn id="16" dur="500" fill="hold"/><p:tgtEl><p:spTgt spid="9"/></p:tgtEl><p:attrNameLst><p:attrName>ppt_y</p:attrName></p:attrNameLst></p:cBhvr></p:anim>
            <p:animEffect transition="in" filter="fade"><p:cBhvr><p:cTn id="17" dur="500"/><p:tgtEl><p:spTgt spid="9"/></p:tgtEl></p:cBhvr></p:animEffect>
            </p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>"#;
        let xml = timing(&format!("{wipe}{float}"));
        let d = roxmltree::Document::parse(&xml).unwrap();
        let a = parse_animations(d.root_element());
        assert_eq!(a[0].effect, "wipe");
        assert_eq!(
            a[0].dir.as_deref(),
            Some("l"),
            "wipe(left) → the contract's `l`"
        );
        assert_eq!(a[0].para, Some([1, 2]));
        assert_eq!(a[1].effect, "float", "fade + vertical move is a float-in");
        assert_eq!(a[1].trigger, "with_prev");
    }

    #[test]
    fn no_timing_means_no_animations_and_garbage_is_ignored() {
        let bare = format!("<p:sld {P}/>");
        let d = roxmltree::Document::parse(&bare).unwrap();
        assert!(parse_animations(d.root_element()).is_empty());
        let weird_xml = timing(
            r#"<p:par><p:cTn id="3"><p:childTnLst><p:par><p:cTn id="4" presetClass="entr"/></p:par></p:childTnLst></p:cTn></p:par>"#,
        );
        let weird = roxmltree::Document::parse(&weird_xml).unwrap();
        assert!(
            parse_animations(weird.root_element()).is_empty(),
            "no target shape"
        );
    }
}
