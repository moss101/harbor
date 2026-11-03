//! Theme, master and layout parts for a designed deck.
//!
//! Real PowerPoint structure: every slide sits on a layout that sits on one
//! master, with the theme's colour scheme and font scheme, so "Change
//! layout", "Reset slide" and "Design → Variants" behave as they would on
//! a deck built by hand. Decoration lives in the master and layouts (not
//! on slides), so a slide's own XML stays content only.

use super::grid::*;
use super::header::master_deco;
use super::look::{frame, Header};
use super::ornament::{orb, sp_list};
use super::theme::ThemeSpec;
use super::xml::*;

/// The layouts a designed deck carries, in package order (1-based part
/// numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPart {
    Cover = 1,
    Section = 2,
    TitleContent = 3,
    TwoContent = 4,
    TitleOnly = 5,
    Quote = 6,
    Closing = 7,
}

pub const LAYOUTS: [LayoutPart; 7] = [
    LayoutPart::Cover,
    LayoutPart::Section,
    LayoutPart::TitleContent,
    LayoutPart::TwoContent,
    LayoutPart::TitleOnly,
    LayoutPart::Quote,
    LayoutPart::Closing,
];

pub(crate) const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;

pub(crate) const GROUP: &str = r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#;

pub(crate) const CLR_MAP: &str = r#"<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>"#;
pub(crate) const CLR_MAP_DARK: &str = r#"<p:clrMap bg1="dk1" tx1="lt1" bg2="dk2" tx2="lt2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>"#;
pub(crate) const OVR_DARK: &str = r#"<p:clrMapOvr><a:overrideClrMapping bg1="dk1" tx1="lt1" bg2="dk2" tx2="lt2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:clrMapOvr>"#;
pub(crate) const OVR_MASTER: &str = "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>";

pub fn theme_xml(t: &ThemeSpec) -> String {
    let p = &t.palette;
    let acc = |n: usize| format!("<a:srgbClr val=\"{}\"/>", p.accents[n]);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="{name}">
<a:themeElements>
<a:clrScheme name="{name}"><a:dk1><a:srgbClr val="{dk1}"/></a:dk1><a:lt1><a:srgbClr val="{lt1}"/></a:lt1><a:dk2><a:srgbClr val="{dk2}"/></a:dk2><a:lt2><a:srgbClr val="{lt2}"/></a:lt2><a:accent1>{a1}</a:accent1><a:accent2>{a2}</a:accent2><a:accent3>{a3}</a:accent3><a:accent4>{a4}</a:accent4><a:accent5>{a5}</a:accent5><a:accent6>{a6}</a:accent6><a:hlink>{a1}</a:hlink><a:folHlink>{a4}</a:folHlink></a:clrScheme>
<a:fontScheme name="{name}"><a:majorFont><a:latin typeface="{major}"/><a:ea typeface=""/><a:cs typeface="{cs}"/></a:majorFont><a:minorFont><a:latin typeface="{minor}"/><a:ea typeface=""/><a:cs typeface="{cs}"/></a:minorFont></a:fontScheme>
<a:fmtScheme name="{name}"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>
</a:themeElements>
<a:objectDefaults/><a:extraClrSchemeLst/>
</a:theme>"#,
        name = esc(t.name),
        dk1 = p.dk1,
        lt1 = p.lt1,
        dk2 = p.dk2,
        lt2 = p.lt2,
        a1 = acc(0),
        a2 = acc(1),
        a3 = acc(2),
        a4 = acc(3),
        a5 = acc(4),
        a6 = acc(5),
        major = esc(t.fonts.major),
        minor = esc(t.fonts.minor),
        cs = esc(t.fonts.cs),
    )
}

pub(crate) fn bg_xml(fill: &Fill) -> String {
    format!("<p:bg><p:bgPr>{}<a:effectLst/></p:bgPr></p:bg>", fill.xml())
}

/// Footer and slide-number placeholders (positions and ink follow the
/// header composition; slides decide whether to instantiate them).
pub(crate) fn footer_phs(id0: u32, t: &ThemeSpec, rtl: bool) -> String {
    let f = frame(&t.look);
    let (ftr, num) = if rtl {
        (f.ftr.mirrored(), f.num.mirrored())
    } else {
        (f.ftr, f.num)
    };
    let (l, r) = if rtl { ("r", "l") } else { ("l", "r") };
    // Furniture sitting on a colour field is light; the rail and sidebar
    // carry a large slide number, everything else a quiet small one.
    let (ftr_clr, num_clr, num_sz, num_algn) = match t.look.header {
        Header::Sidebar { .. } => (
            "<a:schemeClr val=\"lt1\"><a:alpha val=\"70000\"/></a:schemeClr>",
            "<a:schemeClr val=\"lt1\"/>",
            2800,
            l,
        ),
        Header::Rail => (
            "<a:schemeClr val=\"tx1\"><a:alpha val=\"62000\"/></a:schemeClr>",
            "<a:schemeClr val=\"accent1\"/>",
            4400,
            l,
        ),
        _ => (
            "<a:schemeClr val=\"tx1\"><a:alpha val=\"62000\"/></a:schemeClr>",
            "<a:schemeClr val=\"accent1\"/>",
            1000,
            r,
        ),
    };
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{a}\" name=\"Footer Placeholder\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"ftr\" sz=\"quarter\" idx=\"11\"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"ctr\"/><a:lstStyle><a:lvl1pPr algn=\"{l}\"><a:defRPr sz=\"1000\" b=\"0\" spc=\"20\"><a:solidFill>{ftr_clr}</a:solidFill></a:defRPr></a:lvl1pPr></a:lstStyle><a:p><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp>\
<p:sp><p:nvSpPr><p:cNvPr id=\"{b}\" name=\"Slide Number Placeholder\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldNum\" sz=\"quarter\" idx=\"12\"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"ctr\"/><a:lstStyle><a:lvl1pPr algn=\"{num_algn}\"><a:defRPr sz=\"{num_sz}\" b=\"1\"><a:solidFill>{num_clr}</a:solidFill></a:defRPr></a:lvl1pPr></a:lstStyle><a:p><a:fld id=\"{{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}}\" type=\"slidenum\"><a:rPr lang=\"en-US\"/><a:t>&#8249;#&#8250;</a:t></a:fld><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp>",
        ftr.x, ftr.y, ftr.w, ftr.h,
        num.x, num.y, num.w, num.h,
        a = id0,
        b = id0 + 1,
    )
}

pub fn master_xml(t: &ThemeSpec, rtl: bool) -> String {
    let f = frame(&t.look);
    let bullet = match t.look.header {
        Header::Rail | Header::Poster => "&#9642;",
        _ => "&#8226;",
    };
    // Titles sitting on a colour field (banner, sidebar) are light.
    let title_ink = if f.on_field {
        "<a:schemeClr val=\"lt1\"/>"
    } else {
        "<a:schemeClr val=\"tx1\"/>"
    };
    let title_algn = match (f.title_align, rtl) {
        (Align::Center, _) => "ctr",
        (_, true) => "r",
        _ => "l",
    };
    let body_lvl = |lvl: u32, mar: i64, sz: u32, bu: &str, clr: &str| {
        format!(
            "<a:lvl{lvl}pPr marL=\"{mar}\" indent=\"-{ind}\" algn=\"{al}\" rtl=\"{rtl}\" defTabSz=\"914400\" eaLnBrk=\"1\" latinLnBrk=\"0\" hangingPunct=\"1\"><a:lnSpc><a:spcPct val=\"104000\"/></a:lnSpc><a:spcBef><a:spcPts val=\"{bef}\"/></a:spcBef><a:buClr><a:schemeClr val=\"{clr}\"/></a:buClr><a:buSzPct val=\"100000\"/><a:buFont typeface=\"Arial\"/><a:buChar char=\"{bu}\"/><a:defRPr sz=\"{sz}\" kern=\"1200\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:latin typeface=\"+mn-lt\"/><a:ea typeface=\"+mn-ea\"/><a:cs typeface=\"+mn-cs\"/></a:defRPr></a:lvl{lvl}pPr>",
            ind = 320_040,
            al = if rtl { "r" } else { "l" },
            rtl = if rtl { 1 } else { 0 },
            bef = if lvl == 1 { 1200 } else { 600 },
        )
    };
    let page = if t.tinted_page {
        Fill::Solid(Clr::bg2())
    } else {
        Fill::Solid(Clr::bg1())
    };
    let deco = master_deco(t, &f);
    let deco_xml = sp_list(&deco, 20, rtl);
    let clr_map = if t.is_dark() { CLR_MAP_DARK } else { CLR_MAP };
    let title_rect = if rtl { f.title.mirrored() } else { f.title };
    let body_rect = if rtl { f.body.mirrored() } else { f.body };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster {NS}>
<p:cSld>{bg}<p:spTree>{GROUP}{deco_xml}
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{tx}" y="{ty}"/><a:ext cx="{tw}" cy="{th}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr vert="horz" lIns="0" tIns="0" rIns="0" bIns="0" rtlCol="0" anchor="{tanchor}"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Click to edit Master title style</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Text Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{bx}" y="{by}"/><a:ext cx="{bw}" cy="{bh}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr vert="horz" lIns="0" tIns="0" rIns="0" bIns="0" rtlCol="0"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:pPr lvl="0"/><a:r><a:rPr lang="en-US"/><a:t>Click to edit Master text styles</a:t></a:r></a:p></p:txBody></p:sp>
{footers}
</p:spTree></p:cSld>
{clr_map}
<p:sldLayoutIdLst>{layout_ids}</p:sldLayoutIdLst>
<p:txStyles>
<p:titleStyle><a:lvl1pPr algn="{tal}" rtl="{rtlv}" defTabSz="914400" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="92000"/></a:lnSpc><a:spcBef><a:spcPct val="0"/></a:spcBef><a:buNone/><a:defRPr sz="{tsz}" b="{tb}" kern="1200"><a:solidFill>{title_ink}</a:solidFill><a:latin typeface="+mj-lt"/><a:ea typeface="+mj-ea"/><a:cs typeface="+mj-cs"/></a:defRPr></a:lvl1pPr></p:titleStyle>
<p:bodyStyle>{l1}{l2}{l3}{l4}{l5}</p:bodyStyle>
<p:otherStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr><a:lvl1pPr marL="0" algn="{al}" rtl="{rtlv}" defTabSz="914400" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1800" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl1pPr></p:otherStyle>
</p:txStyles>
</p:sldMaster>"#,
        bg = bg_xml(&page),
        footers = footer_phs(4, t, rtl),
        layout_ids = LAYOUTS
            .iter()
            .map(|l| format!(
                "<p:sldLayoutId id=\"{}\" r:id=\"rId{}\"/>",
                2147483649u64 + *l as u64 - 1,
                *l as u32
            ))
            .collect::<String>(),
        tx = title_rect.x,
        ty = title_rect.y,
        tw = title_rect.w,
        th = title_rect.h,
        bx = body_rect.x,
        by = body_rect.y,
        bw = body_rect.w,
        bh = body_rect.h,
        al = if rtl { "r" } else { "l" },
        tal = title_algn,
        tsz = (f.title_pt * 100.0) as i64,
        tanchor = f.title_anchor,
        rtlv = if rtl { 1 } else { 0 },
        tb = if t.bold_titles { 1 } else { 0 },
        l1 = body_lvl(1, 320_040, 2200, bullet, "accent1"),
        l2 = body_lvl(2, 686_000, 1900, "&#8211;", "tx2"),
        l3 = body_lvl(3, 1_052_000, 1700, "&#8211;", "tx2"),
        l4 = body_lvl(4, 1_418_000, 1600, "&#8211;", "tx2"),
        l5 = body_lvl(5, 1_784_000, 1600, "&#8211;", "tx2"),
    )
}

pub(crate) fn layout_shell(
    name: &str,
    ty: &str,
    show_master: bool,
    bg: Option<&Fill>,
    inverse: bool,
    shapes: &str,
) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout {NS} type="{ty}" preserve="1"{show}>
<p:cSld name="{name}">{bg}<p:spTree>{GROUP}{shapes}</p:spTree></p:cSld>{ovr}
</p:sldLayout>"#,
        show = if show_master {
            ""
        } else {
            " showMasterSp=\"0\""
        },
        bg = bg.map(bg_xml).unwrap_or_default(),
        ovr = if inverse { OVR_DARK } else { OVR_MASTER },
    )
}

pub(crate) fn ph_title(
    id: u32,
    ty: &str,
    name: &str,
    rect: R,
    lst: &str,
    anchor: &str,
    prompt: &str,
) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"{ty}\"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"{anchor}\"><a:normAutofit/></a:bodyPr>{lst}<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{prompt}</a:t></a:r></a:p></p:txBody></p:sp>",
        rect.x, rect.y, rect.w, rect.h
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ph_body(
    id: u32,
    ty: &str,
    idx: u32,
    name: &str,
    rect: R,
    lst: &str,
    anchor: &str,
    prompt: &str,
) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"{ty}\" idx=\"{idx}\"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"{anchor}\"><a:normAutofit/></a:bodyPr>{lst}<a:p><a:pPr lvl=\"0\"/><a:r><a:rPr lang=\"en-US\"/><a:t>{prompt}</a:t></a:r></a:p></p:txBody></p:sp>",
        rect.x, rect.y, rect.w, rect.h
    )
}

fn content_layout(t: &ThemeSpec, name: &str, ty: &str, two: bool, rtl: bool) -> String {
    let f = frame(&t.look);
    let tr = if rtl { f.title.mirrored() } else { f.title };
    let br = if rtl { f.body.mirrored() } else { f.body };
    let mut shapes = String::new();
    shapes.push_str(&ph_title(
        2,
        "title",
        "Title 1",
        tr,
        "<a:lstStyle/>",
        f.title_anchor,
        "Click to edit Master title style",
    ));
    if ty != "titleOnly" {
        if two {
            let cols = columns(f.body, 2, GUTTER * 2);
            let cols: Vec<R> = if rtl {
                cols.into_iter().map(|c| c.mirrored()).collect()
            } else {
                cols
            };
            shapes.push_str(&ph_body(
                3,
                "body",
                1,
                "Content Placeholder 2",
                cols[0],
                "<a:lstStyle/>",
                "t",
                "Click to edit Master text styles",
            ));
            shapes.push_str(&ph_body(
                4,
                "body",
                2,
                "Content Placeholder 3",
                cols[1],
                "<a:lstStyle/>",
                "t",
                "Click to edit Master text styles",
            ));
        } else {
            shapes.push_str(&ph_body(
                3,
                "body",
                1,
                "Content Placeholder 2",
                br,
                "<a:lstStyle/>",
                "t",
                "Click to edit Master text styles",
            ));
        }
    }
    shapes.push_str(&footer_phs(30, t, rtl));
    layout_shell(name, ty, true, None, false, &shapes)
}

fn quote_layout(t: &ThemeSpec, rtl: bool) -> String {
    // The quote keeps the header's voice: centred under a short rule, set big
    // under a heavy rule (poster), or flush with a vertical tab (the rest).
    let header = t.look.header;
    let centered = matches!(header, Header::Centered);
    let poster = matches!(header, Header::Poster);
    let mut deco = Vec::new();
    if centered {
        deco.push(Shape::deco(
            "Quote mark",
            Geom::Rect,
            R::new(SLIDE_W / 2 - 520_000, 1_250_000, 1_040_000, 44_000),
            Fill::Solid(Clr::accent(1)),
        ));
    } else if poster {
        deco.push(Shape::deco(
            "Quote mark",
            Geom::Rect,
            R::new(MARGIN_X, 5_150_000, CONTENT_W, 64_000),
            Fill::Solid(Clr::tx1()),
        ));
        deco.push(Shape::deco(
            "Quote accent",
            Geom::Rect,
            R::new(MARGIN_X, 5_150_000, 1_900_000, 64_000),
            Fill::Solid(Clr::accent(1)),
        ));
    } else {
        deco.push(Shape::deco(
            "Quote mark",
            Geom::Rect,
            R::new(MARGIN_X, 1_000_000, 240_000, 1_150_000),
            Fill::Solid(Clr::accent(1)),
        ));
    }
    if !poster {
        deco.push(orb("Glow", SLIDE_W, 0, 5_000_000, Clr::accent(1), 16));
    }
    let mut shapes = sp_list(&deco, 10, rtl);
    let (tr, ar) = if poster {
        (
            R::new(MARGIN_X, 900_000, CONTENT_W, 4_000_000),
            R::new(MARGIN_X, 5_400_000, CONTENT_W, 600_000),
        )
    } else {
        (
            R::new(1_300_000, 1_500_000, 9_600_000, 3_000_000),
            R::new(1_300_000, 4_800_000, 9_600_000, 800_000),
        )
    };
    let (tr, ar) = if rtl {
        (tr.mirrored(), ar.mirrored())
    } else {
        (tr, ar)
    };
    let algn = if centered {
        "ctr"
    } else if rtl {
        "r"
    } else {
        "l"
    };
    let (size, italic, bold) = if poster { (4000, 0, 1) } else { (3600, 1, 0) };
    let title_lst = format!(
        "<a:lstStyle><a:lvl1pPr algn=\"{algn}\" rtl=\"{}\"><a:lnSpc><a:spcPct val=\"100000\"/></a:lnSpc><a:buNone/><a:defRPr sz=\"{size}\" b=\"{bold}\" i=\"{italic}\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:latin typeface=\"+mj-lt\"/><a:cs typeface=\"+mj-cs\"/></a:defRPr></a:lvl1pPr></a:lstStyle>",
        if rtl { 1 } else { 0 }
    );
    let by_lst = format!(
        "<a:lstStyle><a:lvl1pPr marL=\"0\" indent=\"0\" algn=\"{algn}\" rtl=\"{}\"><a:buNone/><a:defRPr sz=\"1800\" b=\"1\"><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill></a:defRPr></a:lvl1pPr></a:lstStyle>",
        if rtl { 1 } else { 0 }
    );
    shapes.push_str(&ph_title(
        2,
        "title",
        "Title 1",
        tr,
        &title_lst,
        if poster { "b" } else { "ctr" },
        "Quote",
    ));
    shapes.push_str(&ph_body(
        3,
        "body",
        1,
        "Attribution",
        ar,
        &by_lst,
        "t",
        "Attribution",
    ));
    shapes.push_str(&footer_phs(30, t, rtl));
    // Headers whose furniture assumes the title lives in the header (band,
    // sidebar, rail) do not draw it behind a free-standing quote.
    let show_master = matches!(header, Header::TopLeft | Header::Centered);
    layout_shell("Quote", "obj", show_master, None, false, &shapes)
}

/// Layout XML for one part.
pub fn layout_xml(t: &ThemeSpec, part: LayoutPart, rtl: bool) -> String {
    match part {
        LayoutPart::Cover | LayoutPart::Closing => super::cover::hero_layout(t, part, rtl),
        LayoutPart::Section => super::cover::section_layout(t, rtl),
        LayoutPart::TitleContent => content_layout(t, "Title and Content", "obj", false, rtl),
        LayoutPart::TwoContent => content_layout(t, "Two Content", "twoObj", true, rtl),
        LayoutPart::TitleOnly => content_layout(t, "Title Only", "titleOnly", false, rtl),
        LayoutPart::Quote => quote_layout(t, rtl),
    }
}

#[cfg(test)]
mod tests {
    use super::super::theme::THEMES;
    use super::*;

    #[test]
    fn every_part_of_every_theme_is_well_formed_xml() {
        for t in THEMES {
            for rtl in [false, true] {
                roxmltree::Document::parse(&theme_xml(t))
                    .unwrap_or_else(|e| panic!("{} theme: {e}", t.id));
                roxmltree::Document::parse(&master_xml(t, rtl))
                    .unwrap_or_else(|e| panic!("{} master rtl={rtl}: {e}", t.id));
                for l in LAYOUTS {
                    roxmltree::Document::parse(&layout_xml(t, l, rtl))
                        .unwrap_or_else(|e| panic!("{} {l:?} rtl={rtl}: {e}", t.id));
                }
            }
        }
    }

    #[test]
    fn dark_themes_swap_the_colour_map_and_inverse_heroes_override_it() {
        let dark = super::super::theme::theme("aurora").unwrap();
        assert!(master_xml(dark, false).contains("bg1=\"dk1\" tx1=\"lt1\""));
        let light = super::super::theme::theme("harbor").unwrap();
        assert!(master_xml(light, false).contains("bg1=\"lt1\" tx1=\"dk1\""));
        assert!(layout_xml(light, LayoutPart::Cover, false).contains("overrideClrMapping"));
        assert!(!layout_xml(light, LayoutPart::TitleContent, false).contains("overrideClrMapping"));
        // Dark decks need no override: their map is already inverted.
        assert!(!layout_xml(dark, LayoutPart::Cover, false).contains("overrideClrMapping"));
    }

    #[test]
    fn layout_ids_are_distinct_and_match_the_parts() {
        let xml = master_xml(super::super::theme::default_theme(), false);
        for n in 1..=LAYOUTS.len() {
            assert!(xml.contains(&format!("r:id=\"rId{n}\"")), "layout {n}");
        }
        assert!(xml.contains("id=\"2147483649\"") && xml.contains("id=\"2147483655\""));
    }
}
