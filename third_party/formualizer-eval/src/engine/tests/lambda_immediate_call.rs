use super::common::arrow_eval_config;
use crate::engine::Engine;
use crate::test_workbook::TestWorkbook;
use formualizer_common::{ExcelErrorKind, LiteralValue};
use formualizer_parse::parser::parse;

fn eval(formula: &str) -> Option<LiteralValue> {
    let mut engine = Engine::new(TestWorkbook::new(), arrow_eval_config());
    engine
        .set_cell_formula("Sheet1", 1, 1, parse(formula).unwrap())
        .unwrap();
    engine.evaluate_all().unwrap();
    engine.get_cell_value("Sheet1", 1, 1)
}

fn number(v: Option<LiteralValue>) -> f64 {
    match v {
        Some(LiteralValue::Number(n)) => n,
        Some(LiteralValue::Int(i)) => i as f64,
        other => panic!("expected a number, got {other:?}"),
    }
}

fn error_kind(v: Option<LiteralValue>) -> ExcelErrorKind {
    match v {
        Some(LiteralValue::Error(e)) => e.kind,
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn an_immediately_invoked_lambda_evaluates() {
    assert_eq!(number(eval("=LAMBDA(x,x+1)(41)")), 42.0);
    assert_eq!(number(eval("=LAMBDA(a,b,a*b)(6,7)")), 42.0);
    assert_eq!(number(eval("=LAMBDA(1+2)()")), 3.0, "a lambda with no parameters is its body");
}

#[test]
fn an_immediately_invoked_lambda_nests_and_composes() {
    assert_eq!(number(eval("=LAMBDA(x,x*2)(LAMBDA(y,y+1)(4))")), 10.0);
    assert_eq!(number(eval("=1+LAMBDA(x,x*3)(5)")), 16.0);
}

#[test]
fn an_immediately_invoked_lambda_propagates_errors_and_checks_arity() {
    assert_eq!(error_kind(eval("=LAMBDA(x,x/0)(1)")), ExcelErrorKind::Div);
    assert_eq!(error_kind(eval("=LAMBDA(x,y,x+y)(1)")), ExcelErrorKind::Value);
    assert_eq!(error_kind(eval("=LAMBDA(x,x+1)(1,2)")), ExcelErrorKind::Value);
}
