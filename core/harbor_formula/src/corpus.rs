//! Fixture corpus types and loader.

use std::collections::BTreeMap;

use crate::fixtures::CORPUS_JSON;
use crate::value::{CellError, CellValue};

#[derive(Debug, Clone, PartialEq)]
pub enum FixtureValue {
    Blank,
    Number(f64),
    Text(String),
    Bool(bool),
    Error(String),
}

impl FixtureValue {
    pub fn to_cell_value(&self) -> CellValue {
        match self {
            FixtureValue::Blank => CellValue::Blank,
            FixtureValue::Number(n) => CellValue::Number(*n),
            FixtureValue::Text(s) => CellValue::Text(s.clone()),
            FixtureValue::Bool(b) => CellValue::Bool(*b),
            FixtureValue::Error(code) => {
                CellValue::Error(CellError::from_code(code).unwrap_or(CellError::Value))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct FixtureCase {
    pub id: String,
    pub group: String,
    pub target: String,
    pub dimension: String,
    /// (sheet, row, col, value)
    pub cells: Vec<(String, u32, u32, FixtureValue)>,
    pub formula: String,
    pub expect: FixtureExpectation,
    /// recalc_edit dimension: edits applied after first evaluation.
    pub edits: Vec<FixtureEdit>,
}

#[derive(Debug, Clone)]
pub struct FixtureExpectation {
    pub value: FixtureValue,
    pub tolerance: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct FixtureEdit {
    pub cells: Vec<(String, u32, u32, FixtureValue)>,
    pub expect: FixtureExpectation,
}

#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    #[error("corpus json error: {0}")]
    Json(String),
    #[error("case {0}: {1}")]
    Case(String, String),
}

/// Parse the embedded corpus. Uses serde_json (floats are needed for
/// expected values) with strict structural validation.
pub fn load_corpus() -> Result<Vec<FixtureCase>, CorpusError> {
    let raw: serde_json::Value =
        serde_json::from_str(CORPUS_JSON).map_err(|e| CorpusError::Json(e.to_string()))?;
    let cases = raw
        .get("cases")
        .and_then(|c| c.as_array())
        .ok_or_else(|| CorpusError::Json("missing cases array".into()))?;
    let mut out = Vec::with_capacity(cases.len());
    for c in cases {
        let id = c
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CorpusError::Json("case missing id".into()))?
            .to_string();
        let cells =
            c.get("cells")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|cell| {
                            let a = cell.as_array().ok_or_else(|| {
                                CorpusError::Case(id.clone(), "cell not array".into())
                            })?;
                            let sheet = a.first().and_then(|v| v.as_str()).ok_or_else(|| {
                                CorpusError::Case(id.clone(), "cell sheet".into())
                            })?;
                            let row =
                                a.get(1).and_then(|v| v.as_u64()).ok_or_else(|| {
                                    CorpusError::Case(id.clone(), "cell row".into())
                                })? as u32;
                            let col =
                                a.get(2).and_then(|v| v.as_u64()).ok_or_else(|| {
                                    CorpusError::Case(id.clone(), "cell col".into())
                                })? as u32;
                            let val = a
                                .get(3)
                                .cloned()
                                .unwrap_or(serde_json::Value::Object(Default::default()));
                            FixtureValue::from_serde(&val).ok_or_else(|| {
                                CorpusError::Case(id.clone(), "cell value".into())
                            })?;
                            Ok((
                                sheet.to_string(),
                                row,
                                col,
                                FixtureValue::from_serde(&val).unwrap(),
                            ))
                        })
                        .collect::<Result<Vec<_>, CorpusError>>()
                })
                .unwrap_or_else(|| Ok(Vec::new()))?;
        let formula = c
            .get("formula")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CorpusError::Case(id.clone(), "missing formula".into()))?
            .to_string();
        let expect_raw = c
            .get("expect")
            .cloned()
            .ok_or_else(|| CorpusError::Case(id.clone(), "missing expect".into()))?;
        let expect = FixtureExpectation {
            value: FixtureValue::from_serde(&expect_raw)
                .ok_or_else(|| CorpusError::Case(id.clone(), "bad expect".into()))?,
            tolerance: expect_raw.get("tol").and_then(|v| v.as_f64()),
        };
        let edits = c
            .get("edits")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|e| {
                        let cells = e
                            .get("cells")
                            .and_then(|v| v.as_array())
                            .map(|cs| {
                                cs.iter()
                                    .map(|cell| {
                                        let a = cell.as_array().unwrap();
                                        (
                                            a[0].as_str().unwrap().to_string(),
                                            a[1].as_u64().unwrap() as u32,
                                            a[2].as_u64().unwrap() as u32,
                                            FixtureValue::from_serde(&a[3].clone()).unwrap(),
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let er = e.get("expect").cloned().unwrap();
                        Ok(FixtureEdit {
                            cells,
                            expect: FixtureExpectation {
                                value: FixtureValue::from_serde(&er).ok_or_else(|| {
                                    CorpusError::Case(id.clone(), "bad edit expect".into())
                                })?,
                                tolerance: er.get("tol").and_then(|v| v.as_f64()),
                            },
                        })
                    })
                    .collect::<Result<Vec<_>, CorpusError>>()
            })
            .unwrap_or_else(|| Ok(Vec::new()))?;
        out.push(FixtureCase {
            id: id.clone(),
            group: c
                .get("group")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            target: c
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            dimension: c
                .get("dimension")
                .and_then(|v| v.as_str())
                .unwrap_or("values")
                .to_string(),
            cells,
            formula,
            expect,
            edits,
        });
    }
    Ok(out)
}

impl FixtureValue {
    fn from_serde(v: &serde_json::Value) -> Option<FixtureValue> {
        let obj = v.as_object()?;
        if obj.is_empty() {
            return Some(FixtureValue::Blank);
        }
        if let Some(serde_json::Value::Number(n)) = obj.get("n") {
            return n.as_f64().map(FixtureValue::Number);
        }
        if let Some(serde_json::Value::String(s)) = obj.get("t") {
            return Some(FixtureValue::Text(s.clone()));
        }
        if let Some(serde_json::Value::Bool(b)) = obj.get("b") {
            return Some(FixtureValue::Bool(*b));
        }
        if let Some(serde_json::Value::String(e)) = obj.get("e") {
            return Some(FixtureValue::Error(e.clone()));
        }
        None
    }
}

/// Unique targets in the corpus (for coverage accounting).
pub fn corpus_targets(cases: &[FixtureCase]) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in cases {
        map.entry(c.target.clone()).or_default().push(c.id.clone());
    }
    map
}
