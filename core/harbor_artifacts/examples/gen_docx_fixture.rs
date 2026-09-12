//! One-shot fixture: emit a structured DOCX (headings, numbered list,
//! table with merged cell) for Dart-side preview tests.

use std::io::{Cursor, Write};

fn main() {
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Harbor Plan</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>First item</w:t></w:r></w:p>
<w:tbl><w:tr>
<w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Merged header</w:t></w:r></w:p></w:tc>
</w:tr><w:tr>
<w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
<w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
</w:tr></w:tbl>
</w:body></w:document>"#;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("[Content_Types].xml", opts).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#).unwrap();
    zip.start_file("word/document.xml", opts).unwrap();
    zip.write_all(document.as_bytes()).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("fixtures/office/structured.docx");
    std::fs::write(&out, bytes).unwrap();
    println!("wrote {}", out.display());
}
