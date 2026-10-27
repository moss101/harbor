use crate::args::{ArgSchema, CoercionPolicy, ShapeKind};
use formualizer_common::{ExcelError, LiteralValue};
use std::sync::LazyLock;

/// Small epsilon used to detect near-zero denominators in trig/hyperbolic functions.
pub const EPSILON_NEAR_ZERO: f64 = 1e-12;

/// Final non-finite guard for numeric aggregate results (SUM/MIN/MAX/...):
/// Excel never surfaces inf/NaN — an overflowed aggregate is `#NUM\!`, the
/// same parity rule operators already apply via `coercion::sanitize_numeric`.
/// One branch per aggregate CALL (apply to the finished reduction, never per
/// element — arrow kernels stay untouched).
pub fn aggregate_result(n: f64) -> LiteralValue {
    if n.is_finite() {
        LiteralValue::Number(n)
    } else {
        LiteralValue::Error(ExcelError::new_num())
    }
}

/// Coerce a `LiteralValue` to `f64` using Excel semantics.
/// - Number/Int map to f64
/// - Boolean maps to 1.0/0.0
/// - Empty maps to 0.0
/// - Others -> `#VALUE!`
pub fn coerce_num(value: &LiteralValue) -> Result<f64, ExcelError> {
    crate::coercion::to_number_lenient(value)
}

/// Get a single numeric argument, with count and error checks.
pub fn unary_numeric_arg<'a, 'b>(
    args: &'a [crate::traits::ArgumentHandle<'a, 'b>],
) -> Result<f64, ExcelError> {
    if args.len() != 1 {
        return Err(ExcelError::new_value()
            .with_message(format!("Expected 1 argument, got {}", args.len())));
    }
    let v = args[0].value()?.into_literal();
    match v {
        LiteralValue::Error(e) => Err(e),
        other => coerce_num(&other),
    }
}

/// Get two numeric arguments, with count and error checks.
pub fn binary_numeric_args<'a, 'b>(
    args: &'a [crate::traits::ArgumentHandle<'a, 'b>],
) -> Result<(f64, f64), ExcelError> {
    if args.len() != 2 {
        return Err(ExcelError::new_value()
            .with_message(format!("Expected 2 arguments, got {}", args.len())));
    }
    let a = args[0].value()?.into_literal();
    let b = args[1].value()?.into_literal();
    let a_num = match a {
        LiteralValue::Error(e) => return Err(e),
        other => coerce_num(&other)?,
    };
    let b_num = match b {
        LiteralValue::Error(e) => return Err(e),
        other => coerce_num(&other)?,
    };
    Ok((a_num, b_num))
}

fn calc_from_literal<'b>(
    v: LiteralValue,
    date_system: crate::engine::DateSystem,
) -> crate::traits::CalcValue<'b> {
    match v {
        LiteralValue::Array(rows) => crate::traits::CalcValue::Range(
            crate::engine::range_view::RangeView::from_owned_rows(rows, date_system),
        ),
        other => crate::traits::CalcValue::Scalar(other),
    }
}

pub fn unary_numeric_elementwise<'a, 'b, F>(
    args: &'a [crate::traits::ArgumentHandle<'a, 'b>],
    ctx: &dyn crate::traits::FunctionContext<'b>,
    mut f: F,
) -> Result<crate::traits::CalcValue<'b>, ExcelError>
where
    F: FnMut(f64) -> Result<LiteralValue, ExcelError>,
{
    if args.len() != 1 {
        return Err(ExcelError::new_value()
            .with_message(format!("Expected 1 argument, got {}", args.len())));
    }

    let shape = if let Ok(rv) = args[0].range_view() {
        rv.dims()
    } else if let Ok(cv) = args[0].value() {
        match cv.into_literal() {
            LiteralValue::Array(arr) => (arr.len(), arr.first().map(|r| r.len()).unwrap_or(0)),
            _ => (1, 1),
        }
    } else {
        (1, 1)
    };

    if shape != (1, 1) {
        let mut out: Vec<Vec<LiteralValue>> = Vec::with_capacity(shape.0);
        if let Ok(view) = args[0].range_view() {
            view.for_each_row(&mut |row| {
                let mut out_row: Vec<LiteralValue> = Vec::with_capacity(row.len());
                for cell in row.iter() {
                    let num_opt = match cell {
                        LiteralValue::Error(e) => return Err(e.clone()),
                        other => {
                            crate::coercion::to_number_lenient_with_locale(other, &ctx.locale())
                                .ok()
                        }
                    };
                    match num_opt {
                        Some(n) => out_row.push(f(n)?),
                        None => out_row.push(LiteralValue::Error(
                            ExcelError::new_value()
                                .with_message("Element is not coercible to number"),
                        )),
                    }
                }
                out.push(out_row);
                Ok(())
            })?;
        } else {
            let v = args[0].value()?.into_literal();
            let LiteralValue::Array(arr) = v else {
                // Defensive: if shape says array but value isn't, treat as scalar.
                let x = unary_numeric_arg(args)?;
                return Ok(calc_from_literal(f(x)?, ctx.date_system()));
            };

            for row in arr {
                let mut out_row: Vec<LiteralValue> = Vec::with_capacity(row.len());
                for cell in row {
                    let num_opt = match &cell {
                        LiteralValue::Error(e) => return Err(e.clone()),
                        other => {
                            crate::coercion::to_number_lenient_with_locale(other, &ctx.locale())
                                .ok()
                        }
                    };
                    match num_opt {
                        Some(n) => out_row.push(f(n)?),
                        None => out_row.push(LiteralValue::Error(
                            ExcelError::new_value()
                                .with_message("Element is not coercible to number"),
                        )),
                    }
                }
                out.push(out_row);
            }
        }

        return Ok(calc_from_literal(
            LiteralValue::Array(out),
            ctx.date_system(),
        ));
    }

    let x = unary_numeric_arg(args)?;
    Ok(calc_from_literal(f(x)?, ctx.date_system()))
}

pub fn binary_numeric_elementwise<'a, 'b, F>(
    args: &'a [crate::traits::ArgumentHandle<'a, 'b>],
    ctx: &dyn crate::traits::FunctionContext<'b>,
    mut f: F,
) -> Result<crate::traits::CalcValue<'b>, ExcelError>
where
    F: FnMut(f64, f64) -> Result<LiteralValue, ExcelError>,
{
    if args.len() != 2 {
        return Err(ExcelError::new_value()
            .with_message(format!("Expected 2 arguments, got {}", args.len())));
    }

    use crate::broadcast::{broadcast_shape, project_index};

    enum Grid<'b> {
        Range(crate::engine::range_view::RangeView<'b>),
        Array(Vec<Vec<LiteralValue>>),
        Scalar(LiteralValue),
    }

    impl<'b> Grid<'b> {
        fn shape(&self) -> (usize, usize) {
            match self {
                Grid::Range(rv) => rv.dims(),
                Grid::Array(arr) => (arr.len(), arr.first().map(|r| r.len()).unwrap_or(0)),
                Grid::Scalar(_) => (1, 1),
            }
        }

        fn get(&self, r: usize, c: usize) -> LiteralValue {
            match self {
                Grid::Range(rv) => rv.get_cell(r, c),
                Grid::Array(arr) => arr
                    .get(r)
                    .and_then(|row| row.get(c))
                    .cloned()
                    .unwrap_or(LiteralValue::Empty),
                Grid::Scalar(v) => v.clone(),
            }
        }
    }

    fn to_grid<'a, 'b>(ah: &crate::traits::ArgumentHandle<'a, 'b>) -> Result<Grid<'b>, ExcelError> {
        if let Ok(rv) = ah.range_view() {
            return Ok(Grid::Range(rv));
        }
        let v = ah.value()?.into_literal();
        Ok(match v {
            LiteralValue::Array(arr) => Grid::Array(arr),
            other => Grid::Scalar(other),
        })
    }

    let g0 = to_grid(&args[0])?;
    let g1 = to_grid(&args[1])?;
    let s0 = g0.shape();
    let s1 = g1.shape();
    let target = broadcast_shape(&[s0, s1])?;

    if target != (1, 1) {
        let mut out: Vec<Vec<LiteralValue>> = Vec::with_capacity(target.0);
        for r in 0..target.0 {
            let mut out_row = Vec::with_capacity(target.1);
            for c in 0..target.1 {
                let (r0, c0) = project_index((r, c), s0);
                let (r1, c1) = project_index((r, c), s1);
                let lv0 = g0.get(r0, c0);
                let lv1 = g1.get(r1, c1);

                let n0 = match &lv0 {
                    LiteralValue::Error(e) => return Err(e.clone()),
                    other => {
                        crate::coercion::to_number_lenient_with_locale(other, &ctx.locale()).ok()
                    }
                };
                let n1 = match &lv1 {
                    LiteralValue::Error(e) => return Err(e.clone()),
                    other => {
                        crate::coercion::to_number_lenient_with_locale(other, &ctx.locale()).ok()
                    }
                };

                let out_cell = match (n0, n1) {
                    (Some(a), Some(b)) => f(a, b)?,
                    _ => LiteralValue::Error(
                        ExcelError::new_value()
                            .with_message("Elements are not coercible to numbers"),
                    ),
                };
                out_row.push(out_cell);
            }
            out.push(out_row);
        }
        return Ok(calc_from_literal(
            LiteralValue::Array(out),
            ctx.date_system(),
        ));
    }

    let (a, b) = binary_numeric_args(args)?;
    Ok(calc_from_literal(f(a, b)?, ctx.date_system()))
}

/// Forward-looking: clamp numeric result to Excel-friendly finite values.
/// Converts NaN to `#NUM!` and +/-Inf to large finite sentinels if desired.
pub fn sanitize_numeric_result(n: f64) -> Result<f64, ExcelError> {
    crate::coercion::sanitize_numeric(n)
}

/// Forward-looking: try converting text that looks like a number (Excel often parses text numbers).
pub fn coerce_text_to_number_maybe(value: &LiteralValue) -> Option<f64> {
    match value {
        LiteralValue::Text(_) => crate::coercion::to_number_lenient(value).ok(),
        _ => None,
    }
}

/// Forward-looking: common rounding strategy for functions requiring specific rounding.
pub fn round_to_precision(n: f64, digits: i32) -> f64 {
    if digits <= 0 {
        return n.round();
    }
    let factor = 10f64.powi(digits);
    (n * factor).round() / factor
}

pub fn collapse_if_scalar(
    rows: Vec<Vec<LiteralValue>>,
    date_system: crate::engine::DateSystem,
) -> crate::traits::CalcValue<'static> {
    if rows.len() == 1 && rows[0].len() == 1 {
        crate::traits::CalcValue::Scalar(rows[0][0].clone())
    } else {
        crate::traits::CalcValue::Range(crate::engine::range_view::RangeView::from_owned_rows(
            rows,
            date_system,
        ))
    }
}

// ─────────────────────────────── Criteria helpers (shared by *IF* aggregators) ───────────────────────────────

/// Match a cell value against a parsed `CriteriaPredicate` (see `crate::args::CriteriaPredicate`).
///
/// This is the single source of truth for Excel's criteria typing rules; every *IF/*IFS/MAXIFS/
/// MINIFS path (scalar, chunked, and cached Arrow masks) must agree with it. Observed Excel rules
/// (checked against real Excel):
/// - A numeric comparison (`<`, `<=`, `>`, `>=`) is satisfied only by NUMERIC cells (numbers and
///   date/time serials). Blanks, text (even "3") and booleans never satisfy it; in particular a
///   blank is not 0.
/// - `=n` / `n` matches number cells equal to `n` AND text cells that read as the number `n`
///   (the text "3" satisfies `=3`). Booleans and blanks never match a number.
/// - `<>n` is the complement of *numeric-cell equality only*: blanks, text (including the text
///   "3"), booleans and errors all satisfy it. (Excel is asymmetric here: `=3` counts the text "3"
///   but `<>3` does not exclude it.)
/// - Wildcard patterns (`*`, `?`) match TEXT cells only (including empty text for `*`), never
///   numbers, booleans or blanks.
/// - Text equality is case-insensitive; an empty text criterion (`""`, `"="`) matches blank cells
///   and empty text; `<>` + empty text matches every non-blank cell.
pub fn criteria_match(pred: &crate::args::CriteriaPredicate, v: &LiteralValue) -> bool {
    use crate::args::CriteriaPredicate as P;
    match pred {
        P::Eq(t) => criteria_equals(t, v),
        P::Ne(t) => !criteria_ne_excludes(t, v),
        P::Gt(n) => cell_number(v).is_some_and(|x| x > *n),
        P::Ge(n) => cell_number(v).is_some_and(|x| x >= *n),
        P::Lt(n) => cell_number(v).is_some_and(|x| x < *n),
        P::Le(n) => cell_number(v).is_some_and(|x| x <= *n),
        P::TextLike {
            pattern,
            case_insensitive,
        } => text_like_match(pattern, *case_insensitive, v),
        P::IsBlank => matches!(v, LiteralValue::Empty),
        P::IsNumber => crate::coercion::to_number_lenient(v).is_ok(),
        P::IsText => matches!(v, LiteralValue::Text(_)),
        P::IsLogical => matches!(v, LiteralValue::Boolean(_)),
    }
}

/// The numeric value of a cell for criteria purposes: numbers and date/time serials only.
/// Booleans, text, blanks and errors are NOT numeric here (a boolean never equals or compares
/// against a number criterion, and text is never coerced for `<`/`>`).
fn cell_number(v: &LiteralValue) -> Option<f64> {
    match v {
        LiteralValue::Boolean(_) => None,
        other => other.as_serial_number(),
    }
}

/// A text cell that reads as a finite number ("3", " 3.5 ", "1e3"). Blank/whitespace-only text is
/// not a number.
fn text_cell_number(s: &str) -> Option<f64> {
    if s.trim().is_empty() {
        return None;
    }
    crate::locale::Locale::invariant()
        .parse_number_invariant(s)
        .filter(|n| n.is_finite())
}

fn numbers_equal(x: f64, y: f64) -> bool {
    (x - y).abs() < 1e-12
}

/// Does the cell satisfy an equality criterion `t`? (`=t` or a bare `t`.)
fn criteria_equals(t: &LiteralValue, v: &LiteralValue) -> bool {
    match t {
        LiteralValue::Boolean(b) => matches!(v, LiteralValue::Boolean(x) if x == b),
        LiteralValue::Text(s) => match v {
            LiteralValue::Text(c) => c.to_lowercase() == s.to_lowercase(),
            // Excel: an empty text criterion matches blank cells as well as empty text.
            LiteralValue::Empty => s.is_empty(),
            _ => false,
        },
        LiteralValue::Empty => matches!(v, LiteralValue::Empty),
        // Numbers and date/time criteria compare by serial value.
        other => match other.as_serial_number() {
            Some(tn) => match v {
                // The text "3" satisfies `=3` (but see `criteria_ne_excludes`).
                LiteralValue::Text(s) => text_cell_number(s).is_some_and(|n| numbers_equal(n, tn)),
                _ => cell_number(v).is_some_and(|n| numbers_equal(n, tn)),
            },
            None => false,
        },
    }
}

/// For `<>t`: is the cell one that the criterion EXCLUDES (i.e. equal to `t`)?
/// Differs from `criteria_equals` only for numeric `t`, where text that merely reads as the
/// number is NOT excluded (Excel: COUNTIF over 1,2,3,"x",,,"3",TRUE with "<>3" is 7, not 6).
fn criteria_ne_excludes(t: &LiteralValue, v: &LiteralValue) -> bool {
    match t.as_serial_number() {
        Some(tn) if !matches!(t, LiteralValue::Boolean(_)) => {
            cell_number(v).is_some_and(|n| numbers_equal(n, tn))
        }
        _ => criteria_equals(t, v),
    }
}

/// Wildcard criteria only ever match TEXT cells: `"*"` does not match numbers, booleans or
/// blanks, and `"?"` matches one-character text only.
fn text_like_match(pattern: &str, case_insensitive: bool, v: &LiteralValue) -> bool {
    let LiteralValue::Text(s) = v else {
        return false;
    };
    let (pat, text) = if case_insensitive {
        (pattern.to_lowercase(), s.to_lowercase())
    } else {
        (pattern.to_string(), s.clone())
    };

    // Fast-path for anchored patterns without '?' or escape sequences
    if !pat.contains('?') && !pat.contains('~') {
        // Pattern like "text*" - starts with
        if pat.ends_with('*') && !pat[..pat.len() - 1].contains('*') {
            return text.starts_with(&pat[..pat.len() - 1]);
        }
        // Pattern like "*text" - ends with
        if pat.starts_with('*') && !pat[1..].contains('*') {
            return text.ends_with(&pat[1..]);
        }
        // Pattern like "*text*" - contains
        if pat.starts_with('*') && pat.ends_with('*') && !pat[1..pat.len() - 1].contains('*') {
            return text.contains(&pat[1..pat.len() - 1]);
        }
        // Pattern with no wildcards - exact match
        if !pat.contains('*') {
            return text == pat;
        }
    }

    // Fall back to general wildcard matching for complex patterns
    wildcard_match(&pat, &text)
}

/// Excel wildcard matcher: `*` any run, `?` exactly one character, `~` escapes `*`, `?`, `~`.
/// Works on characters (not bytes) so `?` consumes one whole character.
fn wildcard_match(pat: &str, text: &str) -> bool {
    fn helper(p: &[char], t: &[char]) -> bool {
        let Some(&c) = p.first() else {
            return t.is_empty();
        };
        match c {
            '*' => (0..=t.len()).any(|i| helper(&p[1..], &t[i..])),
            '?' => !t.is_empty() && helper(&p[1..], &t[1..]),
            '~' if matches!(p.get(1), Some('*' | '?' | '~')) => {
                t.first() == Some(&p[1]) && helper(&p[2..], &t[1..])
            }
            ch => t.first() == Some(&ch) && helper(&p[1..], &t[1..]),
        }
    }
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    helper(&p, &t)
}

// ─────────────────────────────── ArgSchema presets ───────────────────────────────

/// Single scalar argument of any type.
/// Used by many unary or variadic-any functions (e.g., `LEN`, `TYPE`, simple wrappers).
pub static ARG_ANY_ONE: LazyLock<Vec<ArgSchema>> = LazyLock::new(|| vec![ArgSchema::any()]);

/// Two scalar arguments of any type.
/// Used by generic binary functions (e.g., comparisons, concatenation variants).
pub static ARG_ANY_TWO: LazyLock<Vec<ArgSchema>> =
    LazyLock::new(|| vec![ArgSchema::any(), ArgSchema::any()]);

/// Single numeric scalar argument, with lenient text-to-number coercion.
/// Ideal for elementwise numeric functions (e.g., `SIN`, `COS`, `ABS`).
pub static ARG_NUM_LENIENT_ONE: LazyLock<Vec<ArgSchema>> =
    LazyLock::new(|| vec![{ ArgSchema::number_lenient_scalar() }]);

/// Two numeric scalar arguments, with lenient text-to-number coercion.
/// Suited for binary numeric operations (e.g., `ATAN2`, `POWER`, `LOG(base)`).
pub static ARG_NUM_LENIENT_TWO: LazyLock<Vec<ArgSchema>> = LazyLock::new(|| {
    vec![{ ArgSchema::number_lenient_scalar() }, {
        ArgSchema::number_lenient_scalar()
    }]
});

/// Single range argument, numeric semantics with lenient text-to-number coercion.
/// Best for reductions over ranges (e.g., `SUM`, `AVERAGE`, `COUNT`-like families).
pub static ARG_RANGE_NUM_LENIENT_ONE: LazyLock<Vec<ArgSchema>> = LazyLock::new(|| {
    vec![{
        let mut s = ArgSchema::number_lenient_scalar();
        s.shape = ShapeKind::Range;
        s.coercion = CoercionPolicy::NumberLenientText;
        s
    }]
});

/// True when every range argument of a multi-criteria *IFS call is a plain,
/// fully bounded reference (no whole-column/row or open-ended range, no 3D
/// reference, nothing computed). `count_form` is COUNTIFS (ranges at even
/// positions) versus SUMIFS-style (target first, then range/criteria pairs).
pub(crate) fn ifs_ranges_bounded(args: &[crate::traits::ArgumentHandle<'_, '_>], count_form: bool) -> bool {
    use formualizer_parse::parser::ReferenceType;
    let start = if count_form { 0 } else { 1 };
    let mut idx: Vec<usize> = (start..args.len()).step_by(2).collect();
    if !count_form {
        idx.push(0);
    }
    idx.into_iter().all(|i| match args[i].as_reference() {
        Ok(ReferenceType::Cell { .. }) => true,
        Ok(ReferenceType::Range {
            start_row: Some(_),
            start_col: Some(_),
            end_row: Some(_),
            end_col: Some(_),
            ..
        }) => true,
        _ => false,
    })
}

#[cfg(test)]
mod criteria_match_tests {
    use super::criteria_match;
    use crate::args::parse_criteria;
    use formualizer_common::LiteralValue as V;

    fn m(criterion: &str, cell: &V) -> bool {
        criteria_match(&parse_criteria(&V::Text(criterion.into())).unwrap(), cell)
    }
    fn t(s: &str) -> V {
        V::Text(s.into())
    }

    #[test]
    fn numeric_comparisons_only_match_numeric_cells() {
        // Excel: blanks are not 0, text is not coerced (even "3"), TRUE is not 1.
        for c in ["<9", "<=2", ">=2", ">1", ">-5", "<=0"] {
            for cell in [V::Empty, t("3"), t("x"), V::Boolean(true), V::Boolean(false)] {
                assert!(!m(c, &cell), "{c} must not match {cell:?}");
            }
        }
        assert!(m("<9", &V::Number(3.0)) && m(">1", &V::Int(2)) && m("<=2", &V::Number(2.0)));
        assert!(!m(">1", &V::Number(1.0)));
    }

    #[test]
    fn equality_matches_number_and_numeric_text_but_never_booleans_or_blanks() {
        for c in ["=3", "3"] {
            assert!(m(c, &V::Number(3.0)) && m(c, &V::Int(3)) && m(c, &t("3")), "{c}");
            assert!(m(c, &t(" 3 ")), "numeric text is trimmed");
            for cell in [V::Empty, t(""), t("x"), t("4"), V::Boolean(true), V::Number(4.0)] {
                assert!(!m(c, &cell), "{c} must not match {cell:?}");
            }
        }
        // 1 is not TRUE and 0 is not blank / FALSE.
        assert!(!m("=1", &V::Boolean(true)) && !m("=0", &V::Boolean(false)) && !m("=0", &V::Empty));
        // A boolean criterion matches booleans only.
        assert!(m("TRUE", &V::Boolean(true)) && !m("TRUE", &V::Number(1.0)) && !m("TRUE", &t("TRUE")));
    }

    #[test]
    fn not_equal_numeric_excludes_only_numeric_cells_equal_to_it() {
        assert!(!m("<>3", &V::Number(3.0)));
        // Excel quirk: "<>3" still counts the TEXT "3" (7 of the 8 oracle cells).
        for cell in [V::Empty, t("3"), t("x"), V::Boolean(true), V::Number(4.0)] {
            assert!(m("<>3", &cell), "<>3 must match {cell:?}");
        }
        assert!(m("<>0", &V::Empty), "a blank is not equal to 0, so <>0 matches it");
    }

    #[test]
    fn text_equality_is_case_insensitive_and_empty_text_means_blank() {
        assert!(m("x", &t("X")) && m("X", &t("x")) && !m("x", &t("xy")));
        assert!(!m("x", &V::Number(1.0)) && !m("x", &V::Empty));
        assert!(m("", &V::Empty) && m("=", &V::Empty) && m("", &t("")));
        assert!(!m("", &V::Number(0.0)) && !m("", &t("x")));
        // "<>" = non-blank: numbers, text and booleans match, blanks do not.
        assert!(m("<>", &V::Number(1.0)) && m("<>", &t("x")) && m("<>", &V::Boolean(true)));
        assert!(!m("<>", &V::Empty));
        assert!(m("<>x", &V::Empty) && m("<>x", &V::Number(1.0)) && !m("<>x", &t("X")));
    }

    #[test]
    fn wildcards_match_text_cells_only() {
        assert!(m("*", &t("x")) && m("*", &t("3")) && m("*", &t("")));
        assert!(m("?", &t("x")) && m("?", &t("3")) && !m("?", &t("")) && !m("?", &t("xy")));
        for c in ["*", "?", "1*", "*1", "?1"] {
            for cell in [V::Empty, V::Number(1.0), V::Number(11.0), V::Boolean(true)] {
                assert!(!m(c, &cell), "{c} must not match {cell:?}");
            }
        }
        assert!(m("a*c", &t("ABBC")) && m("a?c", &t("abc")) && !m("a?c", &t("ac")));
        // ~ escapes the wildcard characters; `?` consumes one whole (multi-byte) character.
        assert!(m("a~*", &t("a*")) && !m("a~*", &t("ab")) && m("a~?", &t("a?")));
        assert!(m("?", &t("é")));
    }

    #[test]
    fn non_finite_spellings_are_text_criteria() {
        assert!(m("inf", &t("INF")) && !m("inf", &V::Number(f64::MAX)));
        assert!(m("nan", &t("NaN")));
    }
}
