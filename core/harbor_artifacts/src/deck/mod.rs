//! The deck design engine: themes, layouts and motion for generated decks.
//!
//! Content (what to say) is a [`DesignedDeck`]; design (how it looks and
//! moves) is a [`Design`]. `to_pptx_bytes` composes them into a real
//! PowerPoint package — one master, seven layouts, a theme with the
//! palette and font scheme, slides on those layouts, speaker notes,
//! embedded charts, transitions and entrance animations — that opens in
//! PowerPoint, Keynote and Google Slides, previews in Harbor through the
//! same scene reader as any other deck, and is byte-for-byte reproducible
//! (no clock, no randomness), which is what lets a commit re-derive an
//! approved output hash.

pub mod cover;
pub mod grid;
pub mod header;
pub mod layouts;
pub mod look;
pub mod model;
pub mod motion;
pub mod ornament;
pub mod palette;
pub mod parts;
pub mod purpose;
pub mod source;
pub mod starter;
pub mod theme;
pub mod xml;

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;

pub use model::{infer_layout, split_stat, Block, Design, DesignedDeck, DesignedSlide, LayoutKind};
pub use motion::MotionPreset;
pub use theme::{theme as theme_by_id, ThemeSpec, THEMES};

use self::layouts::{Built, Ctx, El};
use self::motion::{ElKind, Role, Target};
use self::parts::{LayoutPart, LAYOUTS};
use self::xml::{esc, pic_xml, SLIDE_H, SLIDE_W};
use crate::pptx::{ChartKind, ChartSpec, PptxError, SlideContent};

const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
const GROUP: &str = r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#;

/// A slide serialised: its XML plus the parts it pulls in.
struct SlideOut {
    xml: String,
    layout: LayoutPart,
    notes: Option<String>,
    chart: Option<(ChartSpec, bool)>,
    image: Option<crate::pptx::SlideImage>,
}

fn chart_style(t: &ThemeSpec, spec: &ChartSpec) -> crate::chart_xml::ChartStyle {
    let mut st = crate::chart_xml::ChartStyle::default();
    if t.is_dark() {
        st.gridline_scheme = "tx1".into();
    }
    st.data_labels = spec.series.len() == 1 && spec.categories.len() <= 8;
    st.gap_width = if t.radius >= 10 { 60 } else { 90 };
    st
}

fn chart_frame(id: u32, name: &str, alt: &str, rect: xml::R, rtl: bool) -> String {
    let r = if rtl { rect.mirrored() } else { rect };
    format!(
        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"{}\" descr=\"{}\"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></p:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" r:id=\"rId3\"/></a:graphicData></a:graphic></p:graphicFrame>",
        esc(name),
        esc(alt),
        r.x,
        r.y,
        r.w,
        r.h
    )
}

fn paragraph_count(s: &xml::Shape) -> u32 {
    s.text.as_ref().map(|t| t.paras.len() as u32).unwrap_or(0)
}

/// Serialise one built slide: assign ids in paint order, plan its motion.
fn slide_out(
    built: &Built,
    kind: LayoutKind,
    index: usize,
    notes: Option<String>,
    design: &Design,
    theme: &ThemeSpec,
) -> SlideOut {
    let rtl = design.rtl;
    let mut body = String::new();
    let mut targets: Vec<Target> = Vec::new();
    let mut chart = None;
    let mut image = None;
    for (i, item) in built.items.iter().enumerate() {
        let id = 2 + i as u32;
        match &item.el {
            El::Sp(s) => {
                body.push_str(&s.xml(id, rtl));
                let k = if s.text.is_some() {
                    ElKind::Text
                } else {
                    ElKind::Plain
                };
                targets.push(Target {
                    id,
                    role: item.role,
                    kind: k,
                    paras: paragraph_count(s),
                });
            }
            El::Pic {
                name,
                alt,
                rect,
                geom,
                image: img,
            } => {
                body.push_str(&pic_xml(id, name, alt, "rId4", *rect, geom, rtl));
                image = Some(img.clone());
                targets.push(Target {
                    id,
                    role: item.role,
                    kind: ElKind::Picture,
                    paras: 0,
                });
            }
            El::Chart {
                name,
                alt,
                rect,
                spec,
            } => {
                body.push_str(&chart_frame(id, name, alt, *rect, rtl));
                chart = Some((spec.clone(), theme.is_dark()));
                targets.push(Target {
                    id,
                    role: item.role,
                    kind: ElKind::Chart,
                    paras: 0,
                });
            }
        }
    }
    // Only elements whose role the plan animates are passed on.
    let animatable: Vec<Target> = targets
        .iter()
        .copied()
        .filter(|t| !matches!(t.role, Role::Deco | Role::Footer))
        .collect();
    let plan = motion::plan(design.motion, kind, &animatable, design.progressive_bullets);
    let transition = motion::transition_xml(design.motion, kind, index, rtl);
    let timing = motion::timing_xml(&plan, &animatable);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sld {NS}><p:cSld><p:spTree>{GROUP}{body}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{transition}{timing}</p:sld>"
    );
    SlideOut {
        xml,
        layout: built.part,
        notes,
        chart,
        image,
    }
}

impl DesignedDeck {
    /// The theme this deck is designed with, in the palette chosen for it.
    pub fn theme(&self) -> Result<ThemeSpec, PptxError> {
        let base = theme::theme(&self.design.theme).ok_or_else(|| {
            PptxError::Malformed(format!("unknown theme {:?}", self.design.theme))
        })?;
        match self.design.palette.as_deref() {
            None => Ok(*base),
            Some(id) => {
                let p = palette::palette(id)
                    .ok_or_else(|| PptxError::Malformed(format!("unknown palette {id:?}")))?;
                Ok(base.with_palette(p))
            }
        }
    }

    /// The layout each slide gets (stated or inferred), in order.
    pub fn resolved_layouts(&self) -> Vec<LayoutKind> {
        let n = self.slides.len();
        self.slides
            .iter()
            .enumerate()
            .map(|(i, s)| s.resolved_layout(i, n))
            .collect()
    }

    /// Serialise to PPTX bytes. Deterministic.
    pub fn to_pptx_bytes(&self) -> Result<Vec<u8>, PptxError> {
        let theme_owned = self.theme()?;
        let theme = &theme_owned;
        let design = &self.design;
        let rtl = design.rtl;
        let kinds = self.resolved_layouts();
        let footer = design.footer.clone().unwrap_or_else(|| self.title.clone());

        let mut outs: Vec<SlideOut> = Vec::new();
        let mut section_no = 0usize;
        for (i, (s, kind)) in self.slides.iter().zip(kinds.iter()).enumerate() {
            if *kind == LayoutKind::Section {
                section_no += 1;
            }
            let mut s = s.clone();
            // A cover with no subtitle takes the deck's.
            if *kind == LayoutKind::Cover && s.bullets.is_empty() {
                if let Some(sub) = &self.subtitle {
                    s.bullets = vec![sub.clone()];
                }
            }
            let cx = Ctx {
                t: theme,
                frame: look::frame(&theme.look),
                rtl,
                footer: footer.clone(),
                numbers: design.slide_numbers,
                index: i,
                deck_title: &self.title,
                section_no,
            };
            let built = layouts::build(*kind, &s, &cx);
            outs.push(slide_out(&built, *kind, i, s.notes.clone(), design, theme));
        }

        let any_notes = outs.iter().any(|o| o.notes.is_some());
        let n = outs.len();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default();
        let mut put = |name: &str, body: &[u8]| -> Result<(), PptxError> {
            zip.start_file(name, opts)?;
            zip.write_all(body)?;
            Ok(())
        };

        // [Content_Types].xml
        let mut ct = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
        );
        ct.push_str("<Default Extension=\"json\" ContentType=\"application/json\"/>");
        if outs.iter().any(|o| o.chart.is_some()) {
            ct.push_str("<Default Extension=\"xlsx\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\"/>");
        }
        if outs
            .iter()
            .any(|o| o.image.as_ref().is_some_and(|i| i.extension == "png"))
        {
            ct.push_str("<Default Extension=\"png\" ContentType=\"image/png\"/>");
        }
        if outs
            .iter()
            .any(|o| o.image.as_ref().is_some_and(|i| i.extension == "jpg"))
        {
            ct.push_str("<Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>");
        }
        let ov = |part: &str, ty: &str| {
            format!("<Override PartName=\"{part}\" ContentType=\"application/vnd.openxmlformats-officedocument.{ty}\"/>")
        };
        ct.push_str(&ov(
            "/ppt/presentation.xml",
            "presentationml.presentation.main+xml",
        ));
        ct.push_str(&ov("/ppt/presProps.xml", "presentationml.presProps+xml"));
        ct.push_str(&ov("/ppt/viewProps.xml", "presentationml.viewProps+xml"));
        ct.push_str(&ov(
            "/ppt/tableStyles.xml",
            "presentationml.tableStyles+xml",
        ));
        ct.push_str(&ov(
            "/ppt/slideMasters/slideMaster1.xml",
            "presentationml.slideMaster+xml",
        ));
        for l in LAYOUTS {
            ct.push_str(&ov(
                &format!("/ppt/slideLayouts/slideLayout{}.xml", l as u32),
                "presentationml.slideLayout+xml",
            ));
        }
        ct.push_str(&ov("/ppt/theme/theme1.xml", "theme+xml"));
        if any_notes {
            ct.push_str(&ov(
                "/ppt/notesMasters/notesMaster1.xml",
                "presentationml.notesMaster+xml",
            ));
            ct.push_str(&ov("/ppt/theme/theme2.xml", "theme+xml"));
        }
        for (i, o) in outs.iter().enumerate() {
            let k = i + 1;
            ct.push_str(&ov(
                &format!("/ppt/slides/slide{k}.xml"),
                "presentationml.slide+xml",
            ));
            if o.notes.is_some() {
                ct.push_str(&ov(
                    &format!("/ppt/notesSlides/notesSlide{k}.xml"),
                    "presentationml.notesSlide+xml",
                ));
            }
            if o.chart.is_some() {
                ct.push_str(&ov(
                    &format!("/ppt/charts/chart{k}.xml"),
                    "drawingml.chart+xml",
                ));
            }
        }
        ct.push_str("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>");
        ct.push_str(&ov("/docProps/app.xml", "extended-properties+xml"));
        ct.push_str("</Types>");
        put("[Content_Types].xml", ct.as_bytes())?;

        put(
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/><Relationship Id="rId4" Type="http://schemas.harbor.dev/2026/relationships/deck-source" Target="harbor/deck.json"/></Relationships>"#,
        )?;
        // The deck's content and design, so its design can be changed later
        // without reverse-engineering slide XML.
        put(
            source::SOURCE_PART,
            serde_json::to_string(&source::to_json(self))
                .map_err(|e| PptxError::Malformed(e.to_string()))?
                .as_bytes(),
        )?;

        // presentation.xml: master rId1, slides rId2.., then theme, props.
        let theme_rid = n + 2;
        let notes_rid = n + 6;
        let sld_ids: String = (1..=n)
            .map(|i| format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 255 + i, i + 1))
            .collect();
        let notes_list = if any_notes {
            format!("<p:notesMasterIdLst><p:notesMasterId r:id=\"rId{notes_rid}\"/></p:notesMasterIdLst>")
        } else {
            String::new()
        };
        put(
            "ppt/presentation.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:presentation {NS} saveSubsetFonts=\"1\"{rtl_attr}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst>{notes_list}<p:sldIdLst>{sld_ids}</p:sldIdLst><p:sldSz cx=\"{SLIDE_W}\" cy=\"{SLIDE_H}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>",
                rtl_attr = if rtl { " rtl=\"1\"" } else { "" },
            )
            .as_bytes(),
        )?;
        let rel = |id: usize, kind: &str, target: &str| {
            format!("<Relationship Id=\"rId{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{target}\"/>")
        };
        let mut pr = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        pr.push_str(&rel(1, "slideMaster", "slideMasters/slideMaster1.xml"));
        for i in 1..=n {
            pr.push_str(&rel(i + 1, "slide", &format!("slides/slide{i}.xml")));
        }
        pr.push_str(&rel(theme_rid, "theme", "theme/theme1.xml"));
        pr.push_str(&rel(n + 3, "presProps", "presProps.xml"));
        pr.push_str(&rel(n + 4, "viewProps", "viewProps.xml"));
        pr.push_str(&rel(n + 5, "tableStyles", "tableStyles.xml"));
        if any_notes {
            pr.push_str(&rel(
                notes_rid,
                "notesMaster",
                "notesMasters/notesMaster1.xml",
            ));
        }
        pr.push_str("</Relationships>");
        put("ppt/_rels/presentation.xml.rels", pr.as_bytes())?;
        put("ppt/presProps.xml", crate::pptx::PRES_PROPS_XML.as_bytes())?;
        put("ppt/viewProps.xml", crate::pptx::VIEW_PROPS_XML.as_bytes())?;
        put(
            "ppt/tableStyles.xml",
            crate::pptx::TABLE_STYLES_XML.as_bytes(),
        )?;

        // Master, layouts, theme.
        put(
            "ppt/slideMasters/slideMaster1.xml",
            parts::master_xml(theme, rtl).as_bytes(),
        )?;
        let mut mr = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        for l in LAYOUTS {
            mr.push_str(&rel(
                l as usize,
                "slideLayout",
                &format!("../slideLayouts/slideLayout{}.xml", l as u32),
            ));
        }
        mr.push_str(&rel(LAYOUTS.len() + 1, "theme", "../theme/theme1.xml"));
        mr.push_str("</Relationships>");
        put(
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            mr.as_bytes(),
        )?;
        for l in LAYOUTS {
            put(
                &format!("ppt/slideLayouts/slideLayout{}.xml", l as u32),
                parts::layout_xml(theme, l, rtl).as_bytes(),
            )?;
            put(
                &format!("ppt/slideLayouts/_rels/slideLayout{}.xml.rels", l as u32),
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{}</Relationships>", rel(1, "slideMaster", "../slideMasters/slideMaster1.xml")).as_bytes(),
            )?;
        }
        put("ppt/theme/theme1.xml", parts::theme_xml(theme).as_bytes())?;

        // Slides.
        for (i, o) in outs.iter().enumerate() {
            let k = i + 1;
            put(&format!("ppt/slides/slide{k}.xml"), o.xml.as_bytes())?;
            let mut sr = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
            );
            sr.push_str(&rel(
                1,
                "slideLayout",
                &format!("../slideLayouts/slideLayout{}.xml", o.layout as u32),
            ));
            if o.notes.is_some() {
                sr.push_str(&rel(
                    2,
                    "notesSlide",
                    &format!("../notesSlides/notesSlide{k}.xml"),
                ));
            }
            if o.chart.is_some() {
                sr.push_str(&rel(3, "chart", &format!("../charts/chart{k}.xml")));
            }
            if let Some(img) = &o.image {
                sr.push_str(&rel(
                    4,
                    "image",
                    &format!("../media/slide{k}-image.{}", img.extension),
                ));
            }
            sr.push_str("</Relationships>");
            put(
                &format!("ppt/slides/_rels/slide{k}.xml.rels"),
                sr.as_bytes(),
            )?;
            if let Some(text) = &o.notes {
                let dummy = SlideContent {
                    title: String::new(),
                    bullets: vec![],
                    notes: Some(text.clone()),
                    chart: None,
                    image: None,
                };
                put(
                    &format!("ppt/notesSlides/notesSlide{k}.xml"),
                    crate::pptx::notes_xml(&dummy).as_bytes(),
                )?;
                put(
                    &format!("ppt/notesSlides/_rels/notesSlide{k}.xml.rels"),
                    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{}{}</Relationships>", rel(1, "notesMaster", "../notesMasters/notesMaster1.xml"), rel(2, "slide", &format!("../slides/slide{k}.xml"))).as_bytes(),
                )?;
            }
            if let Some((spec, _dark)) = &o.chart {
                let style = chart_style(theme, spec);
                let xml = crate::chart_xml::styled_chart_space_xml(
                    spec.kind,
                    &spec.title,
                    &spec.categories,
                    &spec.series,
                    &style,
                );
                put(&format!("ppt/charts/chart{k}.xml"), xml.as_bytes())?;
                put(
                    &format!("ppt/charts/_rels/chart{k}.xml.rels"),
                    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/package\" Target=\"../embeddings/chartdata{k}.xlsx\"/></Relationships>").as_bytes(),
                )?;
                put(
                    &format!("ppt/embeddings/chartdata{k}.xlsx"),
                    &crate::pptx::chart_workbook_xlsx(&spec.categories, &spec.series),
                )?;
            }
            if let Some(img) = &o.image {
                put(
                    &format!("ppt/media/slide{k}-image.{}", img.extension),
                    &img.bytes,
                )?;
            }
        }
        if any_notes {
            put(
                "ppt/notesMasters/notesMaster1.xml",
                crate::pptx::notes_master_xml().as_bytes(),
            )?;
            put(
                "ppt/notesMasters/_rels/notesMaster1.xml.rels",
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{}</Relationships>", rel(1, "theme", "../theme/theme2.xml")).as_bytes(),
            )?;
            put("ppt/theme/theme2.xml", parts::theme_xml(theme).as_bytes())?;
        }
        put(
            "docProps/core.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>Harbor</dc:creator><dc:language>{}</dc:language></cp:coreProperties>",
                esc(&self.title),
                if rtl { "ar-SA" } else { "en-US" }
            )
            .as_bytes(),
        )?;
        put(
            "docProps/app.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Harbor</Application><Slides>{n}</Slides></Properties>"
            )
            .as_bytes(),
        )?;
        let cur = zip.finish()?;
        Ok(cur.into_inner())
    }
}

/// What the app shows in a theme picker.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub tagline: &'static str,
    pub dark: bool,
    /// Six accent colours as `#RRGGBB` plus page and ink, for a swatch strip.
    pub swatches: Vec<String>,
    pub suits: Vec<&'static str>,
    pub major_font: &'static str,
    pub minor_font: &'static str,
}

/// What the app shows in a palette picker.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub mood: &'static str,
    pub dark: bool,
    /// Six accents, then page and ink, as `#RRGGBB`.
    pub swatches: Vec<String>,
}

pub fn palette_catalog() -> Vec<PaletteInfo> {
    palette::PALETTES
        .iter()
        .map(|p| {
            let (page, ink) = match p.mode {
                theme::Mode::Light => (p.palette.lt1, p.palette.dk1),
                theme::Mode::Dark => (p.palette.dk1, p.palette.lt1),
            };
            let mut sw: Vec<String> = p.palette.accents.iter().map(|a| format!("#{a}")).collect();
            sw.push(format!("#{page}"));
            sw.push(format!("#{ink}"));
            PaletteInfo {
                id: p.id,
                name: p.name,
                mood: p.mood,
                dark: p.mode == theme::Mode::Dark,
                swatches: sw,
            }
        })
        .collect()
}

pub fn theme_catalog() -> Vec<ThemeInfo> {
    THEMES
        .iter()
        .map(|t| {
            let (page, ink) = t.page_and_ink();
            let mut sw: Vec<String> = t.palette.accents.iter().map(|a| format!("#{a}")).collect();
            sw.push(format!("#{page}"));
            sw.push(format!("#{ink}"));
            ThemeInfo {
                id: t.id,
                name: t.name,
                tagline: t.tagline,
                dark: t.is_dark(),
                swatches: sw,
                suits: t.suits.to_vec(),
                major_font: t.fonts.major,
                minor_font: t.fonts.minor,
            }
        })
        .collect()
}

#[allow(dead_code)]
fn _kinds_in_use(_: ChartKind) {}
