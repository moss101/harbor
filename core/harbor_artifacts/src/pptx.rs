//! PPTX generation and modification via raw OOXML (Open Packaging
//! Conventions). Supported GA scope (21_Office_Feature_Matrix.csv): text
//! boxes/shapes with titles and content, speaker notes, basic theme.
//! Animations/transitions are PRESERVE_ONLY (we never emit them).

use std::collections::BTreeMap;
use std::io::{Cursor, Write, Read};

use zip::write::SimpleFileOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct SlideContent {
    /// Slide title placeholder text.
    pub title: String,
    /// Body bullet lines.
    pub bullets: Vec<String>,
    /// Speaker notes text.
    pub notes: Option<String>,
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
    SlideTextSet { slide: usize, placeholder: Placeholder, text: String },
    SlideAppend(SlideContent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    Title,
    Body,
}

impl Placeholder {
    fn idx(self) -> u32 {
        match self {
            Placeholder::Title => 0,
            Placeholder::Body => 1,
        }
    }
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

fn slide_xml(slide: &SlideContent) -> String {
    let mut body_paras = String::new();
    for b in &slide.bullets {
        body_paras.push_str(&format!(
            "<a:p><a:pPr><a:buChar char=\"\u{2022}\"/></a:pPr><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>",
            xml_escape(b)
        ));
    }
    if slide.bullets.is_empty() {
        body_paras.push_str("<a:p><a:endParaRPr lang=\"en-US\"/></a:p>");
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
        <p:spPr><a:xfrm><a:off x="838200" y="365125"/><a:ext cx="7416800" cy="1111250"/></a:xfrm></p:spPr>
        <p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p></p:txBody>
      </p:sp>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="3" name="Content Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>
        <p:spPr><a:xfrm><a:off x="838200" y="1593850"/><a:ext cx="7416800" cy="4171925"/></a:xfrm></p:spPr>
        <p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
  <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>
"#,
        xml_escape(&slide.title),
        body_paras
    )
}

fn notes_xml(slide: &SlideContent) -> String {
    let text = slide.notes.clone().unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr/>
    <p:sp><p:nvSpPr><p:cNvPr id="2" name="Notes Placeholder"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>
    <p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p></p:txBody></p:sp>
  </p:spTree></p:cSld>
</p:notes>
"#,
        xml_escape(&text)
    )
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>
</Types>"#;

const MASTER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="9144000" cy="6858000"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
</p:spTree></p:cSld>
<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst>
</p:sldMaster>"#;

const LAYOUT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="obj" preserve="1">
<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="9144000" cy="6858000"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#;

const THEME_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Harbor">
<a:themeElements>
<a:clrScheme name="Harbor"><a:dk1><a:srgbClr val="07111D"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F5FCC"/></a:dk2><a:lt2><a:srgbClr val="F3F6FA"/></a:lt2><a:accent1><a:srgbClr val="1F5FCC"/></a:accent1><a:accent2><a:srgbClr val="1E6F4E"/></a:accent2><a:accent3><a:srgbClr val="8A5200"/></a:accent3><a:accent4><a:srgbClr val="4F46C8"/></a:accent4><a:accent5><a:srgbClr val="A33126"/></a:accent5><a:accent6><a:srgbClr val="5B6B7A"/></a:accent6><a:hlink><a:srgbClr val="1F5FCC"/></a:hlink><a:folHlink><a:srgbClr val="4F46C8"/></a:folHlink></a:clrScheme>
<a:fontScheme name="Harbor"><a:majorFont><a:latin typeface="Inter"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Inter"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>
<a:fmtScheme name="Harbor"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>
</a:themeElements>
</a:theme>"#;

fn rels_for_slide(n: usize, with_notes: bool) -> String {
    let mut r = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide{n}.xml"/>
</Relationships>"#,
        n = n
    );
    if !with_notes {
        r = r.replace(
            &format!(
                "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide\" Target=\"../notesSlides/notesSlide{n}.xml\"/>",
                n = n
            ),
            "",
        );
    }
    r
}

impl PptxDeck {
    /// Serialize to PPTX package bytes.
    pub fn to_pptx_bytes(&self) -> Result<Vec<u8>, PptxError> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default();
        // [Content_Types].xml with per-slide overrides.
        let mut ct = CONTENT_TYPES.trim_end_matches("</Types>").to_string();
        for i in 1..=self.slides.len() {
            ct.push_str(&format!(
                "<Override PartName=\"/ppt/slides/slide{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
            ));
            ct.push_str(&format!(
                "<Override PartName=\"/ppt/notesSlides/notesSlide{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>"
            ));
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

        // presentation.xml + rels
        let mut sld_ids = String::new();
        for i in 1..=self.slides.len() {
            sld_ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 255 + i, i + 1));
        }
        let presentation = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
<p:sldIdLst>{sld_ids}</p:sldIdLst>
<p:sldSz cx="9144000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#
        );
        zip.start_file("ppt/presentation.xml", opts)?;
        zip.write_all(presentation.as_bytes())?;
        zip.start_file("ppt/_rels/presentation.xml.rels", opts)?;
        let mut pres_rels = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#,
        );
        for i in 1..=self.slides.len() {
            pres_rels.push_str(&format!(
                "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{i}.xml\"/>",
                i + 1,
                i = i
            ));
        }
        pres_rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme1.xml\"/></Relationships>",
            self.slides.len() + 2
        ));
        zip.write_all(pres_rels.as_bytes())?;

        zip.start_file("ppt/slideMasters/slideMaster1.xml", opts)?;
        zip.write_all(MASTER_XML.as_bytes())?;
        zip.start_file("ppt/slideMasters/_rels/slideMaster1.xml.rels", opts)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
</Relationships>"#,
        )?;
        zip.start_file("ppt/slideLayouts/slideLayout1.xml", opts)?;
        zip.write_all(LAYOUT_XML.as_bytes())?;
        zip.start_file("ppt/slideLayouts/_rels/slideLayout1.xml.rels", opts)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#,
        )?;
        zip.start_file("ppt/theme/theme1.xml", opts)?;
        zip.write_all(THEME_XML.as_bytes())?;

        for (i, slide) in self.slides.iter().enumerate() {
            let n = i + 1;
            zip.start_file(format!("ppt/slides/slide{n}.xml"), opts)?;
            zip.write_all(slide_xml(slide).as_bytes())?;
            zip.start_file(format!("ppt/slides/_rels/slide{n}.xml.rels"), opts)?;
            zip.write_all(rels_for_slide(n, slide.notes.is_some()).as_bytes())?;
            zip.start_file(format!("ppt/notesSlides/notesSlide{n}.xml"), opts)?;
            zip.write_all(notes_xml(slide).as_bytes())?;
            zip.start_file(format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"), opts)?;
            zip.write_all(
                br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster" Target="../notesMasters/notesMaster1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="../slides/slide1.xml"/>
</Relationships>"#,
            )?;
        }
        zip.start_file("ppt/notesMasters/notesMaster1.xml", opts)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notesMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
</p:notesMaster>"#,
        )?;
        zip.start_file("docProps/core.xml", opts)?;
        zip.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title></cp:coreProperties>"#,
                xml_escape(&self.title)
            )
            .as_bytes(),
        )?;
        zip.start_file("docProps/app.xml", opts)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Harbor</Application></Properties>"#,
        )?;
        let cur = zip.finish()?;
        Ok(cur.into_inner())
    }

    /// Load an existing deck's slide texts (read-back preview) from bytes.
    pub fn from_pptx_bytes(bytes: &[u8]) -> Result<PptxDeck, PptxError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        // Minimal read-back: slide count + titles/bullets text extraction.
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
        let mut slides = Vec::new();
        for name in &names {
            let mut f = archive.by_name(name)?;
            let mut xml = String::new();
            f.read_to_string(&mut xml)?;
            slides.push(parse_slide_texts(&xml)?);
        }
        let title = read_title(&mut archive)?;
        Ok(PptxDeck { title, slides })
    }
}

fn read_title(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<String, PptxError> {
    if let Ok(mut f) = archive.by_name("docProps/core.xml") {
        let mut xml = String::new();
        f.read_to_string(&mut xml)?;
        if let Some(start) = xml.find("<dc:title>") {
            if let Some(end) = xml[start..].find("</dc:title>") {
                return Ok(xml[start + 10..start + end].to_string());
            }
        }
    }
    Ok(String::new())
}

fn parse_slide_texts(xml: &str) -> Result<SlideContent, PptxError> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| PptxError::Malformed(e.to_string()))?;
    let mut title = String::new();
    let mut bullets = Vec::new();
    for sp in doc.descendants().filter(|n| n.has_tag_name("sp")) {
        let is_title = sp
            .descendants()
            .any(|n| n.has_tag_name("ph") && n.attribute("type") == Some("title"));
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
    Ok(SlideContent { title, bullets, notes: None })
}

#[cfg(test)]
mod tests {
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
                },
                SlideContent {
                    title: "Outlook".into(),
                    bullets: vec!["Pipeline strong".into()],
                    notes: None,
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
        assert!(back.slides[0].bullets.contains(&"Revenue grew 12% QoQ".to_string()));
    }

    #[test]
    fn xml_escaping() {
        assert_eq!(xml_escape("a<b>&\"c\""), "a&lt;b&gt;&amp;&quot;c&quot;");
    }
}
