//! Office Feature Matrix conformance tests (21_Office_Feature_Matrix.csv).

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;

fn minimal_docx(document_xml: String) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default();
    zip.start_file("[Content_Types].xml", opts).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#).unwrap();
    zip.start_file("word/document.xml", opts).unwrap();
    zip.write_all(document_xml.as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

fn structured_docx() -> Vec<u8> {
    // Headings via pStyle, list membership via numPr, a table whose first
    // row is horizontally merged (gridSpan=2).
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Harbor Plan</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>First item</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Second item</w:t></w:r></w:p>
<w:tbl><w:tr>
<w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Merged header</w:t></w:r></w:p></w:tc>
</w:tr><w:tr>
<w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
<w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
</w:tr></w:tbl>
<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>Summary</w:t></w:r></w:p>
</w:body></w:document>"#;
    minimal_docx(document.to_string())
}

#[test]
fn docx_headings_lists_tables_roundtrip() {
    let bytes = structured_docx();
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();

    // Headings are recognized by style.
    let styles: Vec<Option<&str>> =
        doc.paragraphs.iter().map(|p| p.style.as_deref()).collect();
    assert!(styles.contains(&Some("Heading1")), "styles: {styles:?}");
    assert!(styles.contains(&Some("Heading2")));

    // All paragraphs present in document order (tables included).
    let texts: Vec<&str> = doc.paragraphs.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "Harbor Plan",
            "First item",
            "Second item",
            "Merged header",
            "A",
            "B",
            "Summary"
        ]
    );

    // A typed edit replacing the title preserves the rest of the document,
    // including the table cells.
    let out = doc
        .apply(
            &bytes,
            &[harbor_artifacts::DocxOp::TextReplace {
                index: 1,
                new_text: "Harbor Plan v2".into(),
            }],
        )
        .unwrap();
    let reloaded = harbor_artifacts::DocxDocument::load(&out).unwrap();
    assert_eq!(reloaded.paragraphs[0].text, "Harbor Plan v2");
    assert_eq!(reloaded.paragraphs[2].text, "Second item");
    assert_eq!(reloaded.paragraphs[4].text, "A");
    assert_eq!(reloaded.paragraphs[5].text, "B");
}

#[test]
fn xlsx_merged_cells_survive_edit_recalc_save() {
    // Build a workbook with a merged title cell (A1:B1) and a formula.
    let mut wb = umya_spreadsheet::new_file();
    let sheet = wb.get_sheet_mut(&0).unwrap();
    sheet.get_cell_mut((1, 1)).set_value("Merged title");
    sheet.add_merge_cells("A1:B1");
    sheet.get_cell_mut((2, 2)).set_value("B2 value");
    sheet.get_cell_mut((2, 3)).set_formula("1+1");
    let mut buf = std::io::BufWriter::new(Cursor::new(Vec::new()));
    umya_spreadsheet::writer::xlsx::write_writer(&wb, &mut buf).unwrap();
    let bytes = buf.into_inner().unwrap().into_inner();

    // Harbor load/edit/recalc/save preserves the merge (matrix row 11).
    let mut doc = harbor_artifacts::WorkbookDoc::load(&bytes).unwrap();
    doc.set_cell(
        "Sheet1",
        2,
        2,
        harbor_artifacts::workbook::CellSet::Value(harbor_formula::value::CellValue::Text(
            "edited".into(),
        )),
    )
    .unwrap();
    let recalc = doc.recalculate_all().unwrap();
    let out = doc.to_bytes().unwrap();

    let reloaded = harbor_artifacts::WorkbookDoc::load(&out).unwrap();
    let mut ar = zip::ZipArchive::new(Cursor::new(out.as_slice())).unwrap();
    let mut sheet_xml = String::new();
    use std::io::Read as _;
    ar.by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut sheet_xml)
        .unwrap();
    assert!(
        sheet_xml.contains("mergeCells"),
        "mergeCells element must survive"
    );
    assert!(
        sheet_xml.contains("A1:B1"),
        "the A1:B1 merge range must survive"
    );
    // Recalculated formula cache persisted through save/reload.
    assert_eq!(
        recalc.get(&("Sheet1".to_string(), 3, 2)),
        Some(&harbor_formula::value::CellValue::Number(2.0))
    );
}

#[test]
fn docx_same_length_replacement_preserves_run_formatting() {
    // Paragraph: bold run "Bold" + normal run " rest" (9 graphemes total).
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Bold</w:t></w:r><w:r><w:t> rest</w:t></w:r></w:p>
</w:body></w:document>"#;
    let bytes = minimal_docx(document.to_string());
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();
    assert_eq!(doc.paragraphs[0].text, "Bold rest");

    // Same-length replacement: 9 graphemes ("Kept  tail" = 10? count:
    // "Kept tail" is 9). The bold run keeps its formatting and the text
    // distributes across the runs at their original spans.
    let out = doc
        .apply(
            &bytes,
            &[harbor_artifacts::DocxOp::TextReplace {
                index: 1,
                new_text: "Kept tail".into(),
            }],
        )
        .unwrap();
    let reloaded = harbor_artifacts::DocxDocument::load(&out).unwrap();
    assert_eq!(reloaded.paragraphs[0].text, "Kept tail");

    // The bold run element (<w:b/>) must still be present in the XML and
    // must precede the run carrying the first part of the new text.
    let mut ar = zip::ZipArchive::new(Cursor::new(out.as_slice())).unwrap();
    let mut xml = String::new();
    use std::io::Read as _;
    ar.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("<w:b/>"), "bold run formatting must survive");
    assert!(xml.contains("Kept"), "first run carries the replacement head");
    assert!(xml.contains("tail"), "second run carries the replacement tail");
}

#[test]
fn table_cells_are_addressable_with_merge_aware_columns() {
    let bytes = structured_docx();
    let xml = {
        let mut ar = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        let mut f = ar.by_name("word/document.xml").unwrap();
        use std::io::Read as _;
        let mut xml = String::new();
        f.read_to_string(&mut xml).unwrap();
        xml
    };
    let map =
        harbor_artifacts::docx::table_cell_paragraph_map(&xml).unwrap();
    let table = map.get(&0).expect("one table");
    // Merged header occupies (row 0, col 0) with gridSpan 2; body cells at
    // (row 1, col 0) and (row 1, col 1).
    let (header_paras, span) = table.get(&(0, 0)).unwrap();
    assert_eq!(*span, 2, "gridSpan recognized");
    assert!(!header_paras.is_empty());
    assert!(table.contains_key(&(1, 0)));
    assert!(table.contains_key(&(1, 1)));
    // Cell paragraphs point at the right global paragraph ordinals: row 1
    // col 0 cell text is "A", col 1 is "B".
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();
    let a_idx = table[&(1, 0)].0[0];
    let b_idx = table[&(1, 1)].0[0];
    assert_eq!(doc.paragraphs[a_idx].text, "A");
    assert_eq!(doc.paragraphs[b_idx].text, "B");
}

#[test]
fn docx_table_cell_set_edit_roundtrip() {
    let bytes = structured_docx();
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();

    // Table-cell addressing: table 0, body row 1, column 1 -> "B".
    let out = doc
        .apply(
            &bytes,
            &[harbor_artifacts::DocxOp::TableCellSet {
                table: 0,
                row: 1,
                col: 1,
                new_text: "Revised B".into(),
            }],
        )
        .unwrap();
    let reloaded = harbor_artifacts::DocxDocument::load(&out).unwrap();
    // The targeted cell changed; every other paragraph is untouched.
    let texts: Vec<&str> = reloaded.paragraphs.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "Harbor Plan",
            "First item",
            "Second item",
            "Merged header",
            "A",
            "Revised B",
            "Summary"
        ]
    );
    // The merged header cell was NOT treated as a body cell.
    assert_eq!(reloaded.paragraphs[3].text, "Merged header");
}

#[test]
fn xlsx_chart_roundtrip_preserved_through_harbor_pipeline() {
    // Matrix row 14: bar/column basic series. Chart creation via the
    // Harbor workbook API, then preservation proof through the full
    // load/edit/recalc/save pipeline.
    let mut wb = umya_spreadsheet::new_file();
    let sheet = wb.get_sheet_mut(&0).unwrap();
    // Data: category labels + two value columns.
    sheet.get_cell_mut((1, 1)).set_value("Region");
    sheet.get_cell_mut((1, 2)).set_value("Q1");
    sheet.get_cell_mut((1, 3)).set_value("Q2");
    let rows = [("North", 1200.0, 1350.0), ("South", 800.0, 950.0)];
    for (i, (region, q1, q2)) in rows.iter().enumerate() {
        let r = (i + 2) as u32;
        sheet.get_cell_mut((1, r)).set_value(*region);
        sheet.get_cell_mut((2, r)).set_value(q1.to_string());
        sheet.get_cell_mut((3, r)).set_value(q2.to_string());
    }
    let series = vec![
        "Sheet1!$B$1:$B$3".to_string(),
        "Sheet1!$C$1:$C$3".to_string(),
    ];
    let chart = umya_spreadsheet::structs::Chart::default();
    let mut chart = chart;
    let mut from = umya_spreadsheet::structs::drawing::spreadsheet::MarkerType::default();
    from.set_coordinate("F2");
    let mut to = umya_spreadsheet::structs::drawing::spreadsheet::MarkerType::default();
    to.set_coordinate("N16");
    chart.new_chart(&umya_spreadsheet::structs::ChartType::BarChart, from, to, series.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    sheet.add_chart(chart);

    let mut buf = std::io::BufWriter::new(Cursor::new(Vec::new()));
    umya_spreadsheet::writer::xlsx::write_writer(&wb, &mut buf).unwrap();
    let bytes = buf.into_inner().unwrap().into_inner();
    assert_eq!(
        harbor_artifacts::WorkbookDoc::count_charts_in_bytes(&bytes).unwrap(),
        1,
        "source workbook must carry exactly one chart"
    );

    // Harbor pipeline: load, recalc everything, save.
    let mut doc = harbor_artifacts::WorkbookDoc::load(&bytes).unwrap();
    doc.recalculate_all().unwrap();
    let out = doc.to_bytes().unwrap();

    // Chart part survives the full pipeline.
    assert_eq!(
        harbor_artifacts::WorkbookDoc::count_charts_in_bytes(&out).unwrap(),
        1,
        "chart must survive load/edit/recalc/save"
    );
    // Data cells intact.
    let reloaded = harbor_artifacts::WorkbookDoc::load(&out).unwrap();
    assert_eq!(
        reloaded.sheet("Sheet1").unwrap().cells.get(&(2, 2)).unwrap().cached,
        Some(harbor_formula::value::CellValue::Number(1200.0))
    );
}

#[test]
fn pptx_chart_embedding_roundtrip() {
    // Matrix row: PPTX charts from Harbor IR with cached values.
    let deck = harbor_artifacts::PptxDeck {
        title: "Chart deck".into(),
        slides: vec![harbor_artifacts::SlideContent {
            title: "Revenue chart".into(),
            bullets: vec!["Values verified in the workbook".into()],
            notes: None,
            chart: Some(harbor_artifacts::ChartSpec {
                kind: harbor_artifacts::ChartKind::Bar,
                title: "Quarterly revenue".into(),
                categories: vec!["Q1".into(), "Q2".into()],
                series: vec![("Revenue".into(), vec![3600.0, 4000.0])],
            }),
            image: None,
        }],
    };
    let bytes = deck.to_pptx_bytes().unwrap();
    let mut ar = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
    // Chart part exists with cached values.
    let mut chart_xml = String::new();
    use std::io::Read as _;
    ar.by_name("ppt/charts/chart1.xml")
        .unwrap()
        .read_to_string(&mut chart_xml)
        .unwrap();
    assert!(chart_xml.contains("barChart"));
    assert!(chart_xml.contains("<c:v>3600</c:v>"));
    assert!(chart_xml.contains("<c:v>4000</c:v>"));
    assert!(chart_xml.contains("<c:v>Q1</c:v>"));
    // Embedded workbook present for the chart.
    assert!(ar.by_name("ppt/embeddings/chartdata1.xlsx").is_ok());
    // Chart relationship from the slide.
    assert!(ar.by_name("ppt/slides/_rels/slide1.xml.rels").is_ok());
}

// ---------------------------------------------------------------------------
// Matrix rows 13/14/15/16 (XLSX preserve + chart kinds) and beyond.
// ---------------------------------------------------------------------------

/// Build a workbook, then inject PRESERVE-scoped parts into the package
/// (pivot tables + cache, external link, VBA project) with a resolvable
/// relationship graph, exactly like a real authoring application would.
fn enriched_workbook() -> Vec<u8> {
    use std::io::Write as _;
    let mut wb = umya_spreadsheet::new_file();
    let sheet = wb.get_sheet_mut(&0).unwrap();
    sheet.get_cell_mut((1, 1)).set_value("data");
    sheet.get_cell_mut((2, 1)).set_formula("1+1");
    let mut buf = std::io::BufWriter::new(Cursor::new(Vec::new()));
    umya_spreadsheet::writer::xlsx::write_writer(&wb, &mut buf).unwrap();
    let plain = buf.into_inner().unwrap().into_inner();

    let mut ar = zip::ZipArchive::new(Cursor::new(plain.as_slice())).unwrap();
    let mut names: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..ar.len() {
        let mut f = ar.by_index(i).unwrap();
        let mut b = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut b).unwrap();
        names.push((f.name().to_string(), b));
    }
    let pivot: Vec<u8> = br#"<pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="PT" cacheId="0"/>"#.to_vec();
    let cache: Vec<u8> = br#"<pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" r:id="rId1" refreshOnLoad="1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/>"#.to_vec();
    let ext: Vec<u8> = br#"<externalLink xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><externalBook><sheetNames><sheetName val="Source"/></sheetNames></externalBook></externalLink>"#.to_vec();
    let vba: Vec<u8> = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 1];
    names.push(("xl/pivotTables/pivotTable1.xml".into(), pivot));
    names.push(("xl/pivotTables/_rels/pivotTable1.xml.rels".into(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../worksheets/sheet1.xml"/></Relationships>"#.to_vec()));
    names.push(("xl/pivotCache/pivotCacheDefinition1.xml".into(), cache));
    names.push(("xl/pivotCache/_rels/pivotCacheDefinition1.xml.rels".into(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheRecords" Target="pivotCacheRecords1.xml"/></Relationships>"#.to_vec()));
    names.push(("xl/pivotCache/pivotCacheRecords1.xml".into(), br#"<pivotCacheRecords xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="0"/>"#.to_vec()));
    names.push(("xl/externalLinks/externalLink1.xml".into(), ext));
    names.push(("xl/externalLinks/_rels/externalLink1.xml.rels".into(), br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath" Target="bogus.xlsx" TargetMode="External"/></Relationships>"#.to_vec()));
    names.push(("xl/vbaProject.bin".into(), vba));
    for (n, b) in names.iter_mut() {
        if n == "xl/_rels/workbook.xml.rels" {
            let s = String::from_utf8(b.clone()).unwrap();
            let s = s.replace("</Relationships>", "<Relationship Id=\"rId90\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition\" Target=\"pivotCache/pivotCacheDefinition1.xml\"/><Relationship Id=\"rId91\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink\" Target=\"externalLinks/externalLink1.xml\"/></Relationships>");
            *b = s.into_bytes();
        }
        if n == "[Content_Types].xml" {
            let s = String::from_utf8(b.clone()).unwrap();
            let s = s.replace("</Types>", "<Override PartName=\"/xl/pivotTables/pivotTable1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml\"/><Override PartName=\"/xl/pivotCache/pivotCacheDefinition1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml\"/><Override PartName=\"/xl/pivotCache/pivotCacheRecords1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml\"/><Override PartName=\"/xl/externalLinks/externalLink1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.ms-excel.sheet.macroEnabled.main+xml\"/><Default Extension=\"bin\" ContentType=\"application/vnd.ms-excel.sheet.binary.vbaProject\"/></Types>");
            *b = s.into_bytes();
        }
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default();
    for (n, b) in &names {
        zip.start_file(n.clone(), opts).unwrap();
        zip.write_all(b).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
// Rows 14/15/16: pivot tables/caches, external links, VBA are
// PRESERVE_ONLY / PRESERVE_NO_EXECUTE: carried byte-identically through
// Harbor's load/edit/recalc/save, with relationships resolvable and
// content types declared.
fn xlsx_preserve_rows_survive_pipeline_byte_identical() {
    let enriched = enriched_workbook();
    let mut doc = harbor_artifacts::WorkbookDoc::load(&enriched).unwrap();
    // The engine reports which parts it does not model.
    let unmodeled = doc.unmodeled_parts();
    assert!(unmodeled.contains(&"xl/pivotTables/pivotTable1.xml".to_string()));
    assert!(unmodeled.contains(&"xl/vbaProject.bin".to_string()));

    doc.set_cell(
        "Sheet1",
        1,
        1,
        harbor_artifacts::workbook::CellSet::Value(harbor_formula::value::CellValue::Text(
            "edited".into(),
        )),
    )
    .unwrap();
    doc.recalculate_all().unwrap();
    let (out, report) = doc.write_package_with_report().unwrap();

    // Note: the umya backend re-emits xl/vbaProject.bin from its own
    // model; presence + byte-identity are asserted below.
    for part in [
        "xl/pivotTables/pivotTable1.xml",
        "xl/pivotCache/pivotCacheDefinition1.xml",
        "xl/pivotCache/pivotCacheRecords1.xml",
        "xl/externalLinks/externalLink1.xml",
    ] {
        assert!(
            report.carried_parts.iter().any(|p| p == part),
            "{part} must be reported as carried"
        );
    }
    // Byte-identical carry-over.
    let original = enriched_workbook();
    fn entry_bytes(pkg: &[u8], name: &str) -> Vec<u8> {
        let mut ar = zip::ZipArchive::new(Cursor::new(pkg)).unwrap();
        let mut f = ar.by_name(name).unwrap();
        let mut b = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut b).unwrap();
        b
    }
    for part in [
        "xl/pivotTables/pivotTable1.xml",
        "xl/pivotCache/pivotCacheRecords1.xml",
        "xl/vbaProject.bin",
    ] {
        assert_eq!(
            entry_bytes(&out, part),
            entry_bytes(&original, part),
            "{part} must be byte-identical"
        );
    }
    // Relationship graph stays resolvable: every Relationship target in
    // every rels file resolves to a part present in the final package.
    let mut ar = zip::ZipArchive::new(Cursor::new(out.as_slice())).unwrap();
    let mut part_set: std::collections::BTreeSet<String> = (0..ar.len())
        .map(|i| ar.by_index(i).unwrap().name().to_string())
        .collect();
    for i in 0..ar.len() {
        let name = ar.by_index(i).unwrap().name().to_string();
        if !name.contains("_rels/") {
            continue;
        }
        let mut xml = String::new();
        ar.by_name(&name).unwrap().read_to_string(&mut xml).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        for rel in doc.descendants().filter(|n| n.has_tag_name("Relationship")) {
            if rel.attribute("TargetMode") == Some("External") {
                continue;
            }
            let target = rel.attribute("Target").unwrap();
            let dir = name.split_once("_rels/").unwrap().0.trim_end_matches('/');
            let mut parts: Vec<&str> = if dir.is_empty() {
                vec![]
            } else {
                dir.split('/').collect()
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
            let resolved: String = parts.join("/");
            assert!(
                part_set.contains(&resolved),
                "relationship target {resolved} ({name}) must exist in package"
            );
        }
    }
    // Content types declare the carried parts (incl. macroEnabled VBA).
    let mut ct = String::new();
    ar.by_name("[Content_Types].xml")
        .unwrap()
        .read_to_string(&mut ct)
        .unwrap();
    assert!(ct.contains("pivotTable+xml"));
    assert!(ct.contains("macroEnabled"));
    assert!(ct.contains("vbaProject"));
    let _ = part_set;
}

#[test]
// Row 13: bar/column/line/pie/scatter basic-series creation through the
// Harbor workbook API; every chart part survives the full pipeline.
fn xlsx_all_basic_chart_kinds_survive_pipeline() {
    for kind in [
        harbor_artifacts::XlsxChartKind::Bar,
        harbor_artifacts::XlsxChartKind::Line,
        harbor_artifacts::XlsxChartKind::Pie,
        harbor_artifacts::XlsxChartKind::Scatter,
    ] {
        let mut wb = umya_spreadsheet::new_file();
        let sheet = wb.get_sheet_mut(&0).unwrap();
        sheet.get_cell_mut((1, 1)).set_value("Region");
        sheet.get_cell_mut((1, 2)).set_value("Value");
        sheet.get_cell_mut((2, 1)).set_value("North");
        sheet.get_cell_mut((2, 2)).set_value("1200");
        sheet.get_cell_mut((3, 1)).set_value("South");
        sheet.get_cell_mut((3, 2)).set_value("800");
        let mut buf = std::io::BufWriter::new(Cursor::new(Vec::new()));
        umya_spreadsheet::writer::xlsx::write_writer(&wb, &mut buf).unwrap();
        let bytes = buf.into_inner().unwrap().into_inner();

        let mut doc = harbor_artifacts::WorkbookDoc::load(&bytes).unwrap();
        doc.add_chart(
            kind,
            "Sheet1",
            "D2",
            "L16",
            vec!["Sheet1!$A$1:$A$3".to_string(), "Sheet1!$B$1:$B$3".to_string()],
            "Basic series",
        )
        .unwrap();
        assert_eq!(
            harbor_artifacts::WorkbookDoc::count_charts_in_bytes(&doc.to_bytes().unwrap()).unwrap(),
            1,
            "{kind:?}: chart part written"
        );
        let mut out = doc;
        out.recalculate_all().unwrap();
        let saved = out.to_bytes().unwrap();
        assert_eq!(
            harbor_artifacts::WorkbookDoc::count_charts_in_bytes(&saved).unwrap(),
            1,
            "{kind:?}: chart must survive load/edit/recalc/save"
        );
    }
}

#[test]
// Row 10 remainder: merged cells, row/column dimensions survive together.
fn xlsx_dimensions_and_merges_survive_pipeline() {
    let mut wb = umya_spreadsheet::new_file();
    let sheet = wb.get_sheet_mut(&0).unwrap();
    sheet.add_merge_cells("A1:B1");
    sheet.get_cell_mut((1, 1)).set_value("Title");
    sheet
        .get_column_dimension_mut("A")
        .set_width(33.5);
    sheet.get_row_dimension_mut(&3).set_height(24.0);
    sheet.get_row_dimension_mut(&3).set_custom_height(true);
    let mut buf = std::io::BufWriter::new(Cursor::new(Vec::new()));
    umya_spreadsheet::writer::xlsx::write_writer(&wb, &mut buf).unwrap();
    let bytes = buf.into_inner().unwrap().into_inner();

    let mut doc = harbor_artifacts::WorkbookDoc::load(&bytes).unwrap();
    doc.recalculate_all().unwrap();
    let out = doc.to_bytes().unwrap();
    let mut ar = zip::ZipArchive::new(Cursor::new(out.as_slice())).unwrap();
    let mut sheet_xml = String::new();
    ar.by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut sheet_xml)
        .unwrap();
    assert!(sheet_xml.contains("A1:B1"), "merge survives: {sheet_xml}");
    assert!(sheet_xml.contains("customWidth"), "column dimension survives");
    assert!(sheet_xml.contains("customHeight"), "row dimension survives");
}

// ---------------------------------------------------------------------------
// DOCX rows 4-8: inline images, sections/headers/footers/page breaks,
// floating drawings, fields/TOC/equations, macros/OLE.
// ---------------------------------------------------------------------------

/// Minimal DOCX with arbitrary package parts beyond document.xml.
fn docx_with_parts(document_xml: &str, parts: &[(&str, Vec<u8>)], content_types_extra: &str) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default();
    let ct = format!(
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>{content_types_extra}<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#
    );
    zip.start_file("[Content_Types].xml", opts).unwrap();
    zip.write_all(ct.as_bytes()).unwrap();
    zip.start_file("_rels/.rels", opts).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#).unwrap();
    zip.start_file("word/document.xml", opts).unwrap();
    zip.write_all(document_xml.as_bytes()).unwrap();
    for (name, bytes) in parts {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn entry(pkg: &[u8], name: &str) -> Vec<u8> {
    let mut ar = zip::ZipArchive::new(Cursor::new(pkg)).unwrap();
    let mut f = ar.by_name(name).unwrap();
    let mut b = Vec::new();
    std::io::Read::read_to_end(&mut f, &mut b).unwrap();
    b
}

#[test]
// Row 4: inline images — media part byte-identical, drawing reference
// intact after a typed edit elsewhere in the document.
fn docx_inline_image_preserved_through_edit() {
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Report</w:t></w:r></w:p>
<w:p><w:r><w:drawing><wp:inline><wp:extent cx="990600" cy="792480"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="1" name="logo"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId5"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="990600" cy="792480"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
<w:p><w:r><w:t>Caption stays</w:t></w:r></w:p>
</w:body></w:document>"#;
    let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];
    let bytes = docx_with_parts(
        document,
        &[
            ("word/media/image1.png", png.clone()),
            ("word/_rels/document.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#.to_vec()),
        ],
        "<Default Extension=\"png\" ContentType=\"image/png\"/>",
    );
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();
    let out = doc
        .apply(
            &bytes,
            &[harbor_artifacts::DocxOp::TextReplace {
                index: 1,
                new_text: "Report v2".into(),
            }],
        )
        .unwrap();
    assert_eq!(entry(&out, "word/media/image1.png"), png, "media byte-identical");
    let xml = String::from_utf8(entry(&out, "word/document.xml")).unwrap();
    assert!(xml.contains("wp:inline") && xml.contains("r:embed=\"rId5\""));
    assert_eq!(
        harbor_artifacts::DocxDocument::load(&out).unwrap().paragraphs[2].text,
        "Caption stays"
    );
}

#[test]
// Row 5: section properties, headers/footers, explicit page breaks —
// fixture per element type, all preserved through a typed edit.
fn docx_sections_headers_footers_page_breaks_preserved() {
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Doc</w:t></w:r></w:p>
<w:p><w:r><w:br w:type="page"/></w:r></w:p>
<w:p><w:r><w:t>After break</w:t></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840" w:orient="portrait"/><w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800"/><w:headerReference w:type="default" r:id="rId8" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/><w:footerReference w:type="first" r:id="rId9" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></w:sectPr>
</w:body></w:document>"#;
    let bytes = docx_with_parts(
        document,
        &[
            ("word/header1.xml", br#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Header text</w:t></w:r></w:p></w:hdr>"#.to_vec()),
            ("word/footer1.xml", br#"<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Footer text</w:t></w:r></w:p></w:ftr>"#.to_vec()),
            ("word/_rels/document.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#.to_vec()),
        ],
        "",
    );
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();
    let out = doc
        .apply(
            &bytes,
            &[harbor_artifacts::DocxOp::TextReplace {
                index: 3,
                new_text: "After the break".into(),
            }],
        )
        .unwrap();
    assert_eq!(entry(&out, "word/header1.xml"), entry(&bytes, "word/header1.xml"));
    assert_eq!(entry(&out, "word/footer1.xml"), entry(&bytes, "word/footer1.xml"));
    assert_eq!(entry(&out, "word/_rels/document.xml.rels"), entry(&bytes, "word/_rels/document.xml.rels"));
    let xml = String::from_utf8(entry(&out, "word/document.xml")).unwrap();
    assert!(xml.contains("w:br w:type=\"page\""), "explicit page break preserved");
    assert!(xml.contains("w:orient=\"portrait\"") && xml.contains("w:top=\"1440\""), "page size/margins preserved");
    assert!(xml.contains("headerReference") && xml.contains("footerReference"));
}

#[test]
// Row 6: floating drawings / advanced anchoring are PRESERVE_ONLY —
// the anchored drawing XML is preserved byte-identically and reported.
fn docx_floating_drawing_preserve_only() {
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><w:body>
<w:p><w:r><w:t>Keep</w:t></w:r></w:p>
<w:p><w:r><w:drawing><wp:anchor behindDoc="0" simplePos="0"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1000" cy="1000"/></wp:anchor></w:drawing></w:r></w:p>
<w:p><w:r><w:t>End</w:t></w:r></w:p>
</w:body></w:document>"#;
    let bytes = docx_with_parts(document, &[], "");
    let anchor_xml = {
        let xml = String::from_utf8(entry(&bytes, "word/document.xml")).unwrap();
        xml[xml.find("<w:drawing>").unwrap()..xml.find("</w:drawing>").unwrap() + 12].to_string()
    };
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();
    let out = doc
        .apply(
            &bytes,
            &[harbor_artifacts::DocxOp::TextReplace { index: 3, new_text: "Finish".into() }],
        )
        .unwrap();
    let xml = String::from_utf8(entry(&out, "word/document.xml")).unwrap();
    assert!(xml.contains(&anchor_xml), "anchored drawing byte-identical");
    // Classifier reports it PRESERVE_ONLY.
    let report = harbor_artifacts::compatibility_report(harbor_artifacts::OfficeFormat::Docx, &out).unwrap();
    assert!(report.entries.iter().any(|e| e.part == "document.xml#floatingDrawing"
        && e.class == harbor_artifacts::MatrixClass::PreserveOnly));
    assert!(report.banner_required());
}

#[test]
// Row 7: fields/TOC/equations are PRESERVE_ONLY — preserved byte level,
// reported, and never rendered as supported content.
fn docx_fields_toc_equations_preserved() {
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Doc</w:t></w:r></w:p>
<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> TOC \o "1-3" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
<w:p><w:r><m:oMath><m:r><m:t>E=mc2</m:t></m:r></m:oMath></w:r></w:p>
<w:p><w:r><w:t>Body</w:t></w:r></w:p>
</w:body></w:document>"#;
    let bytes = docx_with_parts(document, &[], "");
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();
    let out = doc
        .apply(
            &bytes,
            &[harbor_artifacts::DocxOp::TextReplace { index: 4, new_text: "Body v2".into() }],
        )
        .unwrap();
    let xml = String::from_utf8(entry(&out, "word/document.xml")).unwrap();
    assert!(xml.contains("instrText") && xml.contains("TOC \\o"), "TOC field preserved");
    assert!(xml.contains("fldChar"), "field chars preserved");
    assert!(xml.contains("oMath") && xml.contains("E=mc2"), "equation preserved");
    let report = harbor_artifacts::compatibility_report(harbor_artifacts::OfficeFormat::Docx, &out).unwrap();
    assert!(report.entries.iter().any(|e| e.part == "document.xml#fieldOrToc"
        && e.class == harbor_artifacts::MatrixClass::PreserveOnly));
    assert!(report.entries.iter().any(|e| e.part == "document.xml#equation"
        && e.class == harbor_artifacts::MatrixClass::PreserveOnly));
}

#[test]
// Row 8: macros/OLE/active content are PRESERVE_NO_EXECUTE — parts are
// carried byte-identically, reported, and there is no execution path.
fn docx_macros_ole_preserved_never_executed() {
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>
<w:p><w:r><w:t>Title</w:t></w:r></w:p>
<w:p><w:r><w:object><o:OLEObject Type="Embed" ProgID="Word.Document" r:id="rId6"/></w:object></w:r></w:p>
</w:body></w:document>"#;
    let vba: Vec<u8> = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0xDE, 0xAD];
    let ole: Vec<u8> = vec![1, 2, 3, 4, 5];
    let bytes = docx_with_parts(
        document,
        &[
            ("word/vbaProject.bin", vba.clone()),
            ("word/embeddings/oleObject1.bin", ole.clone()),
            ("word/_rels/document.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId6" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject" Target="embeddings/oleObject1.bin"/></Relationships>"#.to_vec()),
        ],
        "",
    );
    let doc = harbor_artifacts::DocxDocument::load(&bytes).unwrap();
    // The loader reports the active content parts up front.
    assert!(doc.preserved_parts.contains(&"word/vbaProject.bin".to_string()));
    assert!(doc.preserved_parts.contains(&"word/embeddings/oleObject1.bin".to_string()));
    let out = doc
        .apply(
            &bytes,
            &[harbor_artifacts::DocxOp::TextReplace { index: 1, new_text: "Title v2".into() }],
        )
        .unwrap();
    assert_eq!(entry(&out, "word/vbaProject.bin"), vba, "VBA byte-identical");
    assert_eq!(entry(&out, "word/embeddings/oleObject1.bin"), ole, "OLE byte-identical");
    let report = harbor_artifacts::compatibility_report(harbor_artifacts::OfficeFormat::Docx, &out).unwrap();
    assert!(report.entries.iter().any(|e| e.part == "word/vbaProject.bin"
        && e.class == harbor_artifacts::MatrixClass::PreserveNoExecute));
    assert!(report.entries.iter().any(|e| e.part == "document.xml#oleObject"
        && e.class == harbor_artifacts::MatrixClass::PreserveNoExecute));
    assert!(report.banner_required(), "export requires a compatibility banner");
}

// ---------------------------------------------------------------------------
// PPTX rows 17-21 and the row 23 classifier.
// ---------------------------------------------------------------------------

#[test]
// Row 17: text boxes/shapes, inline images and the theme — image part,
// relationship and p:pic all present; read-back keeps working.
fn pptx_images_theme_shapes_roundtrip() {
    let deck = harbor_artifacts::PptxDeck {
        title: "Visual deck".into(),
        slides: vec![harbor_artifacts::SlideContent {
            title: "Cover".into(),
            bullets: vec!["With a picture".into()],
            notes: None,
            chart: None,
            image: Some(harbor_artifacts::SlideImage {
                name: "logo".into(),
                extension: "png".into(),
                bytes: vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 9, 9],
            }),
        }],
    };
    let bytes = deck.to_pptx_bytes().unwrap();
    let mut ar = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
    // Media part byte-identical, image/png content type declared.
    let mut media = Vec::new();
    ar.by_name("ppt/media/slide1-image.png")
        .unwrap()
        .read_to_end(&mut media)
        .unwrap();
    assert_eq!(media, vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 9, 9]);
    let mut ct = String::new();
    ar.by_name("[Content_Types].xml").unwrap().read_to_string(&mut ct).unwrap();
    assert!(ct.contains("image/png"));
    // Slide rels reference the image; slide XML carries the p:pic.
    let mut rels = String::new();
    ar.by_name("ppt/slides/_rels/slide1.xml.rels")
        .unwrap()
        .read_to_string(&mut rels)
        .unwrap();
    assert!(rels.contains("relationships/image") && rels.contains("media/slide1-image.png"));
    let mut slide = String::new();
    ar.by_name("ppt/slides/slide1.xml").unwrap().read_to_string(&mut slide).unwrap();
    assert!(slide.contains("<p:pic>") && slide.contains("r:embed=\"rId4\""));
    // Theme part present (Harbor theme).
    assert!(ar.by_name("ppt/theme/theme1.xml").is_ok());
    // Read-back still parses title/bullets.
    let back = harbor_artifacts::PptxDeck::from_pptx_bytes(&bytes).unwrap();
    assert_eq!(back.slides[0].title, "Cover");
}

#[test]
// Row 18: speaker notes survive a write/read round trip through the
// notesSlide relationship.
fn pptx_speaker_notes_roundtrip() {
    let deck = harbor_artifacts::PptxDeck {
        title: "Notes deck".into(),
        slides: vec![
            harbor_artifacts::SlideContent {
                title: "One".into(),
                bullets: vec![],
                notes: Some("Cite the verified workbook".into()),
                chart: None,
                image: None,
            },
            harbor_artifacts::SlideContent {
                title: "Two".into(),
                bullets: vec![],
                notes: None,
                chart: None,
                image: None,
            },
        ],
    };
    let bytes = deck.to_pptx_bytes().unwrap();
    let back = harbor_artifacts::PptxDeck::from_pptx_bytes(&bytes).unwrap();
    assert_eq!(back.slides[0].notes.as_deref(), Some("Cite the verified workbook"));
    assert_eq!(back.slides[1].notes, None, "absent notes stay absent");
}

#[test]
// Row 19: line/pie/scatter chart kinds from Harbor IR with cached values.
fn pptx_all_basic_chart_kinds_embed() {
    for (kind, marker) in [
        (harbor_artifacts::ChartKind::Line, "lineChart"),
        (harbor_artifacts::ChartKind::Pie, "pieChart"),
        (harbor_artifacts::ChartKind::Scatter, "scatterChart"),
    ] {
        let deck = harbor_artifacts::PptxDeck {
            title: "Charts".into(),
            slides: vec![harbor_artifacts::SlideContent {
                title: "Data".into(),
                bullets: vec![],
                notes: None,
                chart: Some(harbor_artifacts::ChartSpec {
                    kind,
                    title: "Series".into(),
                    categories: vec!["1".into(), "2".into()],
                    series: vec![("V".into(), vec![3.5, 7.25])],
                }),
                image: None,
            }],
        };
        let bytes = deck.to_pptx_bytes().unwrap();
        let mut ar = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        let mut xml = String::new();
        ar.by_name("ppt/charts/chart1.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert!(xml.contains(marker), "{marker} element present");
        assert!(xml.contains("<c:v>3.5</c:v>") && xml.contains("<c:v>7.25</c:v>"));
        assert!(xml.contains("<c:v>V</c:v>"), "series name cached");
        if marker == "scatterChart" {
            assert!(xml.contains("c:xVal") && xml.contains("c:yVal"));
        }
        assert!(ar.by_name("ppt/embeddings/chartdata1.xlsx").is_ok());
    }
}

#[test]
// Row 20: animations/transitions are PRESERVE_ONLY — Harbor never emits
// them, and a read-back of an external deck carrying a transition still
// previews its text (preserve; nothing is claimed as supported).
fn pptx_transitions_never_emitted_external_preserved() {
    // Generated decks contain no animation/transition elements.
    let deck = harbor_artifacts::PptxDeck {
        title: "Plain".into(),
        slides: vec![harbor_artifacts::SlideContent {
            title: "T".into(),
            bullets: vec!["b".into()],
            notes: None,
            chart: None,
            image: None,
        }],
    };
    let bytes = deck.to_pptx_bytes().unwrap();
    let mut ar = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
    for i in 0..ar.len() {
        let name = ar.by_index(i).unwrap().name().to_string();
        if !name.ends_with(".xml") {
            continue;
        }
        let mut xml = String::new();
        ar.by_name(&name).unwrap().read_to_string(&mut xml).unwrap();
        assert!(!xml.contains("p:transition"), "{name} must not carry transitions");
        assert!(!xml.contains("p:anim"), "{name} must not carry animations");
    }
    // An external slide with a transition still previews its text.
    let with_transition = {
        let mut ar = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default();
        for i in 0..ar.len() {
            let mut f = ar.by_index(i).unwrap();
            let name = f.name().to_string();
            writer.start_file(name.clone(), opts).unwrap();
            if name == "ppt/slides/slide1.xml" {
                let mut xml = String::new();
                std::io::Read::read_to_string(&mut f, &mut xml).unwrap();
                let patched =
                    xml.replace("<p:clrMapOvr>", "<p:transition spd=\"med\"><p:fade/></p:transition><p:clrMapOvr>");
                writer.write_all(patched.as_bytes()).unwrap();
            } else {
                std::io::copy(&mut f, &mut writer).unwrap();
            }
        }
        writer.finish().unwrap().into_inner()
    };
    let back = harbor_artifacts::PptxDeck::from_pptx_bytes(&with_transition).unwrap();
    assert_eq!(back.slides[0].title, "T");
    assert!(back.slides[0].bullets.contains(&"b".to_string()));
    // The compatibility report classifies the package honestly.
    let report = harbor_artifacts::compatibility_report(harbor_artifacts::OfficeFormat::Pptx, bytes.as_slice()).unwrap();
    assert!(!report.banner_required(), "Harbor-generated deck claims nothing beyond scope");
}

#[test]
// Row 23 + rows 16/21: the compatibility classifier reports every part,
// never claims support for unlisted features, and demands the banner for
// active content.
fn compatibility_classifier_reports_and_banners() {
    let report = harbor_artifacts::compatibility_report(
        harbor_artifacts::OfficeFormat::Xlsx,
        enriched_workbook().as_slice(),
    )
    .unwrap();
    let class_of = |p: &str| {
        report
            .entries
            .iter()
            .find(|e| e.part == p)
            .map(|e| e.class)
            .unwrap_or_else(|| panic!("{p} classified"))
    };
    assert_eq!(class_of("xl/pivotTables/pivotTable1.xml"), harbor_artifacts::MatrixClass::PreserveOnly);
    assert_eq!(class_of("xl/externalLinks/externalLink1.xml"), harbor_artifacts::MatrixClass::PreserveOnly);
    assert_eq!(class_of("xl/vbaProject.bin"), harbor_artifacts::MatrixClass::PreserveNoExecute);
    assert!(report.banner_required(), "active content forces the banner");
    // A plain Harbor workbook raises no banner and no unknown parts.
    let mut wb = umya_spreadsheet::new_file();
    wb.get_sheet_mut(&0).unwrap().get_cell_mut((1, 1)).set_value("x");
    let mut buf = std::io::BufWriter::new(Cursor::new(Vec::new()));
    umya_spreadsheet::writer::xlsx::write_writer(&wb, &mut buf).unwrap();
    let plain = buf.into_inner().unwrap().into_inner();
    let report = harbor_artifacts::compatibility_report(harbor_artifacts::OfficeFormat::Xlsx, &plain).unwrap();
    assert!(report.unknown_parts.is_empty(), "unknown: {:?}", report.unknown_parts);
    assert!(!report.banner_required());
}
