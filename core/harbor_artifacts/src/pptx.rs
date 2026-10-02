//! PPTX generation and modification via raw OOXML (Open Packaging
//! Conventions). Supported GA scope (21_Office_Feature_Matrix.csv): text
//! boxes/shapes with titles and content, speaker notes, basic theme.
//! Animations/transitions are PRESERVE_ONLY (we never emit them).

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct SlideContent {
    /// Slide title placeholder text.
    pub title: String,
    /// Body bullet lines.
    pub bullets: Vec<String>,
    /// Speaker notes text.
    pub notes: Option<String>,
    /// Embedded basic chart (bar/column, line, pie or scatter) with cached
    /// values. Cached data lives in the chart XML itself; per matrix row 14
    /// this is the qualified basic-series scope.
    pub chart: Option<ChartSpec>,
    /// Inline picture rendered from Harbor IR (matrix row 17: images).
    pub image: Option<SlideImage>,
}

/// An inline image on a slide: raw encoded bytes (PNG or JPEG) placed in
/// ppt/media and referenced by a p:pic drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideImage {
    /// File name inside ppt/media (e.g. "chart-shot").
    pub name: String,
    /// Content-type extension without dot ("png" or "jpg").
    pub extension: String,
    pub bytes: Vec<u8>,
}

/// Basic chart kinds supported by the generator (matrix rows 14/19 scope:
/// bar/column, line, pie and scatter basic series).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
    Line,
    Pie,
    Scatter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub title: String,
    pub categories: Vec<String>,
    /// (series name, values) — values.len() == categories.len().
    pub series: Vec<(String, Vec<f64>)>,
}

#[derive(Debug, Clone, Default)]
pub struct PptxDeck {
    pub title: String,
    pub slides: Vec<SlideContent>,
}

#[derive(Debug, thiserror::Error)]
pub enum PptxError {
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("malformed pptx: {0}")]
    Malformed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub enum PptxOp {
    SlideTextSet {
        slide: usize,
        placeholder: Placeholder,
        text: String,
    },
    SlideAppend(SlideContent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    Title,
    Body,
}

impl Placeholder {}

/// Slide part names in PRESENTATION order (`p:sldIdLst` resolved through
/// `ppt/_rels/presentation.xml.rels`). Part names carry no order: after a
/// delete or reorder, `slide3.xml` may be the 2nd slide, or the 2nd slide
/// may be `slide7.xml`. Falls back to numeric part-name order only when
/// the presentation part is absent or lists no resolvable slide.
fn ordered_slide_parts<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Vec<String> {
    const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let mut read_part = |name: &str| -> Option<String> {
        let mut f = archive.by_name(name).ok()?;
        let mut s = String::new();
        f.read_to_string(&mut s).ok()?;
        Some(s)
    };
    let from_presentation = (|| {
        let pres = read_part("ppt/presentation.xml")?;
        let rels = read_part("ppt/_rels/presentation.xml.rels")?;
        let pres = roxmltree::Document::parse(&pres).ok()?;
        let rels = roxmltree::Document::parse(&rels).ok()?;
        let targets: std::collections::HashMap<&str, &str> = rels
            .descendants()
            .filter(|n| n.has_tag_name("Relationship"))
            .filter_map(|n| Some((n.attribute("Id")?, n.attribute("Target")?)))
            .collect();
        let parts: Vec<String> = pres
            .descendants()
            .filter(|n| n.has_tag_name("sldId"))
            .filter_map(|n| n.attribute((REL_NS, "id")))
            .filter_map(|rid| targets.get(rid))
            .map(|t| match t.strip_prefix('/') {
                Some(abs) => abs.to_string(),
                None => normalize_rel_path("ppt", t),
            })
            .collect();
        Some(parts)
    })()
    .unwrap_or_default();
    let present = |p: &String| archive.file_names().any(|n| n == p);
    if !from_presentation.is_empty() && from_presentation.iter().all(present) {
        return from_presentation;
    }
    let mut names: Vec<String> = archive
        .file_names()
        .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
        .map(|n| n.to_string())
        .collect();
    names.sort_by_key(|n| {
        n.trim_start_matches("ppt/slides/slide")
            .trim_end_matches(".xml")
            .parse::<usize>()
            .unwrap_or(0)
    });
    names
}

/// Edit slide text in an EXISTING pptx package, preserving every other
/// part byte-for-byte (the docx-apply pattern, for slide XML).
/// - `slide` is the 1-based position in presentation order.
/// - Title: the slide's title-placeholder text is replaced wholesale.
/// - Body: bullet lines replace the body placeholder's existing
///   paragraphs 1:1 (more lines than paragraphs is a typed refusal —
///   no silent truncation, no invented paragraphs).
/// Text that cannot be placed is a typed refusal too; an edit never
/// reports success while dropping the new text.
pub fn apply_slide_text_edit(
    bytes: &[u8],
    slide: usize,
    placeholder: Placeholder,
    text: &str,
) -> Result<Vec<u8>, PptxError> {
    if slide == 0 {
        return Err(PptxError::Malformed("slide index is 1-based".into()));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let part = ordered_slide_parts(&mut archive)
        .into_iter()
        .nth(slide - 1)
        .ok_or_else(|| PptxError::Malformed(format!("deck has no slide {slide}")))?;
    let mut xml = String::new();
    archive
        .by_name(&part)
        .map_err(|_| PptxError::Malformed(format!("missing {part}")))?
        .read_to_string(&mut xml)?;
    let edited = edit_slide_xml(&xml, placeholder, text)?;
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts: zip::write::SimpleFileOptions = zip::write::FileOptions::default();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        let name = f.name().to_string();
        out.start_file(name.clone(), opts)?;
        if name == part {
            out.write_all(edited.as_bytes())?;
        } else {
            std::io::copy(&mut f, &mut out)?;
        }
    }
    Ok(out.finish()?.into_inner())
}

/// Find the title/body placeholder shape in slide XML and rewrite its
/// run texts. Everything outside that shape is byte-identical. Only the
/// FIRST matching shape is edited.
fn edit_slide_xml(xml: &str, placeholder: Placeholder, text: &str) -> Result<String, PptxError> {
    // Split into shape blocks on <p:sp> boundaries (raw scan; slide XML
    // from any generator keeps p:sp un-nested).
    let want_title = matches!(placeholder, Placeholder::Title);
    let mut out = String::with_capacity(xml.len() + text.len());
    let mut i = 0usize;
    while let Some(sp_start) = xml[i..].find("<p:sp>").map(|p| i + p) {
        let body_open_end = sp_start + "<p:sp>".len();
        let sp_end_rel = xml[body_open_end..]
            .find("</p:sp>")
            .ok_or_else(|| PptxError::Malformed("unbalanced p:sp".into()))?;
        let sp_end = body_open_end + sp_end_rel + "</p:sp>".len();
        let shape = &xml[sp_start..sp_end];
        let is_target = if want_title {
            shape.contains(r#"type="title""#) || shape.contains(r#"type="ctrTitle""#)
        } else {
            // Body placeholder: explicit body type, or a ph with no type.
            shape.contains("<p:ph")
                && (shape.contains(r#"type="body""#) || !shape.contains(r#"type=""#))
        };
        if !is_target {
            out.push_str(&xml[i..sp_end]);
            i = sp_end;
            continue;
        }
        out.push_str(&xml[i..sp_start]);
        match placeholder {
            Placeholder::Title => out.push_str(&replace_first_run_text(shape, text)?),
            Placeholder::Body => {
                let lines: Vec<&str> = if text.trim().is_empty() {
                    vec![""]
                } else {
                    text.split('\n').collect()
                };
                out.push_str(&replace_paragraph_texts(shape, &lines)?);
            }
        }
        out.push_str(&xml[sp_end..]);
        return Ok(out);
    }
    // No shape matched: the edit cannot land, so it must not report Ok.
    Err(PptxError::Malformed(format!(
        "slide has no {} placeholder",
        if want_title { "title" } else { "body" }
    )))
}

/// Locate the next opening `<tag>` / `<tag attr…>` / `<tag/>` at or after
/// `from` (never a longer name such as `<a:pPr`). Returns the start of the
/// tag, the index just past its `>`, and whether it is self-closing.
fn find_open_tag(s: &str, from: usize, tag: &str) -> Option<(usize, usize, bool)> {
    let needle = format!("<{tag}");
    let mut at = from;
    while let Some(rel) = s[at..].find(&needle) {
        let start = at + rel;
        let after = start + needle.len();
        match s.as_bytes().get(after) {
            Some(b'>' | b' ' | b'/') => {
                let close = after + s[after..].find('>')?;
                return Some((start, close + 1, s.as_bytes()[close - 1] == b'/'));
            }
            _ => at = after,
        }
    }
    None
}

/// Replace run texts: the FIRST `<a:t>` gets `text`, every other `<a:t>`
/// in the block is emptied (the docx run pattern). A block with no `<a:t>`
/// at all (an empty paragraph) gets a new run in its first paragraph.
fn replace_first_run_text(block: &str, text: &str) -> Result<String, PptxError> {
    let escaped = xml_escape(text);
    let mut out = String::with_capacity(block.len() + escaped.len());
    let mut first_done = false;
    let mut at = 0usize;
    while let Some((start, open_end, self_closing)) = find_open_tag(block, at, "a:t") {
        out.push_str(&block[at..start]);
        let content = if first_done { "" } else { escaped.as_str() };
        first_done = true;
        let attrs_open = if self_closing {
            // `<a:t/>` or `<a:t xml:space="preserve"/>`: reopen it.
            block[start..open_end - 2].trim_end().to_string() + ">"
        } else {
            block[start..open_end].to_string()
        };
        out.push_str(&attrs_open);
        out.push_str(content);
        out.push_str("</a:t>");
        at = if self_closing {
            open_end
        } else {
            match block[open_end..].find("</a:t>") {
                Some(p) => open_end + p + "</a:t>".len(),
                None => return Err(PptxError::Malformed("unbalanced a:t".into())),
            }
        };
    }
    out.push_str(&block[at..]);
    if first_done || text.is_empty() {
        return Ok(out);
    }
    insert_run(&out, &escaped)
}

/// Add `<a:r><a:t>text</a:t></a:r>` to the first paragraph of `block`:
/// before its `<a:endParaRPr>` if it has one, else before `</a:p>`, else
/// by expanding a self-closing `<a:p/>`.
fn insert_run(block: &str, escaped: &str) -> Result<String, PptxError> {
    let run = format!("<a:r><a:t>{escaped}</a:t></a:r>");
    let Some((p_start, p_open_end, p_self_closing)) = find_open_tag(block, 0, "a:p") else {
        return Err(PptxError::Malformed(
            "placeholder has no paragraph to hold the text".into(),
        ));
    };
    if p_self_closing {
        let open = block[p_start..p_open_end - 2].trim_end();
        return Ok(format!(
            "{}{open}>{run}</a:p>{}",
            &block[..p_start],
            &block[p_open_end..]
        ));
    }
    let p_end = block[p_open_end..]
        .find("</a:p>")
        .map(|p| p_open_end + p)
        .ok_or_else(|| PptxError::Malformed("unbalanced a:p".into()))?;
    let insert_at = find_open_tag(&block[..p_end], p_open_end, "a:endParaRPr")
        .map(|(s, _, _)| s)
        .unwrap_or(p_end);
    Ok(format!(
        "{}{run}{}",
        &block[..insert_at],
        &block[insert_at..]
    ))
}

/// Replace the text of each `<a:p>` paragraph's first run with lines[i];
/// extra paragraphs are emptied. More lines than paragraphs refuses.
fn replace_paragraph_texts(block: &str, lines: &[&str]) -> Result<String, PptxError> {
    let mut out = String::with_capacity(block.len());
    let mut line_idx = 0usize;
    let mut at = 0usize;
    while let Some((p_start, p_open_end, self_closing)) = find_open_tag(block, at, "a:p") {
        let line = lines.get(line_idx).copied().unwrap_or("");
        line_idx += 1;
        out.push_str(&block[at..p_start]);
        if self_closing {
            let paragraph = &block[p_start..p_open_end];
            if line.is_empty() {
                out.push_str(paragraph);
            } else {
                out.push_str(&insert_run(paragraph, &xml_escape(line))?);
            }
            at = p_open_end;
            continue;
        }
        let p_end = block[p_open_end..]
            .find("</a:p>")
            .map(|p| p_open_end + p + "</a:p>".len())
            .ok_or_else(|| PptxError::Malformed("unbalanced a:p".into()))?;
        out.push_str(&replace_first_run_text(&block[p_start..p_end], line)?);
        at = p_end;
    }
    if lines.len() > line_idx {
        return Err(PptxError::Malformed(format!(
            "slide has {line_idx} bullet paragraphs; refusing {} lines",
            lines.len()
        )));
    }
    out.push_str(&block[at..]);
    Ok(out)
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// A cover slide: centred title with an optional subtitle line, on the
/// title layout. Written only when the caller asks for one
/// ([`DeckStyle::first_slide_is_cover`]); its text is read back exactly
/// like a content slide's (title + one "bullet" per subtitle line).
fn cover_slide_xml(slide: &SlideContent) -> String {
    // A cover without a subtitle leaves the shape out: an empty subtitle
    // placeholder shows "Click to add subtitle" when the deck is opened.
    let mut subtitle = String::new();
    if !slide.bullets.is_empty() {
        subtitle.push_str(
            r#"
      <p:sp>
        <p:nvSpPr><p:cNvPr id="3" name="Subtitle 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="subTitle" idx="1"/></p:nvPr></p:nvSpPr>
        <p:spPr/>
        <p:txBody><a:bodyPr><a:normAutofit/></a:bodyPr><a:lstStyle/>"#,
        );
        for line in &slide.bullets {
            subtitle.push_str(&format!(
                "<a:p><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>",
                xml_escape(line)
            ));
        }
        subtitle.push_str("</p:txBody>\n      </p:sp>");
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="ctrTitle"/></p:nvPr></p:nvSpPr>
        <p:spPr/>
        <p:txBody><a:bodyPr><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p></p:txBody>
      </p:sp>{}
    </p:spTree>
  </p:cSld>
  <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>
"#,
        xml_escape(&slide.title),
        subtitle,
    )
}

fn slide_xml(slide: &SlideContent) -> String {
    let mut body_paras = String::new();
    for b in &slide.bullets {
        body_paras.push_str(&format!(
            "<a:p><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>",
            xml_escape(b)
        ));
    }
    if slide.bullets.is_empty() {
        body_paras.push_str("<a:p><a:endParaRPr lang=\"en-US\"/></a:p>");
    }
    // A slide-level chart becomes a graphicFrame referencing the chart part.
    let chart_frame = slide
        .chart
        .as_ref()
        .map(|_| {
            r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="Chart 3"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="838200" y="3600000"/><a:ext cx="7200000" cy="3000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId3"/></a:graphicData></a:graphic></p:graphicFrame>"#.to_string()
        })
        .unwrap_or_default();
    // An inline picture becomes a p:pic shape referencing the media part.
    let image_frame = slide
        .image
        .as_ref()
        .map(|img| {
            let name = xml_escape(&img.name);
            format!(
                r#"<p:pic><p:nvPicPr><p:cNvPr id="5" name="{name}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="rId4"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="4572000" y="1143000"/><a:ext cx="3657600" cy="2743200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#
            )
        })
        .unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
        <p:spPr/>
        <p:txBody><a:bodyPr><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p></p:txBody>
      </p:sp>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="3" name="Content Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
        {body_xfrm}
        <p:txBody><a:bodyPr><a:normAutofit/></a:bodyPr><a:lstStyle/>{}</p:txBody>
      </p:sp>
      {chart_frame}
      {image_frame}
    </p:spTree>
  </p:cSld>
  <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>
"#,
        xml_escape(&slide.title),
        body_paras,
        // With a chart the body stops above it; otherwise the body keeps
        // the layout's placeholder frame.
        body_xfrm = if slide.chart.is_some() {
            r#"<p:spPr><a:xfrm><a:off x="838200" y="1690688"/><a:ext cx="10515600" cy="1800000"/></a:xfrm></p:spPr>"#
        } else {
            "<p:spPr/>"
        },
        chart_frame = chart_frame,
        image_frame = image_frame,
    )
}

/// One `<a:p>` per line, so multi-line notes stay multi-line.
fn paragraphs_xml(text: &str) -> String {
    let mut out = String::new();
    for line in text.split('\n') {
        if line.is_empty() {
            out.push_str("<a:p><a:endParaRPr lang=\"en-US\" dirty=\"0\"/></a:p>");
        } else {
            out.push_str(&format!(
                "<a:p><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>",
                xml_escape(line)
            ));
        }
    }
    out
}

fn notes_xml(slide: &SlideContent) -> String {
    let text = slide.notes.clone().unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr/>
    <p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp>
    <p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>
    <p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody></p:sp>
  </p:spTree></p:cSld>
  <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:notes>
"#,
        paragraphs_xml(&text)
    )
}

// ---------------------------------------------------------------------------
// Package skeleton. A deck PowerPoint, Keynote and LibreOffice open without
// a repair prompt needs more than slides: a master with text styles and
// placeholder frames, layouts the slides' placeholders inherit from, a
// notes master with its own theme whenever notes exist, and presentation
// properties. Slides are 16:9 (12192000 × 6858000 EMU).

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/presProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presProps+xml"/>
<Override PartName="/ppt/viewProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml"/>
<Override PartName="/ppt/tableStyles.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml"/>
<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout2.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>
</Types>"#;

const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;

const GROUP_PROPS: &str = r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#;

const CLR_MAP: &str = r#"<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>"#;

fn run_style(size: u32, font: &str) -> String {
    format!(
        r#"<a:defRPr sz="{size}" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+{font}-lt"/><a:ea typeface="+{font}-ea"/><a:cs typeface="+{font}-cs"/></a:defRPr>"#
    )
}

fn master_xml() -> String {
    let body_level = |lvl: u32, mar: u32, size: u32| {
        format!(
            r#"<a:lvl{lvl}pPr marL="{mar}" indent="-228600" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPts val="1000"/></a:spcBef><a:buFont typeface="Arial"/><a:buChar char="&#8226;"/>{}</a:lvl{lvl}pPr>"#,
            run_style(size, "mn")
        )
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster {NS}>
<p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>{GROUP_PROPS}
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="838200" y="365125"/><a:ext cx="10515600" cy="1325563"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0" anchor="ctr"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Title</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Text Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="838200" y="1825625"/><a:ext cx="10515600" cy="4351338"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:pPr lvl="0"/><a:r><a:rPr lang="en-US"/><a:t>Text</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld>
{CLR_MAP}
<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/><p:sldLayoutId id="2147483650" r:id="rId2"/></p:sldLayoutIdLst>
<p:txStyles>
<p:titleStyle><a:lvl1pPr algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPct val="0"/></a:spcBef><a:buNone/>{title}</a:lvl1pPr></p:titleStyle>
<p:bodyStyle>{l1}{l2}{l3}</p:bodyStyle>
<p:otherStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr><a:lvl1pPr marL="0" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1">{other}</a:lvl1pPr></p:otherStyle>
</p:txStyles>
</p:sldMaster>"#,
        title = run_style(3600, "mj"),
        l1 = body_level(1, 228600, 2400),
        l2 = body_level(2, 685800, 2000),
        l3 = body_level(3, 1143000, 1800),
        other = run_style(1800, "mn"),
    )
}

fn title_layout_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout {NS} type="title" preserve="1">
<p:cSld name="Title Slide"><p:spTree>{GROUP_PROPS}
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="ctrTitle"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="1524000" y="1122363"/><a:ext cx="9144000" cy="2387600"/></a:xfrm></p:spPr><p:txBody><a:bodyPr anchor="b"><a:normAutofit/></a:bodyPr><a:lstStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="4400"/></a:lvl1pPr></a:lstStyle><a:p><a:r><a:rPr lang="en-US"/><a:t>Title</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Subtitle 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="subTitle" idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="1524000" y="3602038"/><a:ext cx="9144000" cy="1655762"/></a:xfrm></p:spPr><p:txBody><a:bodyPr><a:normAutofit/></a:bodyPr><a:lstStyle><a:lvl1pPr marL="0" indent="0" algn="ctr"><a:buNone/><a:defRPr sz="2400"/></a:lvl1pPr></a:lstStyle><a:p><a:r><a:rPr lang="en-US"/><a:t>Subtitle</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#
    )
}

fn content_layout_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout {NS} type="obj" preserve="1">
<p:cSld name="Title and Content"><p:spTree>{GROUP_PROPS}
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Title</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Content Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr lvl="0"/><a:r><a:rPr lang="en-US"/><a:t>Text</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#
    )
}

fn notes_master_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notesMaster {NS}>
<p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>{GROUP_PROPS}
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg" idx="2"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="685800" y="1143000"/><a:ext cx="5486400" cy="3086100"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" sz="quarter" idx="3"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="685800" y="4400550"/><a:ext cx="5486400" cy="3600450"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0"/><a:lstStyle/><a:p><a:pPr lvl="0"/><a:r><a:rPr lang="en-US"/><a:t>Notes</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld>
{CLR_MAP}
<p:notesStyle><a:lvl1pPr marL="0" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1">{style}</a:lvl1pPr></p:notesStyle>
</p:notesMaster>"#,
        style = run_style(1200, "mn"),
    )
}

/// The theme's fonts are system fonts every Office install carries: a
/// generated deck opened on another machine must not silently substitute
/// its typeface and reflow every slide.
const THEME_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Harbor">
<a:themeElements>
<a:clrScheme name="Harbor"><a:dk1><a:srgbClr val="07111D"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F5FCC"/></a:dk2><a:lt2><a:srgbClr val="F3F6FA"/></a:lt2><a:accent1><a:srgbClr val="1F5FCC"/></a:accent1><a:accent2><a:srgbClr val="1E6F4E"/></a:accent2><a:accent3><a:srgbClr val="8A5200"/></a:accent3><a:accent4><a:srgbClr val="4F46C8"/></a:accent4><a:accent5><a:srgbClr val="A33126"/></a:accent5><a:accent6><a:srgbClr val="5B6B7A"/></a:accent6><a:hlink><a:srgbClr val="1F5FCC"/></a:hlink><a:folHlink><a:srgbClr val="4F46C8"/></a:folHlink></a:clrScheme>
<a:fontScheme name="Harbor"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>
<a:fmtScheme name="Harbor"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>
</a:themeElements>
<a:objectDefaults/><a:extraClrSchemeLst/>
</a:theme>"#;

const PRES_PROPS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentationPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"#;

const VIEW_PROPS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:viewPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:gridSpacing cx="76200" cy="76200"/></p:viewPr>"#;

const TABLE_STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:tblStyleLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" def="{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}"/>"#;

/// Package-level choices the slides do not carry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeckStyle {
    /// Render the first slide on the title layout: its title centred and
    /// its bullets as subtitle lines.
    pub first_slide_is_cover: bool,
}

fn rels_for_slide(
    n: usize,
    layout: usize,
    with_notes: bool,
    with_chart: bool,
    image_ext: Option<&str>,
) -> String {
    let mut rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout{layout}.xml"/>"#
    );
    if with_notes {
        rels.push_str(&format!(
            "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide\" Target=\"../notesSlides/notesSlide{n}.xml\"/>"
        ));
    }
    if with_chart {
        // This target was once a plain string literal, so every chart
        // slide pointed at a part literally named `chart{n}.xml`.
        rels.push_str(&format!(
            "<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/chart{n}.xml\"/>"
        ));
    }
    if let Some(ext) = image_ext {
        rels.push_str(&format!(
            "<Relationship Id=\"rId4\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/slide{n}-image.{ext}\"/>"
        ));
    }
    rels.push_str("</Relationships>");
    rels
}

impl PptxDeck {
    /// Serialize to PPTX package bytes (every slide on the content layout).
    pub fn to_pptx_bytes(&self) -> Result<Vec<u8>, PptxError> {
        self.to_pptx_bytes_with(DeckStyle::default())
    }

    /// Serialize with package-level choices. Deterministic: no clock, no
    /// random ids, fixed zip timestamps — the same deck always yields the
    /// same bytes, which is what lets a commit re-derive an approved
    /// output hash from its batch.
    pub fn to_pptx_bytes_with(&self, style: DeckStyle) -> Result<Vec<u8>, PptxError> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default();
        let is_cover = |i: usize| style.first_slide_is_cover && i == 0;
        let any_notes = self.slides.iter().any(|s| s.notes.is_some());
        // [Content_Types].xml with per-slide overrides.
        let mut ct = CONTENT_TYPES.trim_end_matches("</Types>").to_string();
        if any_notes {
            ct.push_str("<Override PartName=\"/ppt/notesMasters/notesMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml\"/>");
            ct.push_str("<Override PartName=\"/ppt/theme/theme2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>");
        }
        for i in 1..=self.slides.len() {
            ct.push_str(&format!(
                "<Override PartName=\"/ppt/slides/slide{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
            ));
            if self.slides[i - 1].notes.is_some() {
                ct.push_str(&format!(
                    "<Override PartName=\"/ppt/notesSlides/notesSlide{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>"
                ));
            }
            if self.slides[i - 1].chart.is_some() {
                ct.push_str(&format!(
                    "<Override PartName=\"/ppt/charts/chart{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>"
                ));
            }
        }
        if self.slides.iter().any(|s| s.chart.is_some()) {
            ct.push_str("<Default Extension=\"xlsx\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\"/>");
        }
        // Image media defaults (matrix row 17): PNG and JPEG.
        if self.slides.iter().any(|s| {
            s.image
                .as_ref()
                .map(|i| i.extension == "png")
                .unwrap_or(false)
        }) {
            ct.push_str("<Default Extension=\"png\" ContentType=\"image/png\"/>");
        }
        if self.slides.iter().any(|s| {
            s.image
                .as_ref()
                .map(|i| i.extension == "jpg")
                .unwrap_or(false)
        }) {
            ct.push_str("<Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>");
        }
        ct.push_str("</Types>");
        zip.start_file("[Content_Types].xml", opts)?;
        zip.write_all(ct.as_bytes())?;

        zip.start_file("_rels/.rels", opts)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>
</Relationships>"#,
        )?;

        // presentation.xml + rels. Relationship ids: rId1 master, rId2..
        // slides, then theme, presProps, viewProps, tableStyles and (when
        // any slide has notes) the notes master.
        let n_slides = self.slides.len();
        let mut sld_ids = String::new();
        for i in 1..=n_slides {
            sld_ids.push_str(&format!(
                "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
                255 + i,
                i + 1
            ));
        }
        let theme_rid = n_slides + 2;
        let notes_master_rid = n_slides + 6;
        let notes_master_list = if any_notes {
            format!(
                "<p:notesMasterIdLst><p:notesMasterId r:id=\"rId{notes_master_rid}\"/></p:notesMasterIdLst>"
            )
        } else {
            String::new()
        };
        let presentation = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation {NS} saveSubsetFonts="1">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>{notes_master_list}
<p:sldIdLst>{sld_ids}</p:sldIdLst>
<p:sldSz cx="12192000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#
        );
        zip.start_file("ppt/presentation.xml", opts)?;
        zip.write_all(presentation.as_bytes())?;
        zip.start_file("ppt/_rels/presentation.xml.rels", opts)?;
        let rel = |id: usize, kind: &str, target: &str| {
            format!("<Relationship Id=\"rId{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{target}\"/>")
        };
        let mut pres_rels = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        );
        pres_rels.push_str(&rel(1, "slideMaster", "slideMasters/slideMaster1.xml"));
        for i in 1..=n_slides {
            pres_rels.push_str(&rel(i + 1, "slide", &format!("slides/slide{i}.xml")));
        }
        pres_rels.push_str(&rel(theme_rid, "theme", "theme/theme1.xml"));
        pres_rels.push_str(&rel(n_slides + 3, "presProps", "presProps.xml"));
        pres_rels.push_str(&rel(n_slides + 4, "viewProps", "viewProps.xml"));
        pres_rels.push_str(&rel(n_slides + 5, "tableStyles", "tableStyles.xml"));
        if any_notes {
            pres_rels.push_str(&rel(
                notes_master_rid,
                "notesMaster",
                "notesMasters/notesMaster1.xml",
            ));
        }
        pres_rels.push_str("</Relationships>");
        zip.write_all(pres_rels.as_bytes())?;
        for (name, body) in [
            ("ppt/presProps.xml", PRES_PROPS_XML),
            ("ppt/viewProps.xml", VIEW_PROPS_XML),
            ("ppt/tableStyles.xml", TABLE_STYLES_XML),
        ] {
            zip.start_file(name, opts)?;
            zip.write_all(body.as_bytes())?;
        }

        zip.start_file("ppt/slideMasters/slideMaster1.xml", opts)?;
        zip.write_all(master_xml().as_bytes())?;
        zip.start_file("ppt/slideMasters/_rels/slideMaster1.xml.rels", opts)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout2.xml"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
</Relationships>"#,
        )?;
        for (n, xml) in [(1, title_layout_xml()), (2, content_layout_xml())] {
            zip.start_file(format!("ppt/slideLayouts/slideLayout{n}.xml"), opts)?;
            zip.write_all(xml.as_bytes())?;
            zip.start_file(
                format!("ppt/slideLayouts/_rels/slideLayout{n}.xml.rels"),
                opts,
            )?;
            zip.write_all(
                br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#,
            )?;
        }
        zip.start_file("ppt/theme/theme1.xml", opts)?;
        zip.write_all(THEME_XML.as_bytes())?;

        for (i, slide) in self.slides.iter().enumerate() {
            let n = i + 1;
            let cover = is_cover(i);
            zip.start_file(format!("ppt/slides/slide{n}.xml"), opts)?;
            if cover {
                zip.write_all(cover_slide_xml(slide).as_bytes())?;
            } else {
                zip.write_all(slide_xml(slide).as_bytes())?;
            }
            zip.start_file(format!("ppt/slides/_rels/slide{n}.xml.rels"), opts)?;
            zip.write_all(
                rels_for_slide(
                    n,
                    if cover { 1 } else { 2 },
                    slide.notes.is_some(),
                    slide.chart.is_some() && !cover,
                    if cover {
                        None
                    } else {
                        slide.image.as_ref().map(|im| im.extension.as_str())
                    },
                )
                .as_bytes(),
            )?;
            if slide.notes.is_some() {
                zip.start_file(format!("ppt/notesSlides/notesSlide{n}.xml"), opts)?;
                zip.write_all(notes_xml(slide).as_bytes())?;
                zip.start_file(
                    format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                    opts,
                )?;
                // Each notes page belongs to its own slide (this used to
                // point every notes page at slide 1).
                zip.write_all(
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster" Target="../notesMasters/notesMaster1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="../slides/slide{n}.xml"/>
</Relationships>"#
                    )
                    .as_bytes(),
                )?;
            }
            if cover {
                continue;
            }
            // Embedded chart: chart part + rels to the embedded workbook +
            // the workbook itself (minimal; cached values live in the XML).
            if let Some(spec) = &slide.chart {
                let chart_xml =
                    chart_space_xml(spec.kind, &spec.title, &spec.categories, &spec.series);
                zip.start_file(format!("ppt/charts/chart{n}.xml"), opts)?;
                zip.write_all(chart_xml.as_bytes())?;
                zip.start_file(format!("ppt/charts/_rels/chart{n}.xml.rels"), opts)?;
                zip.write_all(
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/chartdata{n}.xlsx"/></Relationships>"#
                    )
                    .as_bytes(),
                )?;
                let xlsx = minimal_embedded_xlsx(&spec.series);
                zip.start_file(format!("ppt/embeddings/chartdata{n}.xlsx"), opts)?;
                zip.write_all(&xlsx)?;
            }
            if let Some(img) = &slide.image {
                zip.start_file(format!("ppt/media/slide{n}-image.{}", img.extension), opts)?;
                zip.write_all(&img.bytes)?;
            }
        }
        if any_notes {
            zip.start_file("ppt/notesMasters/notesMaster1.xml", opts)?;
            zip.write_all(notes_master_xml().as_bytes())?;
            zip.start_file("ppt/notesMasters/_rels/notesMaster1.xml.rels", opts)?;
            zip.write_all(
                br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme2.xml"/>
</Relationships>"#,
            )?;
            zip.start_file("ppt/theme/theme2.xml", opts)?;
            zip.write_all(THEME_XML.as_bytes())?;
        }
        zip.start_file("docProps/core.xml", opts)?;
        zip.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>Harbor</dc:creator></cp:coreProperties>"#,
                xml_escape(&self.title)
            )
            .as_bytes(),
        )?;
        zip.start_file("docProps/app.xml", opts)?;
        zip.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Harbor</Application><Slides>{n_slides}</Slides></Properties>"#
            )
            .as_bytes(),
        )?;
        let cur = zip.finish()?;
        Ok(cur.into_inner())
    }

    /// Load an existing deck's slide texts (read-back preview) from bytes.
    /// Speaker notes are recovered through each slide's notesSlide
    /// relationship (matrix row 18 round-trip).
    pub fn from_pptx_bytes(bytes: &[u8]) -> Result<PptxDeck, PptxError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        crate::inflate_probe(&mut archive).map_err(PptxError::Malformed)?;
        // Minimal read-back: slide count + titles/bullets text extraction.
        let names = ordered_slide_parts(&mut archive);
        let mut slides = Vec::new();
        for name in &names {
            let xml = {
                let mut f = archive.by_name(name)?;
                let mut xml = String::new();
                f.read_to_string(&mut xml)?;
                xml
            };
            let mut slide = parse_slide_texts(&xml)?;
            slide.notes = read_notes_via_rels(&mut archive, name)?;
            slides.push(slide);
        }
        let title = read_title(&mut archive)?;
        Ok(PptxDeck { title, slides })
    }
}

/// Resolve `ppt/slides/_rels/slideN.xml.rels` to the notes slide part and
/// extract its text. None when the slide has no notes relationship.
fn read_notes_via_rels(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    slide_name: &str,
) -> Result<Option<String>, PptxError> {
    let rels_name = format!(
        "ppt/slides/_rels/{}.rels",
        slide_name.rsplit('/').next().unwrap_or(slide_name)
    );
    let target = match archive.by_name(&rels_name) {
        Ok(mut f) => {
            let mut rels = String::new();
            f.read_to_string(&mut rels)?;
            find_notes_slide_target(&rels)
        }
        Err(_) => None,
    };
    let Some(target) = target else {
        return Ok(None);
    };
    // Target is relative to ppt/slides/ (e.g. "../notesSlides/notesSlide1.xml").
    let part = normalize_rel_path("ppt/slides", &target);
    let Ok(mut f) = archive.by_name(&part) else {
        return Err(PptxError::Malformed(format!(
            "notes relationship target missing: {part}"
        )));
    };
    let mut xml = String::new();
    f.read_to_string(&mut xml)?;
    let doc = roxmltree::Document::parse(&xml).map_err(|e| PptxError::Malformed(e.to_string()))?;
    // One line per paragraph: multi-paragraph notes used to come back run
    // together with no separator at all.
    let lines: Vec<String> = doc
        .descendants()
        .filter(|n| n.has_tag_name("p"))
        .map(|p| {
            p.descendants()
                .filter(|n| n.has_tag_name("t"))
                .map(|t| t.text().unwrap_or_default())
                .collect::<String>()
        })
        .collect();
    let text = lines.join("\n").trim_end_matches('\n').to_string();
    Ok(if text.trim().is_empty() {
        None
    } else {
        Some(text)
    })
}

fn find_notes_slide_target(rels_xml: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(rels_xml).ok()?;
    for rel in doc.descendants().filter(|n| n.has_tag_name("Relationship")) {
        if rel.attribute("Type")
            == Some(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide",
            )
        {
            return rel.attribute("Target").map(|s| s.to_string());
        }
    }
    None
}

/// Resolve a relationship target relative to its source part directory.
fn normalize_rel_path(source_dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if source_dir.is_empty() {
        Vec::new()
    } else {
        source_dir.split('/').collect()
    };
    for seg in target.split('/') {
        match seg {
            "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn read_title(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<String, PptxError> {
    if let Ok(mut f) = archive.by_name("docProps/core.xml") {
        let mut xml = String::new();
        f.read_to_string(&mut xml)?;
        // Parsed, not sliced: a title with `&` used to come back as the
        // escaped `&amp;`.
        if let Ok(doc) = roxmltree::Document::parse(&xml) {
            if let Some(t) = doc
                .descendants()
                .find(|n| n.has_tag_name("title") && n.tag_name().namespace().is_some())
            {
                return Ok(t.text().unwrap_or_default().to_string());
            }
        }
    }
    Ok(String::new())
}

fn parse_slide_texts(xml: &str) -> Result<SlideContent, PptxError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| PptxError::Malformed(e.to_string()))?;
    let mut title = String::new();
    let mut bullets = Vec::new();
    for sp in doc.descendants().filter(|n| n.has_tag_name("sp")) {
        let is_title = sp.descendants().any(|n| {
            n.has_tag_name("ph") && matches!(n.attribute("type"), Some("title" | "ctrTitle"))
        });
        let texts: Vec<String> = sp
            .descendants()
            .filter(|n| n.has_tag_name("t"))
            .map(|n| n.text().unwrap_or_default().to_string())
            .collect();
        if texts.is_empty() {
            continue;
        }
        if is_title && title.is_empty() {
            title = texts.join("");
        } else {
            bullets.extend(texts);
        }
    }
    Ok(SlideContent {
        title,
        bullets,
        notes: None,
        chart: None,
        image: None,
    })
}

/// Build a c:chartSpace document for a basic bar/column, line, pie or
/// scatter chart with cached categories and values.
pub fn chart_space_xml(
    kind: ChartKind,
    title: &str,
    categories: &[String],
    series: &[(String, Vec<f64>)],
) -> String {
    let chart_el = match kind {
        ChartKind::Bar => "barChart",
        ChartKind::Line => "lineChart",
        ChartKind::Pie => "pieChart",
        ChartKind::Scatter => "scatterChart",
    };
    let mut sers = String::new();
    for (i, (name, values)) in series.iter().enumerate() {
        let mut cats = String::new();
        let mut vals = String::new();
        for (j, c) in categories.iter().enumerate() {
            cats.push_str(&format!(
                "<c:pt idx=\"{j}\"><c:v>{}</c:v></c:pt>",
                xml_escape(c)
            ));
        }
        for (j, v) in values.iter().enumerate() {
            vals.push_str(&format!("<c:pt idx=\"{j}\"><c:v>{v}</c:v></c:pt>"));
        }
        let series_name = format!(
            r#"<c:tx><c:strRef><c:f>Sheet1!$A$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>{}</c:v></c:pt></c:strCache></c:strRef></c:tx>"#,
            xml_escape(name)
        );
        let ser = match kind {
            // Category charts: c:cat (strings) + c:val (numbers).
            ChartKind::Bar | ChartKind::Line | ChartKind::Pie => format!(
                r#"<c:ser><c:idx val="{i}"/><c:order val="{i}"/>{series_name}<c:cat><c:strRef><c:f>Sheet1!$B$1:$B${n}</c:f><c:strCache><c:ptCount val="{n}"/>{cats}</c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$C$1:$C${n}</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="{n}"/>{vals}</c:numCache></c:numRef></c:val></c:ser>"#,
                n = categories.len(),
            ),
            // Scatter: numeric c:xVal (categories parsed as numbers) +
            // c:yVal; requires numeric categories by definition.
            ChartKind::Scatter => {
                let mut xvals = String::new();
                for (j, c) in categories.iter().enumerate() {
                    let x: f64 = c.trim().parse().unwrap_or(0.0);
                    xvals.push_str(&format!("<c:pt idx=\"{j}\"><c:v>{x}</c:v></c:pt>"));
                }
                format!(
                    r#"<c:ser><c:idx val="{i}"/><c:order val="{i}"/>{series_name}<c:spPr><a:ln w="28575"><a:solidFill><a:srgbClr val="1F5FCC"/></a:solidFill></a:ln></c:spPr><c:marker><c:symbol val="circle"/></c:marker><c:xVal><c:numRef><c:f>Sheet1!$B$1:$B${n}</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="{n}"/>{xvals}</c:numCache></c:numRef></c:xVal><c:yVal><c:numRef><c:f>Sheet1!$C$1:$C${n}</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="{n}"/>{vals}</c:numCache></c:numRef></c:yVal></c:ser>"#,
                    n = categories.len(),
                )
            }
        };
        sers.push_str(&ser);
    }
    let axes = match kind {
        ChartKind::Bar | ChartKind::Line | ChartKind::Scatter => {
            r#"<c:axId val="111111111"/><c:axId val="222222222"/>"#.to_string()
        }
        ChartKind::Pie => String::new(),
    };
    let axes_xml = match kind {
        ChartKind::Bar | ChartKind::Line => String::from(
            r#"<c:catAx><c:axId val="111111111"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="222222222"/></c:catAx><c:valAx><c:axId val="222222222"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="111111111"/></c:valAx>"#,
        ),
        // Scatter uses two value axes.
        ChartKind::Scatter => String::from(
            r#"<c:valAx><c:axId val="111111111"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="222222222"/></c:valAx><c:valAx><c:axId val="222222222"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="111111111"/></c:valAx>"#,
        ),
        ChartKind::Pie => String::new(),
    };
    let grouping = match kind {
        ChartKind::Bar => "<c:grouping val=\"clustered\"/>",
        ChartKind::Line | ChartKind::Scatter => "<c:grouping val=\"standard\"/>",
        ChartKind::Pie => "",
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>{title}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title><c:autoTitleDeleted val="0"/>
<c:plotArea><c:layout/>{chart_el}{grouping}{sers}{axes}{axes_xml}</c:plotArea>
<c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/>
</c:chart>
</c:chartSpace>"#,
        title = xml_escape(title),
        chart_el = chart_el,
        grouping = grouping,
        sers = sers,
        axes = axes,
        axes_xml = axes_xml,
    )
}

/// Minimal embedded workbook bytes for a chart (PowerPoint tolerates an
/// empty data sheet when cached values are present).
pub fn minimal_embedded_xlsx(series: &[(String, Vec<f64>)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("[Content_Types].xml", opts).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#).unwrap();
    zip.start_file("_rels/.rels", opts).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#).unwrap();
    zip.start_file("xl/workbook.xml", opts).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></sheets></workbook>"#).unwrap();
    zip.start_file("xl/_rels/workbook.xml.rels", opts).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#).unwrap();
    zip.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
    let _ = series;
    zip.write_all(br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#).unwrap();
    zip.finish().unwrap().into_inner()
}

#[cfg(test)]
mod tests {
    /// Rewrite one part of a pptx package (test helper).
    fn rewrite_part(bytes: &[u8], part: &str, f: impl Fn(&str) -> String) -> Vec<u8> {
        let mut ar = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts: zip::write::SimpleFileOptions = zip::write::FileOptions::default();
        for i in 0..ar.len() {
            let mut file = ar.by_index(i).unwrap();
            let name = file.name().to_string();
            out.start_file(name.clone(), opts).unwrap();
            if name == part {
                let mut xml = String::new();
                std::io::Read::read_to_string(&mut file, &mut xml).unwrap();
                std::io::Write::write_all(&mut out, f(&xml).as_bytes()).unwrap();
            } else {
                std::io::copy(&mut file, &mut out).unwrap();
            }
        }
        out.finish().unwrap().into_inner()
    }

    fn three_slide_deck() -> Vec<u8> {
        let slide = |t: &str| SlideContent {
            title: t.into(),
            bullets: vec!["b".into()],
            notes: None,
            chart: None,
            image: None,
        };
        PptxDeck {
            title: "Deck".into(),
            slides: vec![slide("First"), slide("Second"), slide("Third")],
        }
        .to_pptx_bytes()
        .unwrap()
    }

    #[test]
    fn slide_index_follows_presentation_order_not_part_names() {
        // Reorder: the deck now presents slide3, slide2, slide1.
        let reordered = rewrite_part(&three_slide_deck(), "ppt/presentation.xml", |xml| {
            let start = xml.find("<p:sldIdLst>").unwrap();
            let end = xml.find("</p:sldIdLst>").unwrap();
            let ids: Vec<&str> = xml[start + "<p:sldIdLst>".len()..end]
                .split("/>")
                .filter(|s| !s.is_empty())
                .collect();
            let reversed: String = ids.iter().rev().map(|s| format!("{s}/>")).collect();
            format!("{}<p:sldIdLst>{reversed}{}", &xml[..start], &xml[end..])
        });
        let deck = PptxDeck::from_pptx_bytes(&reordered).unwrap();
        assert_eq!(deck.slides[0].title, "Third", "reader follows sldIdLst");

        // Editing displayed slide 1 edits what the reader shows as slide 1.
        let edited =
            crate::pptx::apply_slide_text_edit(&reordered, 1, Placeholder::Title, "Edited")
                .unwrap();
        let after = PptxDeck::from_pptx_bytes(&edited).unwrap();
        assert_eq!(after.slides[0].title, "Edited");
        assert_eq!(after.slides[1].title, "Second");
        assert_eq!(after.slides[2].title, "First");

        // Past the end is a typed refusal, not a missing-part guess.
        assert!(
            crate::pptx::apply_slide_text_edit(&reordered, 4, Placeholder::Title, "x").is_err()
        );
    }

    #[test]
    fn edits_into_empty_runs_are_placed_or_refused_never_dropped() {
        // Blank bullet (no <a:t>), attributed <a:t>, and a self-closing <a:p/>.
        let bytes = rewrite_part(&three_slide_deck(), "ppt/slides/slide1.xml", |xml| {
            let body_start = xml.find("Content Placeholder").unwrap();
            let (head, body) = xml.split_at(body_start);
            let p_start = body.find("<a:p>").unwrap();
            let p_end = body.find("</a:p>").unwrap() + "</a:p>".len();
            let paras = "<a:p><a:endParaRPr lang=\"en-US\"/></a:p>\
                         <a:p><a:r><a:t xml:space=\"preserve\">old</a:t></a:r></a:p>\
                         <a:p/>";
            format!("{head}{}{paras}{}", &body[..p_start], &body[p_end..])
        });
        let out = crate::pptx::apply_slide_text_edit(
            &bytes,
            1,
            Placeholder::Body,
            "first\nsecond & more\nthird",
        )
        .unwrap();
        let deck = PptxDeck::from_pptx_bytes(&out).unwrap();
        assert_eq!(
            deck.slides[0].bullets,
            vec!["first", "second & more", "third"]
        );
        // Another slide is untouched.
        assert_eq!(deck.slides[1].title, "Second");
        // A fourth line has no paragraph to land in: refused.
        assert!(
            crate::pptx::apply_slide_text_edit(&bytes, 1, Placeholder::Body, "a\nb\nc\nd").is_err()
        );
    }

    #[test]
    fn slide_text_edit_preserves_package_and_applies() {
        let deck = PptxDeck {
            title: "Deck".into(),
            slides: vec![SlideContent {
                title: "Old title".into(),
                bullets: vec!["one".into(), "two".into()],
                notes: None,
                chart: None,
                image: None,
            }],
        };
        let bytes = deck.to_pptx_bytes().unwrap();

        // Title edit.
        let out =
            crate::pptx::apply_slide_text_edit(&bytes, 1, Placeholder::Title, "New title").unwrap();
        let re = PptxDeck::from_pptx_bytes(&out).unwrap();
        assert!(re.slides[0].title.contains("New title"));
        assert!(
            re.slides[0].bullets.contains(&"one".to_string()),
            "bullets untouched by a title edit"
        );

        // Body edit (2 paragraphs, 2 lines).
        let out2 =
            crate::pptx::apply_slide_text_edit(&out, 1, Placeholder::Body, "alpha\nbeta").unwrap();
        let re2 = PptxDeck::from_pptx_bytes(&out2).unwrap();
        assert!(re2.slides[0].bullets.contains(&"alpha".to_string()));
        assert!(re2.slides[0].bullets.contains(&"beta".to_string()));

        // More lines than paragraphs is a typed refusal.
        assert!(
            crate::pptx::apply_slide_text_edit(&out2, 1, Placeholder::Body, "a\nb\nc").is_err()
        );

        // Byte preservation: every other zip entry is identical.
        let mut before = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut after = zip::ZipArchive::new(std::io::Cursor::new(&out2)).unwrap();
        use std::io::Read as _;
        for i in 0..before.len() {
            let (mut a, mut b) = (Vec::new(), Vec::new());
            let mut fa = before.by_index(i).unwrap();
            let name = fa.name().to_string();
            fa.read_to_end(&mut a).unwrap();
            let mut fb = after.by_name(&name).unwrap();
            fb.read_to_end(&mut b).unwrap();
            if name == "ppt/slides/slide1.xml" {
                assert_ne!(a, b, "the edited slide must differ");
            } else {
                assert_eq!(a, b, "{name} must be byte-preserved");
            }
        }
    }

    use super::*;

    #[test]
    fn generate_and_read_back_deck() {
        let deck = PptxDeck {
            title: "Q3 Board Review".into(),
            slides: vec![
                SlideContent {
                    title: "Revenue".into(),
                    bullets: vec!["Revenue grew 12% QoQ".into(), "EMEA leads growth".into()],
                    notes: Some("Source: verified workbook recalc".into()),
                    chart: None,
                    image: None,
                },
                SlideContent {
                    title: "Outlook".into(),
                    bullets: vec!["Pipeline strong".into()],
                    notes: None,
                    chart: None,
                    image: None,
                },
            ],
        };
        let bytes = deck.to_pptx_bytes().unwrap();
        assert!(bytes.len() > 2000);
        // It is a valid zip with OOXML content types.
        let mut ar = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        assert!(ar.by_name("[Content_Types].xml").is_ok());
        assert!(ar.by_name("ppt/slides/slide1.xml").is_ok());
        assert!(ar.by_name("ppt/slides/slide2.xml").is_ok());
        let back = PptxDeck::from_pptx_bytes(&bytes).unwrap();
        assert_eq!(back.title, "Q3 Board Review");
        assert_eq!(back.slides.len(), 2);
        assert_eq!(back.slides[0].title, "Revenue");
        assert!(back.slides[0]
            .bullets
            .contains(&"Revenue grew 12% QoQ".to_string()));
    }

    fn slide(title: &str, bullets: &[&str], notes: Option<&str>) -> SlideContent {
        SlideContent {
            title: title.into(),
            bullets: bullets.iter().map(|b| b.to_string()).collect(),
            notes: notes.map(str::to_string),
            chart: None,
            image: None,
        }
    }

    fn part(bytes: &[u8], name: &str) -> String {
        let mut ar = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut s = String::new();
        ar.by_name(name).unwrap().read_to_string(&mut s).unwrap();
        s
    }

    #[test]
    fn generated_packages_are_sound_and_deterministic() {
        let mut chart_slide = slide("Revenue", &["Grew in Q2"], Some("Source: CRM export"));
        chart_slide.chart = Some(ChartSpec {
            kind: ChartKind::Bar,
            title: "Revenue".into(),
            categories: vec!["Q1".into(), "Q2".into()],
            series: vec![("Revenue".into(), vec![1.0, 2.0])],
        });
        let deck = PptxDeck {
            title: "Plan & review".into(),
            slides: vec![
                slide("Plan & review", &["September 2026"], None),
                slide(
                    "Where we are",
                    &["Two launches", "One slip"],
                    Some("line one\nline two"),
                ),
                chart_slide,
            ],
        };
        let style = DeckStyle {
            first_slide_is_cover: true,
        };
        let a = deck.to_pptx_bytes_with(style).unwrap();
        let b = deck.to_pptx_bytes_with(style).unwrap();
        assert_eq!(a, b, "no clock and no random ids in the package");
        let problems = crate::package_integrity(&a);
        assert!(problems.is_empty(), "{problems:?}");
        // Without notes there is no notes master to reference.
        let plain = PptxDeck {
            title: "t".into(),
            slides: vec![slide("One", &["a"], None)],
        }
        .to_pptx_bytes()
        .unwrap();
        assert!(crate::package_integrity(&plain).is_empty());
        assert!(!part(&plain, "ppt/presentation.xml").contains("notesMasterIdLst"));
        // Each notes page belongs to its own slide; the chart link names
        // the real part (it used to be the literal `chart{n}.xml`).
        assert!(
            part(&a, "ppt/notesSlides/_rels/notesSlide2.xml.rels").contains("../slides/slide2.xml")
        );
        assert!(part(&a, "ppt/slides/_rels/slide3.xml.rels").contains("../charts/chart3.xml"));
        assert!(part(&a, "ppt/slides/_rels/slide1.xml.rels").contains("slideLayout1.xml"));
        assert!(part(&a, "ppt/slides/_rels/slide2.xml.rels").contains("slideLayout2.xml"));
        // Read-back: the cover title is a title, notes keep their lines.
        let back = PptxDeck::from_pptx_bytes(&a).unwrap();
        assert_eq!(back.title, "Plan & review");
        assert_eq!(back.slides[0].title, "Plan & review");
        assert_eq!(back.slides[0].bullets, vec!["September 2026".to_string()]);
        assert_eq!(back.slides[1].notes.as_deref(), Some("line one\nline two"));
        assert_eq!(back.slides[0].notes, None);
        // A cover without a subtitle has no subtitle shape, so PowerPoint
        // does not show an empty "Click to add subtitle" box on it.
        assert!(part(&a, "ppt/slides/slide1.xml").contains(r#"type="subTitle""#));
        let bare = PptxDeck {
            title: "Plan".into(),
            slides: vec![slide("Plan", &[], None), slide("One", &["a"], None)],
        }
        .to_pptx_bytes_with(style)
        .unwrap();
        assert!(crate::package_integrity(&bare).is_empty());
        assert!(!part(&bare, "ppt/slides/slide1.xml").contains("subTitle"));
        let back = PptxDeck::from_pptx_bytes(&bare).unwrap();
        assert_eq!(back.slides[0].title, "Plan");
        assert!(back.slides[0].bullets.is_empty());
    }

    #[test]
    fn xml_escaping() {
        assert_eq!(xml_escape("a<b>&\"c\""), "a&lt;b&gt;&amp;&quot;c&quot;");
    }
}
