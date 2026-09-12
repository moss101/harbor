//! One-shot fixture generator: writes real artifact bytes produced by the
//! pinned engine. Output is committed under fixtures/office for Dart-side
//! preview tests. Run: cargo run -p harbor_render --example gen_fixtures

use harbor_artifacts::workbook::WorkbookDoc;
use harbor_formula::engine::HarborWorkbook;
use harbor_formula::value::CellValue;

fn board_workbook() -> Vec<u8> {
    let mut wb = HarborWorkbook::new();
    let rows = [("North", 1200.0, 1350.0), ("South", 800.0, 950.0), ("East", 640.0, 700.0), ("West", 960.0, 1000.0)];
    for (i, (region, q1, q2)) in rows.iter().enumerate() {
        let r = (i + 2) as u32;
        wb.set_value("Sheet1", r, 1, CellValue::Text(region.to_string()));
        wb.set_value("Sheet1", r, 2, CellValue::Number(*q1));
        wb.set_value("Sheet1", r, 3, CellValue::Number(*q2));
    }
    wb.set_value("Sheet1", 1, 2, CellValue::Text("Q1".into()));
    wb.set_value("Sheet1", 1, 3, CellValue::Text("Q2".into()));
    wb.set_formula("Sheet1", 6, 2, "=SUM(B2:B5)");
    wb.set_formula("Sheet1", 6, 3, "=SUM(C2:C5)");
    wb.to_xlsx_bytes()
}

fn main() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
        .parent().unwrap().join("fixtures/office/board_demo.xlsx");
    let bytes = board_workbook();
    // Prove the real path: load, recalc every formula, save.
    let mut doc = WorkbookDoc::load(&bytes).unwrap();
    let recalc = doc.recalculate_all().unwrap();
    assert_eq!(
        recalc.get(&("Sheet1".to_string(), 6, 2)),
        Some(&CellValue::Number(3600.0))
    );
    let final_bytes = doc.to_bytes().unwrap();
    std::fs::write(&out, final_bytes).unwrap();
    println!("wrote {}", out.display());
}
