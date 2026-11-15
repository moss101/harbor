use super::*;
use harbor_artifacts::{DeckStyle, PptxDeck, SlideContent};

fn legacy_deck() -> Vec<u8> {
    PptxDeck {
        title: "Q3".into(),
        slides: vec![
            SlideContent {
                title: "Harbor Q3 review".into(),
                bullets: vec!["Prepared for the board".into()],
                notes: None,
                chart: None,
                image: None,
            },
            SlideContent {
                title: "Revenue grew 24%".into(),
                bullets: vec!["EMEA led growth".into(), "Churn fell to 2.1%".into()],
                notes: Some("say it slowly".into()),
                chart: None,
                image: None,
            },
        ],
    }
    .to_pptx_bytes_with(DeckStyle {
        first_slide_is_cover: true,
    })
    .unwrap()
}

fn shapes(slide: &SlideScene) -> Vec<&ShapeItem> {
    slide
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Shape(s) => Some(s),
            _ => None,
        })
        .collect()
}

fn first_run(s: &ShapeItem) -> &Run {
    &s.text.as_ref().unwrap().paras[0].runs[0]
}

#[test]
fn legacy_deck_resolves_size_geometry_styles_and_notes() {
    let scene = deck_scene(&legacy_deck()).unwrap();
    assert_eq!((scene.width, scene.height), (12192000.0, 6858000.0));
    assert_eq!(scene.slides.len(), 2);

    // Cover: title placeholder geometry comes from the LAYOUT, its 44 pt
    // size and centring from the layout's list style.
    let cover = &scene.slides[0];
    let title = shapes(cover)
        .into_iter()
        .find(|s| s.text.as_ref().unwrap().paras[0].runs[0].t == "Harbor Q3 review")
        .expect("cover title");
    assert_eq!(
        (title.frame.x, title.frame.y, title.frame.w, title.frame.h),
        (1524000.0, 1122363.0, 9144000.0, 2387600.0)
    );
    assert_eq!(first_run(title).sz, 44.0);
    assert_eq!(title.text.as_ref().unwrap().paras[0].algn, "ctr");
    assert_eq!(title.text.as_ref().unwrap().anchor, "b");

    // Content slide: geometry from the MASTER, 36 pt title; 24 pt bullets
    // with the master's bullet and margin; theme dark colour for text.
    let slide = &scene.slides[1];
    let st = shapes(slide);
    let t = st
        .iter()
        .find(|s| first_run(s).t == "Revenue grew 24%")
        .unwrap();
    assert_eq!((t.frame.x, t.frame.y), (838200.0, 365125.0));
    assert_eq!(first_run(t).sz, 36.0);
    assert_eq!(first_run(t).c, "#07111D");
    assert_eq!(first_run(t).font, "Calibri");
    let body = st
        .iter()
        .find(|s| first_run(s).t == "EMEA led growth")
        .unwrap();
    let p = &body.text.as_ref().unwrap().paras[0];
    assert_eq!(first_run(body).sz, 24.0);
    assert_eq!(p.bullet.as_ref().unwrap().ch, "\u{2022}");
    assert_eq!(p.mar_l, 228600.0);
    assert_eq!(p.indent, -228600.0);
    assert_eq!(body.text.as_ref().unwrap().paras.len(), 2);

    assert_eq!(slide.notes.as_deref(), Some("say it slowly"));
    assert_eq!(
        slide.background,
        Paint::Solid {
            c: "#FFFFFF".into(),
            a: 1.0
        },
        "bgRef 1001 + bg1 resolves through the theme"
    );
    assert!(slide.transition.is_none());
    assert!(slide.animations.is_empty());
    assert!(slide.unsupported.is_empty(), "{:?}", slide.unsupported);
    // The scene is plain JSON.
    let json = serde_json::to_string(&scene).unwrap();
    let back: DeckScene = serde_json::from_str(&json).unwrap();
    assert_eq!(back, scene);
}

/// Build a one-slide package by hand to exercise what the legacy writer
/// never emits: groups, rotation, flips, gradients, a dark colour-map
/// override, shadows, custom geometry, connectors, tables, transitions.
fn handmade(slide_body: &str, layout_extra: &str, slide_extra: &str) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default();
    let mut put = |name: &str, body: &str| {
        zip.start_file(name, o).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    };
    let rels = |items: &[(&str, &str, &str)]| {
        let mut s = String::from(
            r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        );
        for (id, ty, target) in items {
            s.push_str(&format!(r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{ty}" Target="{target}"/>"#));
        }
        s.push_str("</Relationships>");
        s
    };
    put(
        "[Content_Types].xml",
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#,
    );
    put(
        "ppt/presentation.xml",
        r#"<?xml version="1.0"?><p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="9144000" cy="5143500"/></p:presentation>"#,
    );
    put(
        "ppt/_rels/presentation.xml.rels",
        &rels(&[
            ("rId1", "slideMaster", "slideMasters/slideMaster1.xml"),
            ("rId2", "slide", "slides/slide1.xml"),
        ]),
    );
    put(
        "ppt/slides/_rels/slide1.xml.rels",
        &rels(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")]),
    );
    put(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        &rels(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")]),
    );
    put(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        &rels(&[
            ("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"),
            ("rId2", "theme", "../theme/theme1.xml"),
        ]),
    );
    put(
        "ppt/theme/theme1.xml",
        r#"<?xml version="1.0"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="t"><a:dk1><a:srgbClr val="101820"/></a:dk1><a:lt1><a:srgbClr val="FAFAFA"/></a:lt1><a:dk2><a:srgbClr val="223344"/></a:dk2><a:lt2><a:srgbClr val="EEEEEE"/></a:lt2><a:accent1><a:srgbClr val="0066CC"/></a:accent1><a:accent2><a:srgbClr val="CC6600"/></a:accent2></a:clrScheme><a:fontScheme name="f"><a:majorFont><a:latin typeface="Georgia"/></a:majorFont><a:minorFont><a:latin typeface="Arial"/></a:minorFont></a:fontScheme><a:fmtScheme name="s"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#,
    );
    put(
        "ppt/slideMasters/slideMaster1.xml",
        r#"<?xml version="1.0"?><p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree/></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent1" accent4="accent1" accent5="accent1" accent6="accent1" hlink="accent1" folHlink="accent1"/><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#,
    );
    put(
        "ppt/slideLayouts/slideLayout1.xml",
        &format!(
            r#"<?xml version="1.0"?><p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree>{layout_extra}</p:spTree></p:cSld>{}</p:sldLayout>"#,
            ""
        ),
    );
    put(
        "ppt/slides/slide1.xml",
        &format!(
            r#"<?xml version="1.0"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{slide_body}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{slide_extra}</p:sld>"#
        ),
    );
    zip.finish().unwrap().into_inner()
}

const XFRM: &str =
    r#"<a:xfrm><a:off x="1000000" y="500000"/><a:ext cx="2000000" cy="1000000"/></a:xfrm>"#;

#[test]
fn gradients_rotation_flips_shadows_and_style_refs() {
    let body = format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="4" name="Card" descr="a card"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm rot="2700000" flipH="1"><a:off x="1000000" y="500000"/><a:ext cx="2000000" cy="1000000"/></a:xfrm><a:prstGeom prst="roundRect"><a:avLst><a:gd name="adj" fmla="val 25000"/></a:avLst></a:prstGeom><a:gradFill><a:gsLst><a:gs pos="0"><a:schemeClr val="accent1"/></a:gs><a:gs pos="100000"><a:schemeClr val="accent2"><a:alpha val="50000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000"/></a:gradFill><a:ln w="19050"><a:solidFill><a:schemeClr val="bg1"/></a:solidFill><a:prstDash val="dash"/></a:ln><a:effectLst><a:outerShdw blurRad="76200" dist="25400" dir="5400000"><a:srgbClr val="000000"><a:alpha val="30000"/></a:srgbClr></a:outerShdw></a:effectLst></p:spPr></p:sp>
        <p:sp><p:nvSpPr><p:cNvPr id="5" name="Styled"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{XFRM}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:style><a:lnRef idx="1"><a:schemeClr val="accent2"/></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></p:style><p:txBody><a:bodyPr anchor="ctr"/><a:lstStyle/><a:p><a:r><a:rPr sz="2000"/><a:t>On accent</a:t></a:r></a:p></p:txBody></p:sp>"#
    );
    let scene = deck_scene(&handmade(&body, "", "")).unwrap();
    assert_eq!((scene.width, scene.height), (9144000.0, 5143500.0));
    let s = &scene.slides[0];
    let card = shapes(s)[0];
    assert_eq!(card.frame.id, 4);
    assert_eq!(card.frame.alt.as_deref(), Some("a card"));
    assert_eq!(card.frame.rot, 45.0);
    assert!(card.frame.flip_h && !card.frame.flip_v);
    assert_eq!(
        card.geom,
        Geom::Preset {
            name: "roundRect".into(),
            adj: vec![25000.0]
        }
    );
    match &card.fill {
        Paint::Linear { angle, stops } => {
            assert_eq!(*angle, 90.0);
            assert_eq!(stops[0].c, "#0066CC");
            assert_eq!(stops[1].c, "#CC6600");
            assert!((stops[1].a - 0.5).abs() < 1e-9);
        }
        other => panic!("{other:?}"),
    }
    let line = card.line.as_ref().unwrap();
    assert_eq!((line.w, line.dash.as_str()), (19050.0, "dash"));
    assert_eq!(
        line.paint,
        Paint::Solid {
            c: "#FAFAFA".into(),
            a: 1.0
        }
    );
    let sh = card.shadow.as_ref().unwrap();
    assert_eq!(sh.blur, 76200.0);
    assert!((sh.dy - 25400.0).abs() < 1e-6 && sh.dx.abs() < 1e-6);

    // Style refs: fill from fillRef idx 1 with phClr = accent1; line from
    // lnRef with accent2; text colour from fontRef (lt1).
    let styled = shapes(s)[1];
    assert_eq!(
        styled.fill,
        Paint::Solid {
            c: "#0066CC".into(),
            a: 1.0
        }
    );
    assert_eq!(
        styled.line.as_ref().unwrap().paint,
        Paint::Solid {
            c: "#CC6600".into(),
            a: 1.0
        }
    );
    assert_eq!(first_run(styled).c, "#FAFAFA");
    assert_eq!(first_run(styled).font, "Arial", "minor font from the theme");
    assert_eq!(styled.text.as_ref().unwrap().anchor, "ctr");
}

#[test]
fn groups_scale_and_rotate_children_and_nest() {
    let body = r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="2" name="G"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="2000000" cy="1000000"/><a:chOff x="0" y="0"/><a:chExt cx="1000000" cy="500000"/></a:xfrm></p:grpSpPr>
        <p:sp><p:nvSpPr><p:cNvPr id="3" name="in"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="100000" y="100000"/><a:ext cx="200000" cy="100000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></p:spPr></p:sp></p:grpSp>"#;
    let scene = deck_scene(&handmade(body, "", "")).unwrap();
    let r = shapes(&scene.slides[0])[0];
    // 2× scale and a (1000000,1000000) offset.
    assert_eq!(
        (r.frame.x, r.frame.y, r.frame.w, r.frame.h),
        (1200000.0, 1200000.0, 400000.0, 200000.0)
    );

    let rotated = r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="2" name="G"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm rot="5400000"><a:off x="0" y="0"/><a:ext cx="1000000" cy="1000000"/><a:chOff x="0" y="0"/><a:chExt cx="1000000" cy="1000000"/></a:xfrm></p:grpSpPr>
        <p:sp><p:nvSpPr><p:cNvPr id="3" name="in"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="200000" cy="200000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></p:spPr></p:sp></p:grpSp>"#;
    let scene = deck_scene(&handmade(rotated, "", "")).unwrap();
    let r = shapes(&scene.slides[0])[0];
    // A 90° turn about the group centre moves the top-left square to the
    // top-right, and the child inherits the rotation.
    assert_eq!(r.frame.rot, 90.0);
    assert!(
        (r.frame.x - 800000.0).abs() < 1.0 && r.frame.y.abs() < 1.0,
        "{:?}",
        r.frame
    );
}

#[test]
fn dark_layout_override_flips_text_and_background_colours() {
    // A layout that maps bg1→dk1 / tx1→lt1 (the "inverse" layout dark decks
    // use): slides under it inherit the override.
    use std::io::Write;
    let layout_extra = r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Band"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="9144000" cy="100000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="tx1"/></a:solidFill></p:spPr></p:sp>"#;
    let body = format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="4" name="T"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr>{XFRM}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Hello</a:t></a:r></a:p></p:txBody></p:sp>"#
    );
    // Patch the layout in the package to carry the override.
    let bytes = handmade(&body, layout_extra, "");
    let mut zin = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut zout = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..zin.len() {
        let mut f = zin.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "ppt/slideLayouts/slideLayout1.xml" {
            let s = String::from_utf8(data).unwrap().replace(
                "</p:sldLayout>",
                r#"<p:clrMapOvr><a:overrideClrMapping bg1="dk1" tx1="lt1" bg2="dk2" tx2="lt2" accent1="accent1" accent2="accent2" accent3="accent1" accent4="accent1" accent5="accent1" accent6="accent1" hlink="accent1" folHlink="accent1"/></p:clrMapOvr></p:sldLayout>"#,
            );
            data = s.into_bytes();
        }
        zout.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zout.write_all(&data).unwrap();
    }
    let dark = zout.finish().unwrap().into_inner();
    let scene = deck_scene(&dark).unwrap();
    let s = &scene.slides[0];
    assert_eq!(
        s.background,
        Paint::Solid {
            c: "#101820".into(),
            a: 1.0
        },
        "bg1 → dk1"
    );
    let band = shapes(s)[0];
    assert_eq!(
        band.frame.id, 0,
        "layout shapes are not addressable by animations"
    );
    assert_eq!(
        band.fill,
        Paint::Solid {
            c: "#FAFAFA".into(),
            a: 1.0
        },
        "tx1 → lt1"
    );
    assert_eq!(
        first_run(shapes(s)[1]).c,
        "#FAFAFA",
        "slide text follows the layout's map"
    );
}

#[test]
fn custom_geometry_connectors_tables_and_unsupported_objects() {
    let body = format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="4" name="Tri"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{XFRM}<a:custGeom><a:pathLst><a:path w="100" h="100"><a:moveTo><a:pt x="0" y="100"/></a:moveTo><a:lnTo><a:pt x="50" y="0"/></a:lnTo><a:lnTo><a:pt x="100" y="100"/></a:lnTo><a:close/></a:path></a:pathLst></a:custGeom><a:solidFill><a:srgbClr val="00AA00"/></a:solidFill></p:spPr></p:sp>
        <p:cxnSp><p:nvCxnSpPr><p:cNvPr id="5" name="Line"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr><p:spPr><a:xfrm><a:off x="0" y="2000000"/><a:ext cx="3000000" cy="0"/></a:xfrm><a:prstGeom prst="line"><a:avLst/></a:prstGeom><a:ln w="28575"><a:solidFill><a:srgbClr val="333333"/></a:solidFill><a:tailEnd type="triangle"/></a:ln></p:spPr></p:cxnSp>
        <p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="6" name="Tbl"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="500000" y="2500000"/><a:ext cx="4000000" cy="800000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1" bandRow="1"/><a:tblGrid><a:gridCol w="2000000"/><a:gridCol w="2000000"/></a:tblGrid>
        <a:tr h="400000"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Head</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc><a:tc gridSpan="1"><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>H2</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc></a:tr>
        <a:tr h="400000"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>a</a:t></a:r></a:p></a:txBody><a:tcPr><a:solidFill><a:srgbClr val="DDEEFF"/></a:solidFill></a:tcPr></a:tc><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>b</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>
        <p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="7" name="Diagram"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="1" cy="1"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:graphic></p:graphicFrame>"#
    );
    let scene = deck_scene(&handmade(&body, "", "")).unwrap();
    let s = &scene.slides[0];
    match &shapes(s)[0].geom {
        Geom::Path { w, h, cmds } => {
            assert_eq!((*w, *h), (100.0, 100.0));
            assert_eq!(cmds.len(), 4);
            assert_eq!(cmds[3], PathCmd::Z);
        }
        other => panic!("{other:?}"),
    }
    let line = shapes(s)[1];
    assert_eq!(
        line.geom,
        Geom::Preset {
            name: "line".into(),
            adj: vec![]
        }
    );
    assert_eq!(
        line.line.as_ref().unwrap().tail.as_deref(),
        Some("triangle")
    );
    let table = s
        .items
        .iter()
        .find_map(|i| match i {
            Item::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(table.cols, vec![2000000.0, 2000000.0]);
    assert_eq!(table.rows.len(), 2);
    // Header cell: accent fill (default style), white bold text.
    let head = &table.rows[0].cells[0];
    assert_eq!(
        head.fill,
        Paint::Solid {
            c: "#0066CC".into(),
            a: 1.0
        }
    );
    let run = &head.text.paras[0].runs[0];
    assert!(run.b);
    assert_eq!(run.c, "#FFFFFF");
    // Explicit cell fill wins.
    assert_eq!(
        table.rows[1].cells[0].fill,
        Paint::Solid {
            c: "#DDEEFF".into(),
            a: 1.0
        }
    );
    assert!(
        s.unsupported.iter().any(|u| u.contains("diagram")),
        "SmartArt is named, not dropped: {:?}",
        s.unsupported
    );
}

#[test]
fn transitions_and_animations_ride_along() {
    let extra = r#"<mc:AlternateContent><mc:Choice Requires="p14"><p:transition spd="med" p14:dur="800"><p14:ripple/></p:transition></mc:Choice><mc:Fallback><p:transition spd="med"><p:fade/></p:transition></mc:Fallback></mc:AlternateContent>"#;
    let scene = deck_scene(&handmade("", "", extra)).unwrap();
    let t = scene.slides[0].transition.as_ref().unwrap();
    assert_eq!((t.kind.as_str(), t.dur_ms), ("ripple", 800));
}

#[test]
fn hostile_packages_are_refused_not_panicked_on() {
    assert!(deck_scene(b"").is_err());
    assert!(deck_scene(b"PK\x03\x04garbage").is_err());
    assert!(deck_scene(b"%PDF-1.4").is_err());
    let good = legacy_deck();
    // Truncated archive.
    assert!(deck_scene(&good[..good.len() / 2]).is_err());
    // A package whose slide is malformed degrades to a noted slide.
    let bytes = handmade("<p:sp>", "", "");
    let scene = deck_scene(&bytes);
    if let Ok(s) = scene {
        assert!(!s.slides[0].unsupported.is_empty());
    }
    // A slide list with no slides.
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        "ppt/presentation.xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"<p:presentation xmlns:p=\"p\"/>").unwrap();
    let empty = zip.finish().unwrap().into_inner();
    assert!(deck_scene(&empty).is_err());
}

#[test]
fn chart_slides_resolve_to_chart_items() {
    use harbor_artifacts::{ChartKind, ChartSpec};
    let bytes = PptxDeck {
        title: "C".into(),
        slides: vec![SlideContent {
            title: "Chart".into(),
            bullets: vec![],
            notes: None,
            chart: Some(ChartSpec {
                kind: ChartKind::Bar,
                title: "Revenue".into(),
                categories: vec!["Q1".into(), "Q2".into()],
                series: vec![("2026".into(), vec![10.0, 14.0])],
            }),
            image: None,
        }],
    }
    .to_pptx_bytes()
    .unwrap();
    let scene = deck_scene(&bytes).unwrap();
    let slide = &scene.slides[0];
    let chart = slide
        .items
        .iter()
        .find_map(|i| match i {
            Item::Chart(c) => Some(c),
            _ => None,
        })
        .expect("a chart item");
    assert_eq!(chart.chart.kind, "column");
    assert_eq!(chart.chart.categories, vec!["Q1", "Q2"]);
    assert_eq!(chart.chart.series[0].values, vec![10.0, 14.0]);
    assert!(slide.unsupported.is_empty(), "{:?}", slide.unsupported);
}

#[test]
fn absurdly_nested_xml_is_refused_not_a_stack_overflow() {
    let deep = format!("{}{}", "<a:x>".repeat(5000), "</a:x>".repeat(5000));
    let body = format!("<p:sp>{deep}</p:sp>");
    // The slide part exceeds the depth budget: the deck is refused or the
    // slide is noted; either way the process lives.
    if let Ok(s) = deck_scene(&handmade(&body, "", "")) {
        assert!(!s.slides[0].unsupported.is_empty());
    }
}
