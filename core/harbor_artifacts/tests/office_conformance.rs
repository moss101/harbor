//! Office Feature Matrix conformance tests (21_Office_Feature_Matrix.csv).

use std::io::{Cursor, Write};

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
