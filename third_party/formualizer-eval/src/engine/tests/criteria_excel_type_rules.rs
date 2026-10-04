//! Excel's criteria typing rules, checked end to end across COUNTIF/SUMIF/AVERAGEIF/COUNTIFS/
//! SUMIFS/AVERAGEIFS/MAXIFS/MINIFS over bounded ranges and whole columns. Expected values are
//! the answers real Excel gave for this exact data (oracle batch 16):
//!   A1:A8 = 10..80 ; B1:B8 = 1, 2, 3, "x", (blank), (blank), "3" (text), TRUE
//! Every code path (scalar matcher, chunked masks, cached Arrow masks, MAXIFS loop) must agree.

use super::common::arrow_eval_config;
use crate::engine::{Engine, EvalConfig};
use crate::test_workbook::TestWorkbook;
use formualizer_common::LiteralValue;
use formualizer_parse::parser::parse;

fn setup(config: EvalConfig) -> Engine<TestWorkbook> {
    let mut e = Engine::new(TestWorkbook::new(), config);
    for i in 0..8u32 {
        e.set_cell_value("Sheet1", i + 1, 1, LiteralValue::Number(10.0 * (i + 1) as f64))
            .unwrap();
    }
    for (row, v) in [
        (1, LiteralValue::Number(1.0)),
        (2, LiteralValue::Number(2.0)),
        (3, LiteralValue::Number(3.0)),
        (4, LiteralValue::Text("x".into())),
        (7, LiteralValue::Text("3".into())),
        (8, LiteralValue::Boolean(true)),
    ] {
        e.set_cell_value("Sheet1", row, 2, v).unwrap();
    }
    e
}

fn eval(e: &mut Engine<TestWorkbook>, f: &str) -> f64 {
    // Away from the data so whole-column references are not self-inclusive.
    e.set_cell_formula("Sheet1", 1, 20, parse(f).unwrap()).unwrap();
    e.evaluate_cell("Sheet1", 1, 20).unwrap();
    match e.get_cell_value("Sheet1", 1, 20).unwrap() {
        LiteralValue::Number(n) => n,
        LiteralValue::Int(i) => i as f64,
        other => panic!("{f} -> {other:?}"),
    }
}

/// (criterion, COUNTIF, SUMIF over A, MAXIFS over A, MINIFS over A) as Excel answered.
const MATRIX: &[(&str, f64, f64, f64, f64)] = &[
    ("<9", 3.0, 60.0, 30.0, 10.0),  // blanks, text "3" and TRUE are not numeric
    (">1", 2.0, 50.0, 30.0, 20.0),
    ("<>3", 7.0, 330.0, 80.0, 10.0), // only the number 3 is excluded (text "3" is not)
    ("=3", 2.0, 100.0, 70.0, 30.0),  // number 3 AND text "3"
    ("3", 2.0, 100.0, 70.0, 30.0),
    ("", 2.0, 110.0, 60.0, 50.0),    // blanks
    ("=", 2.0, 110.0, 60.0, 50.0),
    ("<>", 6.0, 250.0, 80.0, 10.0),  // everything non-blank
    ("x", 1.0, 40.0, 40.0, 40.0),
    ("*", 2.0, 110.0, 70.0, 40.0),   // text cells only
    ("?", 2.0, 110.0, 70.0, 40.0),   // one-character text only
    (">=2", 2.0, 50.0, 30.0, 20.0),
    ("<=2", 2.0, 30.0, 20.0, 10.0),
];

fn check_matrix(config: fn() -> EvalConfig) {
    for &(c, count, sum, max, min) in MATRIX {
        for (b, a) in [("B1:B8", "A1:A8"), ("B:B", "A:A")] {
            // Whole columns: only the criteria that are bounded by the oracle's 8 used rows.
            let whole = b == "B:B";
            if whole && !["<9", "<>3", ""].contains(&c) {
                continue;
            }
            let mut e = setup(config());
            // Blank handling differs for whole columns: the ~1M trailing blanks follow the same
            // rule as interior blanks, so only the count changes for blank-matching criteria.
            let tail = if whole { 1_048_576.0 - 8.0 } else { 0.0 };
            let blank_matches = matches!(c, "<>3" | "");
            let count_adj = if whole && blank_matches { tail } else { 0.0 };
            let countifs_adj = 0.0; // second criterion A>0 bounds COUNTIFS to the 8 data rows
            assert_eq!(eval(&mut e, &format!("=COUNTIF({b},\"{c}\")")), count + count_adj, "COUNTIF {b} {c}");
            assert_eq!(eval(&mut e, &format!("=COUNTIFS({b},\"{c}\",{a},\">0\")")), count + countifs_adj, "COUNTIFS {b} {c}");
            assert_eq!(eval(&mut e, &format!("=SUMIF({b},\"{c}\",{a})")), sum, "SUMIF {b} {c}");
            assert_eq!(eval(&mut e, &format!("=SUMIFS({a},{b},\"{c}\")")), sum, "SUMIFS {b} {c}");
            assert_eq!(eval(&mut e, &format!("=MAXIFS({a},{b},\"{c}\")")), max, "MAXIFS {b} {c}");
            assert_eq!(eval(&mut e, &format!("=MINIFS({a},{b},\"{c}\")")), min, "MINIFS {b} {c}");
            if count > 0.0 && sum > 0.0 {
                let n_num = (sum_count(c)) as f64;
                let avg = sum / n_num;
                let got = eval(&mut e, &format!("=AVERAGEIF({b},\"{c}\",{a})"));
                assert!((got - avg).abs() < 1e-9, "AVERAGEIF {b} {c}: {got} vs {avg}");
                let got = eval(&mut e, &format!("=AVERAGEIFS({a},{b},\"{c}\")"));
                assert!((got - avg).abs() < 1e-9, "AVERAGEIFS {b} {c}: {got} vs {avg}");
            }
        }
    }
}

/// How many rows each criterion selects within A1:A8 (A is numeric on every row).
fn sum_count(c: &str) -> usize {
    match c {
        "<9" => 3,
        ">1" | ">=2" | "<=2" | "=3" | "3" | "" | "=" | "*" | "?" => 2,
        "<>3" => 7,
        "<>" => 6,
        "x" => 1,
        _ => unreachable!(),
    }
}

#[test]
fn criteria_type_rules_match_excel_default_storage() {
    check_matrix(EvalConfig::default);
}

#[test]
fn criteria_type_rules_match_excel_arrow_storage() {
    check_matrix(arrow_eval_config);
}

#[test]
fn text_x_equality_is_case_insensitive() {
    let mut e = setup(arrow_eval_config());
    assert_eq!(eval(&mut e, "=COUNTIF(B1:B8,\"X\")"), 1.0);
    assert_eq!(eval(&mut e, "=SUMIFS(A1:A8,B1:B8,\"X\")"), 40.0);
    assert_eq!(eval(&mut e, "=MAXIFS(A1:A8,B1:B8,\"X\")"), 40.0);
}

#[test]
fn boolean_true_never_matches_a_number_criterion() {
    let mut e = setup(arrow_eval_config());
    // B8 is TRUE: it must satisfy neither "=1" nor ">0" nor "<=1".
    assert_eq!(eval(&mut e, "=COUNTIF(B1:B8,1)"), 1.0);
    assert_eq!(eval(&mut e, "=COUNTIF(B1:B8,\">0\")"), 3.0);
    assert_eq!(eval(&mut e, "=COUNTIF(B1:B8,\"<=1\")"), 1.0);
    assert_eq!(eval(&mut e, "=COUNTIF(B1:B8,TRUE)"), 1.0);
}
