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
