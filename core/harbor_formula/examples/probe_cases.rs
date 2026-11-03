//! Developer tool for formula batches: evaluate candidate cases on the
//! pinned engine and print what it returns, one JSON line per case.
//!
//! Usage: cargo run -q -p harbor_formula --example probe_cases -- cases.json
//! `cases.json` is `{"cases":[{id, cells, formula, ...}]}` in corpus format
//! (the `expect` field is not required). Cases are evaluated exactly the
//! way `qualify::run_case` does: pinned clock, formula at Sheet1!1000,1000.

use formualizer::eval::engine::DeterministicMode;
use harbor_formula::{CellValue, HarborWorkbook};
use serde_json::{json, Value};

fn fixture_value(v: &Value) -> CellValue {
    let Some(o) = v.as_object() else {
        return CellValue::Blank;
    };
    if let Some(n) = o.get("n").and_then(|x| x.as_f64()) {
        CellValue::Number(n)
    } else if let Some(s) = o.get("t").and_then(|x| x.as_str()) {
        CellValue::Text(s.into())
    } else if let Some(b) = o.get("b").and_then(|x| x.as_bool()) {
        CellValue::Bool(b)
    } else if let Some(e) = o.get("e").and_then(|x| x.as_str()) {
        CellValue::Error(
            harbor_formula::CellError::from_code(e).unwrap_or(harbor_formula::CellError::Value),
        )
    } else {
        CellValue::Blank
    }
}

fn render(v: &CellValue) -> Value {
    match v {
        CellValue::Blank => json!({}),
        CellValue::Number(n) => json!({ "n": n }),
        CellValue::Text(s) => json!({ "t": s }),
        CellValue::Bool(b) => json!({ "b": b }),
        CellValue::Error(e) => json!({ "e": e.code() }),
    }
}

fn run(case: &Value) -> Value {
    let mut wb = HarborWorkbook::new();
    let (clock, _) = harbor_formula::fixtures::pinned_clock();
    wb.inner_mut()
        .set_deterministic_mode(DeterministicMode::Enabled {
            timestamp_utc: clock,
            timezone: formualizer::eval::timezone::TimeZoneSpec::Utc,
        })
        .expect("clock");
    for cell in case["cells"].as_array().into_iter().flatten() {
        let a = cell.as_array().unwrap();
        wb.set_value(
            a[0].as_str().unwrap(),
            a[1].as_u64().unwrap() as u32,
            a[2].as_u64().unwrap() as u32,
            fixture_value(&a[3]),
        );
    }
    wb.set_formula("Sheet1", 1000, 1000, case["formula"].as_str().unwrap());
    render(&wb.evaluate_cell("Sheet1", 1000, 1000))
}

fn main() {
    let path = std::env::args().nth(1).expect("cases.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    std::panic::set_hook(Box::new(|_| {}));
    for case in doc["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap().to_string();
        let r = std::panic::catch_unwind(|| run(case));
        let out = match r {
            Ok(v) => json!({ "id": id, "engine": v }),
            Err(_) => json!({ "id": id, "engine": { "panic": true } }),
        };
        println!("{out}");
    }
}
