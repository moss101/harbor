//! Chart part (`ppt/charts/chartN.xml`, root `c:chartSpace`) -> scene
//! [`ChartData`].
//!
//! The slide resolver (`slides.rs`) finds the `p:graphicFrame` that points
//! at a chart part and hands the part's XML to [`parse_chart`]. Anything
//! Harbor's painter cannot draw faithfully is an `Err("unsupported chart:
//! ...")` so the slide lists it under `unsupported` instead of drawing
//! wrong data; any other `Err` means the part itself is corrupt or over a
//! safety cap.
//!
//! Decisions worth knowing:
//!
//! * A numeric cache point that is absent (`c:pt` with that `idx` missing,
//!   or an unparseable / non-finite value) is a *gap* in the source.
//!   [`ChartData`] cannot represent gaps, so it becomes `0.0`.
//! * Colours: `a:srgbClr`, `a:schemeClr` (resolved through the caller's
//!   `scheme_color`), `a:sysClr`, a few `a:prstClr` names, `a:scrgbClr` and
//!   `a:hslClr`, with `lumMod/lumOff/satMod/satOff/hue*/tint/shade/comp/
//!   inv/gray/alpha*` transforms. [`ChartSeries`] has no alpha, so a
//!   translucent colour is composited over white.
//! * `percent_stacked` implies `stacked` (both are set).
//! * `c:overlap` is read by nobody: [`ChartData`] has no field for it
//!   (stacked groupings already imply a full overlap).
//! * Scatter charts carry their X values in `ChartSeries::x`; their
//!   `categories` stay empty.
//! * Bounded: input over [`MAX_INPUT_BYTES`], more than [`MAX_SERIES`]
//!   series, or more than [`MAX_POINTS`] points per series / category list
//!   is an `Err`. Everything else is linear in the input size.

use crate::scene::{ChartData, ChartSeries};
use roxmltree::{Document, Node, ParsingOptions};

/// Deepest element nesting accepted. Real chart parts stay under ~25;
/// `roxmltree` recurses per nesting level with no limit of its own, so a
/// hostile part could overflow the stack of a small worker thread.
pub const MAX_XML_DEPTH: usize = 64;
/// Largest chart part accepted.
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
/// Most series a chart may carry.
pub const MAX_SERIES: usize = 64;
/// Most points per series (and per category list).
pub const MAX_POINTS: usize = 10_000;

/// Office's default accents, used only when `scheme_color` cannot supply
/// `accentN` (so a missing theme still gets distinct series colours).
const FALLBACK_ACCENTS: [&str; 6] = [
    "#4472C4", "#ED7D31", "#A5A5A5", "#FFC000", "#5B9BD5", "#70AD47",
];

type SchemeFn<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Parse a `c:chartSpace` document into scene chart data.
///
/// `scheme_color("accent1" | "tx1" | "bg1" | "dk2" | ...)` must return the
/// theme colour as `"#RRGGBB"` (already mapped through the slide's colour
/// map) or `None` when unknown (`phClr`, a missing theme, ...).
pub fn parse_chart(
    xml: &str,
    scheme_color: &dyn Fn(&str) -> Option<String>,
) -> Result<ChartData, String> {
    if xml.len() > MAX_INPUT_BYTES {
        return Err(format!(
            "chart: part is {} bytes, over the {MAX_INPUT_BYTES} byte cap",
            xml.len()
        ));
    }
    check_xml_depth(xml, MAX_XML_DEPTH)?;
    let opts = ParsingOptions {
        allow_dtd: false,
        nodes_limit: 4_000_000,
        ..ParsingOptions::default()
    };
    let doc = Document::parse_with_options(xml, opts)
        .map_err(|e| format!("chart: malformed XML: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "chartSpace" {
        return Err("chart: root element is not chartSpace".into());
    }
    if root
        .tag_name()
        .namespace()
        .is_some_and(|ns| ns.contains("chartex"))
    {
        return Err("unsupported chart: extended chart (chartex)".into());
    }
    let chart = kid(root, "chart").ok_or("chart: missing c:chart")?;
    let plot_area = kid(chart, "plotArea").ok_or("chart: missing c:plotArea")?;

    // Exactly one plot. Several (a combo, or primary + secondary axis
    // groups) are refused: drawing only the first would be wrong data.
    let plots: Vec<Node> = plot_area
        .children()
        .filter(|c| c.is_element() && c.tag_name().name().ends_with("Chart"))
        .collect();
    let plot = match plots.as_slice() {
        [] => return Err("chart: plot area has no chart".into()),
        [one] => *one,
        many => {
            let names: Vec<String> = many.iter().map(|p| friendly(p.tag_name().name())).collect();
            return Err(format!("unsupported chart: combo of {}", names.join(" + ")));
        }
    };
    let plot_name = plot.tag_name().name();
    if let Some(base) = plot_name.strip_suffix("3DChart") {
        return Err(format!(
            "unsupported chart: 3-D {}",
            friendly(&format!("{base}Chart"))
        ));
    }

    let mut data = ChartData {
        kind: match plot_name {
            "barChart" => {
                let dir = kid(plot, "barDir").and_then(val).unwrap_or("col");
                if dir == "bar" {
                    "bar"
                } else {
                    "column"
                }
            }
            "lineChart" => "line",
            "areaChart" => "area",
            "pieChart" => "pie",
            "doughnutChart" => "doughnut",
            "scatterChart" => "scatter",
            other => return Err(format!("unsupported chart: {}", friendly(other))),
        }
        .to_string(),
        ..ChartData::default()
    };

    // Grouping (bar, line, area).
    match kid(plot, "grouping").and_then(val) {
        Some("stacked") => data.stacked = true,
        Some("percentStacked") => {
            data.stacked = true;
            data.percent_stacked = true;
        }
        _ => {}
    }
    if data.kind == "pie" || data.kind == "doughnut" {
        data.stacked = false;
        data.percent_stacked = false;
    }

    data.title = chart_title(chart);
    data.legend = match kid(chart, "legend") {
        None => "none".into(),
        Some(l) => match kid(l, "legendPos").and_then(val) {
            Some("t") => "t",
            Some("b") => "b",
            Some("l") => "l",
            _ => "r", // `r`, `tr` and the schema default
        }
        .into(),
    };

    // Axes: gridlines, value-axis scaling.
    let axes: Vec<Node> = plot_area
        .children()
        .filter(|c| {
            c.is_element() && matches!(c.tag_name().name(), "valAx" | "catAx" | "dateAx" | "serAx")
        })
        .collect();
    data.show_gridlines = axes.iter().any(|a| kid(*a, "majorGridlines").is_some());
    if data.kind != "pie" && data.kind != "doughnut" {
        let vals: Vec<Node> = axes
            .iter()
            .copied()
            .filter(|a| a.tag_name().name() == "valAx")
            .collect();
        // Scatter has two value axes; the vertical one (left/right) is Y.
        let value_axis = if data.kind == "scatter" {
            vals.iter()
                .copied()
                .find(|a| matches!(kid(*a, "axPos").and_then(val), Some("l") | Some("r")))
                .or_else(|| vals.get(1).copied())
                .or_else(|| vals.first().copied())
        } else {
            vals.first().copied()
        };
        if let Some(scaling) = value_axis.and_then(|a| kid(a, "scaling")) {
            let min = kid(scaling, "min").and_then(val).and_then(parse_f);
            let max = kid(scaling, "max").and_then(val).and_then(parse_f);
            let ok = match (min, max) {
                (Some(lo), Some(hi)) => lo < hi,
                _ => true,
            };
            if ok && !data.percent_stacked {
                data.axis_min = min;
                data.axis_max = max;
            }
        }
    }

    // Text colour: chart-wide text properties, then axes, then legend.
    let mut tx_nodes: Vec<Node> = Vec::new();
    tx_nodes.extend(kid(root, "txPr"));
    for a in &axes {
        tx_nodes.extend(kid(*a, "txPr"));
    }
    tx_nodes.extend(kid(chart, "legend").and_then(|l| kid(l, "txPr")));
    data.text_color = tx_nodes
        .iter()
        .find_map(|tx| {
            tx.descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "defRPr")
                .find_map(|d| match fill_in(d, scheme_color) {
                    Fill::Color(c) => Some(c),
                    _ => None,
                })
        })
        .or_else(|| scheme_color("tx1").and_then(|c| parse_hex(&c)).map(to_hex))
        .unwrap_or_else(|| "#000000".into());

    // Plot-level settings.
    let is_bar = data.kind == "bar" || data.kind == "column";
    if is_bar {
        let gw = kid(plot, "gapWidth")
            .and_then(val)
            .and_then(parse_f)
            .unwrap_or(150.0);
        data.gap_width = gw.clamp(0.0, 500.0) / 100.0;
    }
    if data.kind == "doughnut" {
        let hs = kid(plot, "holeSize")
            .and_then(val)
            .and_then(parse_f)
            .unwrap_or(50.0);
        data.hole = hs.clamp(1.0, 90.0) / 100.0;
    }

    // Series.
    let mut raw: Vec<(u64, usize, Node)> = Vec::new();
    for (pos, s) in kids(plot, "ser").into_iter().enumerate() {
        if pos >= MAX_SERIES {
            return Err(format!("chart: more than {MAX_SERIES} series"));
        }
        let order = kid(s, "order")
            .and_then(val)
            .and_then(|v| v.trim().parse::<u64>().ok());
        raw.push((order.unwrap_or(pos as u64), pos, s));
    }
    if raw.is_empty() {
        return Err("chart: plot has no series".into());
    }
    raw.sort_by_key(|(order, pos, _)| (*order, *pos));

    let plot_labels = kid(plot, "dLbls").map(dlbls_on);
    let plot_vary = kid(plot, "varyColors").map(|n| bool_attr(n));
    let line_like = matches!(data.kind.as_str(), "line" | "scatter");
    let radial = matches!(data.kind.as_str(), "pie" | "doughnut");
    let vary = match plot_vary {
        Some(v) => v,
        None => radial, // absent: pie/doughnut vary, the rest do not
    };

    let mut categories: Vec<String> = Vec::new();
    let mut smooth = kid(plot, "scatterStyle")
        .and_then(val)
        .is_some_and(|s| s.starts_with("smooth"));
    let mut labels_on = false;

    for (n, (_, pos, s)) in raw.iter().enumerate() {
        let colour_idx = kid(*s, "idx")
            .and_then(val)
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(*pos);
        let name = series_name(*s).unwrap_or_else(|| format!("Series {}", n + 1));
        let default = default_color(colour_idx, scheme_color);
        let color =
            series_color(*s, line_like, radial, scheme_color).unwrap_or_else(|| default.clone());

        let mut series = ChartSeries {
            name,
            color: color.clone(),
            ..ChartSeries::default()
        };

        if data.kind == "scatter" {
            let ys = match kid(*s, "yVal") {
                Some(h) => numbers(h)?,
                None => Vec::new(),
            };
            let xs = match kid(*s, "xVal") {
                Some(h) => numbers_or_none(h)?,
                None => None,
            };
            let (xs, ys) = match xs {
                Some(xs) => {
                    let len = xs.len().min(ys.len());
                    (xs[..len].to_vec(), ys[..len].to_vec())
                }
                None => ((1..=ys.len()).map(|i| i as f64).collect(), ys),
            };
            series.x = Some(xs);
            series.values = ys;
            if kid(*s, "smooth").is_some_and(bool_attr) {
                smooth = true;
            }
        } else {
            series.values = match kid(*s, "val") {
                Some(h) => numbers(h)?,
                None => Vec::new(),
            };
            if categories.is_empty() {
                if let Some(h) = kid(*s, "cat") {
                    categories = read_categories(h)?;
                }
            }
            if data.kind == "line" && kid(*s, "smooth").is_some_and(bool_attr) {
                smooth = true;
            }
            // Per-point colours.
            let overrides = point_overrides(*s, line_like, radial, scheme_color);
            if vary || !overrides.is_empty() {
                let n_pts = series.values.len();
                series.point_colors = (0..n_pts)
                    .map(|k| {
                        overrides
                            .iter()
                            .find(|(i, _)| *i == k)
                            .map(|(_, c)| c.clone())
                            .unwrap_or_else(|| {
                                if vary {
                                    default_color(k, scheme_color)
                                } else {
                                    color.clone()
                                }
                            })
                    })
                    .collect();
            }
        }

        let eff = kid(*s, "dLbls")
            .map(dlbls_on)
            .or(plot_labels)
            .unwrap_or(false);
        labels_on |= eff;
        data.series.push(series);
    }

    if data.kind != "scatter" {
        let longest = data
            .series
            .iter()
            .map(|s| s.values.len())
            .max()
            .unwrap_or(0);
        // Category labels the file did not supply are their 1-based index.
        for i in categories.len()..longest {
            categories.push((i + 1).to_string());
        }
        data.categories = categories;
    }
    data.smooth = smooth && matches!(data.kind.as_str(), "line" | "scatter");
    data.data_labels = labels_on;
    Ok(data)
}

// ---------------------------------------------------------------------------
// XML helpers

pub use harbor_artifacts::check_xml_depth;

fn kid<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn kids<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Vec<Node<'a, 'i>> {
    n.children()
        .filter(|c| c.is_element() && c.tag_name().name() == name)
        .collect()
}

fn val<'a>(n: Node<'a, '_>) -> Option<&'a str> {
    n.attribute("val")
}

/// A `CT_Boolean`: present without `val` means true.
fn bool_attr(n: Node) -> bool {
    match n.attribute("val") {
        None => true,
        Some(v) => matches!(v.trim(), "1" | "true" | "on"),
    }
}

fn parse_f(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn friendly(plot_name: &str) -> String {
    match plot_name {
        "ofPieChart" => "bar-of-pie".into(),
        other => other.strip_suffix("Chart").unwrap_or(other).to_string(),
    }
}

/// Chart title text from `c:chart/c:title`; `None` when absent, deleted or
/// automatic (an automatic title is the series name, which the painter
/// would have to guess).
fn chart_title(chart: Node) -> Option<String> {
    let title = kid(chart, "title")?;
    let tx = kid(title, "tx")?;
    let text = if let Some(rich) = kid(tx, "rich") {
        kids(rich, "p")
            .iter()
            .map(|p| {
                p.descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "t")
                    .filter_map(|t| t.text())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let cache = kid(tx, "strRef").and_then(|r| kid(r, "strCache"))?;
        read_dense(cache).ok()?.into_iter().flatten().next()?
    };
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn series_name(ser: Node) -> Option<String> {
    let tx = kid(ser, "tx")?;
    if let Some(cache) = kid(tx, "strRef").and_then(|r| kid(r, "strCache")) {
        if let Ok(d) = read_dense(cache) {
            return d
                .into_iter()
                .flatten()
                .next()
                .filter(|s| !s.trim().is_empty());
        }
        return None;
    }
    kid(tx, "v")
        .and_then(|v| v.text())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}

/// `c:dLbls` switches value / percentage labels on?
fn dlbls_on(d: Node) -> bool {
    if kid(d, "delete").is_some_and(bool_attr) {
        return false;
    }
    kid(d, "showVal").is_some_and(bool_attr) || kid(d, "showPercent").is_some_and(bool_attr)
}

// ---------------------------------------------------------------------------
// Caches

/// Dense cache: index -> text, `None` where no `c:pt` has that index.
fn read_dense(cache: Node) -> Result<Vec<Option<String>>, String> {
    let declared = match kid(cache, "ptCount").and_then(val) {
        Some(v) => match v.trim().parse::<usize>() {
            Ok(n) if n > MAX_POINTS => {
                return Err(format!(
                    "chart: {n} points in a series, over the {MAX_POINTS} cap"
                ))
            }
            Ok(n) => n,
            Err(_) => 0,
        },
        None => 0,
    };
    let mut out: Vec<Option<String>> = vec![None; declared];
    for pt in kids(cache, "pt") {
        let Some(idx) = pt
            .attribute("idx")
            .and_then(|s| s.trim().parse::<usize>().ok())
        else {
            continue;
        };
        if idx >= MAX_POINTS {
            return Err(format!(
                "chart: point index {idx} over the {MAX_POINTS} cap"
            ));
        }
        if idx >= out.len() {
            out.resize(idx + 1, None);
        }
        let text = kid(pt, "v")
            .and_then(|v| v.text())
            .unwrap_or("")
            .to_string();
        out[idx] = Some(text);
    }
    Ok(out)
}

/// The node that holds `c:ptCount` / `c:pt` under a `c:val`, `c:cat`,
/// `c:xVal`, `c:yVal` holder: the cache of a reference, or a literal.
fn cache_of<'a, 'i>(holder: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    for child in holder.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "numRef" => return kid(child, "numCache"),
            "strRef" => return kid(child, "strCache"),
            "numLit" | "strLit" => return Some(child),
            _ => {}
        }
    }
    None
}

fn numbers(holder: Node) -> Result<Vec<f64>, String> {
    Ok(numbers_or_none(holder)?.unwrap_or_default())
}

/// `None` when the holder carries a non-numeric (string) cache, so the
/// caller can fall back to 1..n for scatter X values.
fn numbers_or_none(holder: Node) -> Result<Option<Vec<f64>>, String> {
    let Some(cache) = cache_of(holder) else {
        return Ok(Some(Vec::new()));
    };
    let is_str = cache.tag_name().name() == "strCache" || cache.tag_name().name() == "strLit";
    let dense = read_dense(cache)?;
    let parsed: Vec<f64> = dense
        .iter()
        .map(|p| p.as_deref().and_then(parse_f).unwrap_or(0.0))
        .collect();
    if is_str && dense.iter().flatten().any(|s| parse_f(s).is_none()) {
        return Ok(None);
    }
    Ok(Some(parsed))
}

fn read_categories(holder: Node) -> Result<Vec<String>, String> {
    // Multi-level: levels run innermost first; outer labels are written
    // once at the start of their group and apply until the next one.
    if let Some(cache) = kid(holder, "multiLvlStrRef").and_then(|r| kid(r, "multiLvlStrCache")) {
        let count = kid(cache, "ptCount")
            .and_then(val)
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if count > MAX_POINTS {
            return Err(format!(
                "chart: {count} categories, over the {MAX_POINTS} cap"
            ));
        }
        let mut levels: Vec<Vec<Option<String>>> = Vec::new();
        for lvl in kids(cache, "lvl") {
            levels.push(read_dense(lvl)?);
        }
        let n = levels.iter().map(Vec::len).max().unwrap_or(0).max(count);
        let mut carried: Vec<Option<String>> = vec![None; levels.len()];
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let mut parts: Vec<String> = Vec::new();
            for (l, level) in levels.iter().enumerate().rev() {
                let here = level.get(i).cloned().flatten();
                let label = if l == 0 {
                    here
                } else {
                    if here.is_some() {
                        carried[l] = here;
                    }
                    carried[l].clone()
                };
                if let Some(label) = label.filter(|s| !s.is_empty()) {
                    parts.push(label);
                }
            }
            out.push(parts.join(" / "));
        }
        return Ok(out);
    }
    let Some(cache) = cache_of(holder) else {
        return Ok(Vec::new());
    };
    Ok(read_dense(cache)?
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect())
}

// ---------------------------------------------------------------------------
// Series colours

fn default_color(i: usize, sc: SchemeFn) -> String {
    let accent = i % 6;
    let round = (i / 6) % 4;
    let base = sc(&format!("accent{}", accent + 1))
        .and_then(|c| parse_hex(&c))
        .or_else(|| parse_hex(FALLBACK_ACCENTS[accent]))
        .unwrap_or((0.0, 0.0, 0.0));
    let (m, o) = match round {
        1 => (0.6, 0.0),
        2 => (0.8, 0.2),
        3 => (0.4, 0.6),
        _ => (1.0, 0.0),
    };
    let rgb = if round == 0 {
        base
    } else {
        lum_mod_off(base, m, o)
    };
    to_hex(rgb)
}

/// The colour a series is drawn in, from `c:spPr` (and `c:marker` for line
/// and scatter). Order: line charts prefer the stroke, bars and areas the
/// fill; a pie never falls back to its stroke (that is the slice separator).
fn series_color(ser: Node, line_like: bool, radial: bool, sc: SchemeFn) -> Option<String> {
    color_from(ser, line_like, radial, sc)
}

fn color_from(holder: Node, line_like: bool, radial: bool, sc: SchemeFn) -> Option<String> {
    let sp = kid(holder, "spPr");
    let fill = sp.map(|s| fill_in(s, sc));
    let ln = sp.and_then(|s| kid(s, "ln")).map(|l| fill_in(l, sc));
    let marker = kid(holder, "marker")
        .and_then(|m| kid(m, "spPr"))
        .map(|s| fill_in(s, sc));
    let order: Vec<Option<Fill>> = if line_like {
        vec![ln, marker, fill]
    } else if radial {
        vec![fill]
    } else {
        vec![fill, ln]
    };
    order.into_iter().flatten().find_map(|f| match f {
        Fill::Color(c) => Some(c),
        _ => None,
    })
}

/// `(point index, colour)` from `c:dPt` overrides.
fn point_overrides(ser: Node, line_like: bool, radial: bool, sc: SchemeFn) -> Vec<(usize, String)> {
    kids(ser, "dPt")
        .into_iter()
        .filter_map(|d| {
            let idx = kid(d, "idx")
                .and_then(val)
                .and_then(|v| v.trim().parse::<usize>().ok())?;
            if idx >= MAX_POINTS {
                return None;
            }
            Some((idx, color_from(d, line_like, radial, sc)?))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Colour: resolution and transforms

enum Fill {
    Color(String),
    Empty,
    Unknown,
}

/// The first fill child of `holder` (`a:solidFill`, `a:gradFill` -> first
/// stop, `a:pattFill` -> foreground, `a:noFill`).
fn fill_in(holder: Node, sc: SchemeFn) -> Fill {
    for c in holder.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "solidFill" => return color_in(c, sc),
            "gradFill" => {
                let stop = kid(c, "gsLst").and_then(|l| kid(l, "gs"));
                return stop.map_or(Fill::Unknown, |s| color_in(s, sc));
            }
            "pattFill" => {
                return kid(c, "fgClr").map_or(Fill::Unknown, |s| color_in(s, sc));
            }
            "noFill" => return Fill::Empty,
            "blipFill" => return Fill::Unknown,
            _ => {}
        }
    }
    Fill::Unknown
}

fn color_in(parent: Node, sc: SchemeFn) -> Fill {
    for c in parent.children().filter(|c| c.is_element()) {
        if let Some((rgb, a)) = resolve_color(c, sc) {
            // ChartSeries has no alpha: composite over white.
            let flat = (
                rgb.0 * a + 255.0 * (1.0 - a),
                rgb.1 * a + 255.0 * (1.0 - a),
                rgb.2 * a + 255.0 * (1.0 - a),
            );
            return Fill::Color(to_hex(flat));
        }
    }
    Fill::Unknown
}

type Rgb = (f64, f64, f64);

/// Resolve one colour element (`a:srgbClr`, `a:schemeClr`, ...) with its
/// transforms to `(rgb 0..255, alpha 0..1)`.
fn resolve_color(node: Node, sc: SchemeFn) -> Option<(Rgb, f64)> {
    let v = node.attribute("val");
    let mut rgb: Rgb = match node.tag_name().name() {
        "srgbClr" => parse_hex(v?)?,
        "schemeClr" => parse_hex(&sc(v?)?)?,
        "sysClr" => match node.attribute("lastClr").and_then(parse_hex) {
            Some(c) => c,
            None => match v? {
                "window" => (255.0, 255.0, 255.0),
                "windowText" => (0.0, 0.0, 0.0),
                _ => return None,
            },
        },
        "prstClr" => preset_color(v?)?,
        "scrgbClr" => {
            let ch = |k: &str| {
                node.attribute(k)
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .map(|p| (p / 100_000.0).clamp(0.0, 1.0) * 255.0)
            };
            (ch("r")?, ch("g")?, ch("b")?)
        }
        "hslClr" => {
            let h = node.attribute("hue")?.trim().parse::<f64>().ok()? / 60_000.0;
            let s = node.attribute("sat")?.trim().parse::<f64>().ok()? / 100_000.0;
            let l = node.attribute("lum")?.trim().parse::<f64>().ok()? / 100_000.0;
            hsl_to_rgb(h.rem_euclid(360.0), s.clamp(0.0, 1.0), l.clamp(0.0, 1.0))
        }
        _ => return None,
    };
    let mut alpha = 1.0f64;
    for t in node.children().filter(|c| c.is_element()) {
        let amount = t.attribute("val").and_then(pct);
        match (t.tag_name().name(), amount) {
            ("alpha", Some(p)) => alpha = p,
            ("alphaMod", Some(p)) => alpha *= p,
            ("alphaOff", Some(p)) => alpha += p,
            ("lumMod", Some(p)) => rgb = hsl_op(rgb, |h, s, l| (h, s, l * p)),
            ("lumOff", Some(p)) => rgb = hsl_op(rgb, |h, s, l| (h, s, l + p)),
            ("lum", Some(p)) => rgb = hsl_op(rgb, |h, s, _| (h, s, p)),
            ("satMod", Some(p)) => rgb = hsl_op(rgb, |h, s, l| (h, s * p, l)),
            ("satOff", Some(p)) => rgb = hsl_op(rgb, |h, s, l| (h, s + p, l)),
            ("sat", Some(p)) => rgb = hsl_op(rgb, |h, _, l| (h, p, l)),
            ("hueOff", _) | ("hueMod", _) | ("hue", _) => {
                let raw = t
                    .attribute("val")
                    .and_then(|s| s.trim().parse::<f64>().ok());
                if let Some(raw) = raw {
                    let name = t.tag_name().name();
                    rgb = hsl_op(rgb, |h, s, l| match name {
                        "hueOff" => (h + raw / 60_000.0, s, l),
                        "hue" => (raw / 60_000.0, s, l),
                        _ => (h * raw / 100_000.0, s, l),
                    });
                }
            }
            ("tint", Some(p)) => {
                rgb = (
                    rgb.0 * p + 255.0 * (1.0 - p),
                    rgb.1 * p + 255.0 * (1.0 - p),
                    rgb.2 * p + 255.0 * (1.0 - p),
                )
            }
            ("shade", Some(p)) => rgb = (rgb.0 * p, rgb.1 * p, rgb.2 * p),
            ("comp", _) => rgb = hsl_op(rgb, |h, s, l| (h + 180.0, s, l)),
            ("inv", _) => rgb = (255.0 - rgb.0, 255.0 - rgb.1, 255.0 - rgb.2),
            ("gray", _) => {
                let y = 0.299 * rgb.0 + 0.587 * rgb.1 + 0.114 * rgb.2;
                rgb = (y, y, y);
            }
            _ => {}
        }
    }
    Some((rgb, alpha.clamp(0.0, 1.0)))
}

/// `"50000"` -> 0.5, `"50%"` -> 0.5.
fn pct(s: &str) -> Option<f64> {
    let s = s.trim();
    match s.strip_suffix('%') {
        Some(p) => p.trim().parse::<f64>().ok().map(|v| v / 100.0),
        None => s.parse::<f64>().ok().map(|v| v / 100_000.0),
    }
    .filter(|v| v.is_finite())
}

fn preset_color(name: &str) -> Option<Rgb> {
    let hex = match name {
        "black" => "000000",
        "white" => "FFFFFF",
        "red" => "FF0000",
        "green" => "008000",
        "blue" => "0000FF",
        "yellow" => "FFFF00",
        "orange" => "FFA500",
        "gray" | "grey" => "808080",
        "darkGray" | "dkGray" | "darkGrey" | "dkGrey" => "A9A9A9",
        "lightGray" | "ltGray" | "lightGrey" | "ltGrey" => "D3D3D3",
        "purple" => "800080",
        "cyan" => "00FFFF",
        "magenta" => "FF00FF",
        "navy" => "000080",
        "maroon" => "800000",
        _ => return None,
    };
    parse_hex(hex)
}

/// `"RRGGBB"` or `"#RRGGBB"` -> components.
fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let ch = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok().map(f64::from);
    Some((ch(0)?, ch(2)?, ch(4)?))
}

fn to_hex(c: Rgb) -> String {
    let q = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", q(c.0), q(c.1), q(c.2))
}

fn lum_mod_off(rgb: Rgb, m: f64, o: f64) -> Rgb {
    hsl_op(rgb, |h, s, l| (h, s, l * m + o))
}

fn hsl_op(rgb: Rgb, f: impl FnOnce(f64, f64, f64) -> (f64, f64, f64)) -> Rgb {
    let (h, s, l) = rgb_to_hsl(rgb);
    let (h, s, l) = f(h, s, l);
    hsl_to_rgb(h.rem_euclid(360.0), s.clamp(0.0, 1.0), l.clamp(0.0, 1.0))
}

/// Hue in degrees, saturation and lightness in `0..=1`.
fn rgb_to_hsl((r, g, b): Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-12 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if (max - r).abs() < 1e-12 {
        ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-12 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    ((r1 + m) * 255.0, (g1 + m) * 255.0, (b1 + m) * 255.0)
}

// ---------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(name: &str) -> Option<String> {
        Some(
            match name {
                "accent1" => "#4472C4",
                "accent2" => "#ED7D31",
                "accent3" => "#A5A5A5",
                "accent4" => "#FFC000",
                "accent5" => "#5B9BD5",
                "accent6" => "#70AD47",
                "tx1" | "dk1" => "#000000",
                "bg1" | "lt1" => "#FFFFFF",
                "tx2" | "dk2" => "#44546A",
                "bg2" | "lt2" => "#E7E6E6",
                _ => return None,
            }
            .to_string(),
        )
    }

    fn parse(xml: &str) -> Result<ChartData, String> {
        parse_chart(xml, &theme)
    }

    const NS: &str = r#"xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

    fn space(chart_inner: &str) -> String {
        format!(r#"<c:chartSpace {NS}><c:chart>{chart_inner}</c:chart></c:chartSpace>"#)
    }

    fn plot(inner: &str) -> String {
        space(&format!("<c:plotArea><c:layout/>{inner}</c:plotArea>"))
    }

    fn str_cache(vals: &[&str]) -> String {
        let pts: String = vals
            .iter()
            .enumerate()
            .map(|(i, v)| format!(r#"<c:pt idx="{i}"><c:v>{v}</c:v></c:pt>"#))
            .collect();
        format!(r#"<c:ptCount val="{}"/>{pts}"#, vals.len())
    }

    fn ser(i: usize, name: &str, cats: &[&str], vals: &[f64], extra: &str) -> String {
        let pts: String = vals
            .iter()
            .enumerate()
            .map(|(j, v)| format!(r#"<c:pt idx="{j}"><c:v>{v}</c:v></c:pt>"#))
            .collect();
        format!(
            r#"<c:ser><c:idx val="{i}"/><c:order val="{i}"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache>{n}</c:strCache></c:strRef></c:tx>{extra}<c:cat><c:strRef><c:f>x</c:f><c:strCache>{c}</c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>y</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="{len}"/>{pts}</c:numCache></c:numRef></c:val></c:ser>"#,
            n = str_cache(&[name]),
            c = str_cache(cats),
            len = vals.len(),
        )
    }

    fn fill(c: &str) -> String {
        format!(r#"<c:spPr><a:solidFill><a:srgbClr val="{c}"/></a:solidFill></c:spPr>"#)
    }

    #[test]
    fn column_clustered() {
        let xml = plot(&format!(
            r#"<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/>{}{}<c:gapWidth val="80"/><c:overlap val="-20"/><c:axId val="1"/><c:axId val="2"/></c:barChart>
            <c:catAx><c:axId val="1"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/><c:max val="100"/><c:min val="0"/></c:scaling><c:majorGridlines/></c:valAx>"#,
            ser(
                0,
                "Rev",
                &["Q1", "Q2", "Q3"],
                &[1.0, 2.5, 3.0],
                &fill("112233")
            ),
            ser(1, "Cost", &["Q1", "Q2", "Q3"], &[0.5, 1.0, 1.5], ""),
        ));
        let d = parse(&xml).unwrap();
        assert_eq!(d.kind, "column");
        assert_eq!(d.categories, ["Q1", "Q2", "Q3"]);
        assert_eq!(d.series.len(), 2);
        assert_eq!(d.series[0].name, "Rev");
        assert_eq!(d.series[0].values, [1.0, 2.5, 3.0]);
        assert_eq!(d.series[0].color, "#112233");
        assert_eq!(d.series[1].color, "#ED7D31"); // idx 1 -> accent2
        assert!(d.series[0].point_colors.is_empty());
        assert!(!d.stacked && !d.percent_stacked);
        assert_eq!(d.gap_width, 0.8);
        assert_eq!((d.axis_min, d.axis_max), (Some(0.0), Some(100.0)));
        assert!(d.show_gridlines);
        assert_eq!(d.legend, "none");
        assert!(!d.data_labels);
        assert_eq!(d.text_color, "#000000");
        assert_eq!(d.title, None);
    }

    #[test]
    fn horizontal_bar_stacked_and_percent() {
        let mk = |g: &str| {
            plot(&format!(
                r#"<c:barChart><c:barDir val="bar"/><c:grouping val="{g}"/>{}</c:barChart>"#,
                ser(0, "A", &["x"], &[1.0], "")
            ))
        };
        let d = parse(&mk("stacked")).unwrap();
        assert_eq!(d.kind, "bar");
        assert!(d.stacked && !d.percent_stacked);
        assert_eq!(d.gap_width, 1.5); // schema default 150
        let d = parse(&mk("percentStacked")).unwrap();
        assert!(d.stacked && d.percent_stacked);
    }

    #[test]
    fn line_with_smooth_marker_colour_and_labels() {
        let line = r#"<c:spPr><a:ln w="28575"><a:solidFill><a:schemeClr val="accent2"><a:lumMod val="75000"/></a:schemeClr></a:solidFill></a:ln></c:spPr>"#;
        let xml = plot(&format!(
            r#"<c:lineChart><c:grouping val="standard"/><c:varyColors val="0"/>{}<c:dLbls><c:showLegendKey val="0"/><c:showVal val="1"/><c:showCatName val="0"/></c:dLbls><c:marker val="1"/><c:axId val="1"/><c:axId val="2"/></c:lineChart>"#,
            ser(0, "Trend", &["a", "b"], &[3.0, 4.0], line)
                .replace("</c:ser>", r#"<c:smooth val="1"/></c:ser>"#)
        ));
        let d = parse(&xml).unwrap();
        assert_eq!(d.kind, "line");
        assert!(d.smooth);
        assert!(d.data_labels);
        // accent2 ED7D31 at 75% lightness
        let (r, g, b) = hsl_to_rgb_round(parse_hex("#ED7D31").unwrap(), 0.75);
        assert_eq!(d.series[0].color, format!("#{r:02X}{g:02X}{b:02X}"));
    }

    fn hsl_to_rgb_round(c: Rgb, lum_mod: f64) -> (u8, u8, u8) {
        let o = lum_mod_off(c, lum_mod, 0.0);
        (o.0.round() as u8, o.1.round() as u8, o.2.round() as u8)
    }

    #[test]
    fn area_stacked_with_alpha_flattened() {
        let sp = r#"<c:spPr><a:solidFill><a:srgbClr val="000000"><a:alpha val="50000"/></a:srgbClr></a:solidFill></c:spPr>"#;
        let xml = plot(&format!(
            r#"<c:areaChart><c:grouping val="stacked"/><c:varyColors val="0"/>{}<c:axId val="1"/><c:axId val="2"/></c:areaChart>"#,
            ser(0, "A", &["x", "y"], &[1.0, 2.0], sp)
        ));
        let d = parse(&xml).unwrap();
        assert_eq!(d.kind, "area");
        assert!(d.stacked);
        assert_eq!(d.series[0].color, "#808080"); // 50% black over white
    }

    #[test]
    fn pie_per_point_colours_and_default_vary() {
        let dpt = format!(
            r#"<c:dPt><c:idx val="1"/><c:bubble3D val="0"/>{}</c:dPt>"#,
            fill("FF0000")
        );
        let xml = plot(&format!(
            r#"<c:pieChart><c:varyColors val="1"/>{}<c:firstSliceAng val="0"/></c:pieChart>"#,
            ser(0, "Share", &["a", "b", "c"], &[50.0, 30.0, 20.0], &dpt)
        ));
        let d = parse(&xml).unwrap();
        assert_eq!(d.kind, "pie");
        assert_eq!(d.series[0].point_colors, ["#4472C4", "#FF0000", "#A5A5A5"]);
        assert_eq!(d.hole, 0.0);
        // varyColors absent -> pie still varies
        let xml = plot(&format!(
            r#"<c:pieChart>{}</c:pieChart>"#,
            ser(0, "S", &["a", "b"], &[1.0, 2.0], "")
        ));
        assert_eq!(parse(&xml).unwrap().series[0].point_colors.len(), 2);
    }

    #[test]
    fn doughnut_hole_and_labels_percent() {
        let xml = plot(&format!(
            r#"<c:doughnutChart><c:varyColors val="1"/>{}<c:dLbls><c:showVal val="0"/><c:showPercent val="1"/></c:dLbls><c:firstSliceAng val="0"/><c:holeSize val="65"/></c:doughnutChart>"#,
            ser(0, "S", &["a", "b"], &[1.0, 2.0], "")
        ));
        let d = parse(&xml).unwrap();
        assert_eq!(d.kind, "doughnut");
        assert_eq!(d.hole, 0.65);
        assert!(d.data_labels);
    }

    #[test]
    fn scatter_xy() {
        let xml = plot(
            r#"<c:scatterChart><c:scatterStyle val="smoothMarker"/><c:varyColors val="0"/>
            <c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>pts</c:v></c:tx>
            <c:spPr><a:ln w="19050"><a:solidFill><a:srgbClr val="AA0000"/></a:solidFill></a:ln></c:spPr>
            <c:xVal><c:numLit><c:ptCount val="3"/><c:pt idx="0"><c:v>1.5</c:v></c:pt><c:pt idx="1"><c:v>2.5</c:v></c:pt><c:pt idx="2"><c:v>4</c:v></c:pt></c:numLit></c:xVal>
            <c:yVal><c:numRef><c:f>y</c:f><c:numCache><c:ptCount val="3"/><c:pt idx="0"><c:v>10</c:v></c:pt><c:pt idx="1"><c:v>20</c:v></c:pt><c:pt idx="2"><c:v>15</c:v></c:pt></c:numCache></c:numRef></c:yVal></c:ser>
            <c:axId val="1"/><c:axId val="2"/></c:scatterChart>
            <c:valAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/><c:min val="-5"/></c:scaling><c:axPos val="b"/></c:valAx>
            <c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/><c:max val="50"/><c:min val="0"/></c:scaling><c:axPos val="l"/><c:majorGridlines/></c:valAx>"#,
        );
        let d = parse(&xml).unwrap();
        assert_eq!(d.kind, "scatter");
        assert!(d.smooth);
        assert_eq!(d.series[0].name, "pts");
        assert_eq!(d.series[0].x.as_deref(), Some(&[1.5, 2.5, 4.0][..]));
        assert_eq!(d.series[0].values, [10.0, 20.0, 15.0]);
        assert_eq!(d.series[0].color, "#AA0000");
        assert!(d.categories.is_empty());
        // Y axis scaling only
        assert_eq!((d.axis_min, d.axis_max), (Some(0.0), Some(50.0)));
    }

    #[test]
    fn scatter_string_x_falls_back_to_index() {
        let xml = plot(
            r#"<c:scatterChart><c:ser><c:idx val="0"/><c:order val="0"/>
            <c:xVal><c:strRef><c:f>x</c:f><c:strCache><c:ptCount val="2"/><c:pt idx="0"><c:v>Q1</c:v></c:pt><c:pt idx="1"><c:v>Q2</c:v></c:pt></c:strCache></c:strRef></c:xVal>
            <c:yVal><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt></c:numLit></c:yVal></c:ser></c:scatterChart>"#,
        );
        let d = parse(&xml).unwrap();
        assert_eq!(d.series[0].x.as_deref(), Some(&[1.0, 2.0][..]));
    }

    #[test]
    fn title_legend_and_text_colour() {
        let xml = format!(
            r#"<c:chartSpace {NS}><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Revenue</a:t></a:r></a:p><a:p><a:r><a:t>by quarter</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title>
            <c:plotArea>{}</c:plotArea><c:legend><c:legendPos val="t"/><c:overlay val="0"/></c:legend></c:chart>
            <c:txPr><a:bodyPr/><a:p><a:pPr><a:defRPr sz="1000"><a:solidFill><a:schemeClr val="tx2"><a:lumMod val="50000"/></a:schemeClr></a:solidFill></a:defRPr></a:pPr></a:p></c:txPr></c:chartSpace>"#,
            r#"<c:barChart><c:barDir val="col"/>"#.to_string()
                + &ser(0, "A", &["x"], &[1.0], "")
                + "</c:barChart>"
        );
        let d = parse(&xml).unwrap();
        assert_eq!(d.title.as_deref(), Some("Revenue\nby quarter"));
        assert_eq!(d.legend, "t");
        let expected = to_hex(lum_mod_off(parse_hex("#44546A").unwrap(), 0.5, 0.0));
        assert_eq!(d.text_color, expected);
    }

    #[test]
    fn legend_positions() {
        for (pos, want) in [("b", "b"), ("l", "l"), ("r", "r"), ("tr", "r"), ("t", "t")] {
            let xml = format!(
                r#"<c:chartSpace {NS}><c:chart><c:plotArea><c:lineChart>{}</c:lineChart></c:plotArea><c:legend><c:legendPos val="{pos}"/></c:legend></c:chart></c:chartSpace>"#,
                ser(0, "A", &["x"], &[1.0], "")
            );
            assert_eq!(parse(&xml).unwrap().legend, want);
        }
    }

    #[test]
    fn default_text_colour_is_tx1() {
        let xml = plot(&format!(
            "<c:lineChart>{}</c:lineChart>",
            ser(0, "A", &["x"], &[1.0], "")
        ));
        let d = parse_chart(&xml, &|n| (n == "tx1").then(|| "#222222".to_string())).unwrap();
        assert_eq!(d.text_color, "#222222");
        // no theme at all: black, and the default accent cycle still works
        let d = parse_chart(&xml, &|_| None).unwrap();
        assert_eq!(d.text_color, "#000000");
        assert_eq!(d.series[0].color, "#4472C4");
    }

    #[test]
    fn default_colour_cycle_beyond_six() {
        let sers: String = (0..8)
            .map(|i| ser(i, &format!("s{i}"), &["x"], &[1.0], ""))
            .collect();
        let d = parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{sers}</c:barChart>"
        )))
        .unwrap();
        assert_eq!(d.series[0].color, "#4472C4");
        assert_eq!(d.series[5].color, "#70AD47");
        assert_ne!(d.series[6].color, d.series[0].color);
        let cols: std::collections::HashSet<_> = d.series.iter().map(|s| s.color.clone()).collect();
        assert_eq!(cols.len(), 8);
    }

    #[test]
    fn gradient_takes_first_stop_and_pattern_fg() {
        let sp = r#"<c:spPr><a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="102030"/></a:gs><a:gs pos="100000"><a:srgbClr val="FFFFFF"/></a:gs></a:gsLst></a:gradFill></c:spPr>"#;
        let d = parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{}</c:barChart>",
            ser(0, "A", &["x"], &[1.0], sp)
        )))
        .unwrap();
        assert_eq!(d.series[0].color, "#102030");
        let sp = r#"<c:spPr><a:pattFill prst="ltUpDiag"><a:fgClr><a:srgbClr val="334455"/></a:fgClr><a:bgClr><a:srgbClr val="FFFFFF"/></a:bgClr></a:pattFill></c:spPr>"#;
        let d = parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{}</c:barChart>",
            ser(0, "A", &["x"], &[1.0], sp)
        )))
        .unwrap();
        assert_eq!(d.series[0].color, "#334455");
    }

    #[test]
    fn colour_transforms() {
        let one = |inner: &str| {
            let doc_xml = format!(r#"<a:solidFill xmlns:a="x">{inner}</a:solidFill>"#);
            let doc = Document::parse(&doc_xml).unwrap();
            match color_in(doc.root_element(), &theme) {
                Fill::Color(c) => c,
                _ => "none".into(),
            }
        };
        assert_eq!(one(r#"<a:srgbClr val="ff8000"/>"#), "#FF8000");
        assert_eq!(
            one(r#"<a:srgbClr val="FF0000"><a:shade val="50000"/></a:srgbClr>"#),
            "#800000"
        );
        assert_eq!(
            one(r#"<a:srgbClr val="000000"><a:tint val="50000"/></a:srgbClr>"#),
            "#808080"
        );
        assert_eq!(
            one(r#"<a:srgbClr val="FF0000"><a:inv/></a:srgbClr>"#),
            "#00FFFF"
        );
        assert_eq!(
            one(r#"<a:srgbClr val="808080"><a:alpha val="0"/></a:srgbClr>"#),
            "#FFFFFF"
        );
        assert_eq!(
            one(r#"<a:sysClr val="windowText" lastClr="111111"/>"#),
            "#111111"
        );
        assert_eq!(one(r#"<a:sysClr val="window"/>"#), "#FFFFFF");
        assert_eq!(one(r#"<a:prstClr val="red"/>"#), "#FF0000");
        assert_eq!(one(r#"<a:scrgbClr r="100000" g="0" b="0"/>"#), "#FF0000");
        assert_eq!(
            one(r#"<a:hslClr hue="0" sat="100000" lum="50000"/>"#),
            "#FF0000"
        );
        assert_eq!(
            one(r#"<a:schemeClr val="bg1"><a:lumMod val="95000"/></a:schemeClr>"#),
            "#F2F2F2"
        );
        assert_eq!(
            one(r#"<a:schemeClr val="bg1"><a:lumMod val="50%"/></a:schemeClr>"#),
            "#808080"
        );
        // unknown scheme / malformed values do not resolve
        assert_eq!(one(r#"<a:schemeClr val="phClr"/>"#), "none");
        assert_eq!(one(r#"<a:srgbClr val="ZZZZZZ"/>"#), "none");
        assert_eq!(one(r#"<a:srgbClr val="12345"/>"#), "none");
    }

    #[test]
    fn vary_colors_bars_and_dpt_overrides() {
        let dpt = format!(r#"<c:dPt><c:idx val="2"/>{}</c:dPt>"#, fill("010203"));
        let d = parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/><c:varyColors val=\"0\"/>{}</c:barChart>",
            ser(0, "A", &["a", "b", "c"], &[1.0, 2.0, 3.0], &dpt)
        )))
        .unwrap();
        assert_eq!(d.series[0].point_colors, ["#4472C4", "#4472C4", "#010203"]);
        let d = parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/><c:varyColors val=\"1\"/>{}</c:barChart>",
            ser(0, "A", &["a", "b"], &[1.0, 2.0], "")
        )))
        .unwrap();
        assert_eq!(d.series[0].point_colors, ["#4472C4", "#ED7D31"]);
    }

    #[test]
    fn caches_missing_points_literals_and_multilevel() {
        // missing idx 1 -> 0.0; ptCount 4 pads; literal tx; numeric categories
        let xml = plot(
            r#"<c:barChart><c:barDir val="col"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>Lit</c:v></c:tx>
            <c:cat><c:numRef><c:f>c</c:f><c:numCache><c:ptCount val="4"/><c:pt idx="0"><c:v>2020</c:v></c:pt><c:pt idx="1"><c:v>2021</c:v></c:pt><c:pt idx="2"><c:v>2022</c:v></c:pt><c:pt idx="3"><c:v>2023</c:v></c:pt></c:numCache></c:numRef></c:cat>
            <c:val><c:numRef><c:f>v</c:f><c:numCache><c:ptCount val="4"/><c:pt idx="0"><c:v>5</c:v></c:pt><c:pt idx="2"><c:v>NaN</c:v></c:pt><c:pt idx="3"><c:v>inf</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser></c:barChart>"#,
        );
        let d = parse(&xml).unwrap();
        assert_eq!(d.series[0].name, "Lit");
        assert_eq!(d.categories, ["2020", "2021", "2022", "2023"]);
        assert_eq!(d.series[0].values, [5.0, 0.0, 0.0, 0.0]);

        let xml = plot(
            r#"<c:barChart><c:barDir val="col"/><c:ser><c:idx val="0"/><c:order val="0"/>
            <c:cat><c:multiLvlStrRef><c:f>c</c:f><c:multiLvlStrCache><c:ptCount val="4"/>
              <c:lvl><c:pt idx="0"><c:v>Q1</c:v></c:pt><c:pt idx="1"><c:v>Q2</c:v></c:pt><c:pt idx="2"><c:v>Q1</c:v></c:pt><c:pt idx="3"><c:v>Q2</c:v></c:pt></c:lvl>
              <c:lvl><c:pt idx="0"><c:v>2023</c:v></c:pt><c:pt idx="2"><c:v>2024</c:v></c:pt></c:lvl>
            </c:multiLvlStrCache></c:multiLvlStrRef></c:cat>
            <c:val><c:numLit><c:ptCount val="4"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt><c:pt idx="2"><c:v>3</c:v></c:pt><c:pt idx="3"><c:v>4</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart>"#,
        );
        let d = parse(&xml).unwrap();
        assert_eq!(
            d.categories,
            ["2023 / Q1", "2023 / Q2", "2024 / Q1", "2024 / Q2"]
        );
        assert_eq!(d.series[0].name, "Series 1");
    }

    #[test]
    fn series_sorted_by_order_and_idx_colour() {
        let a = ser(0, "first", &["x"], &[1.0], "")
            .replace(r#"<c:order val="0"/>"#, r#"<c:order val="1"/>"#);
        let b = ser(1, "second", &["x"], &[2.0], "")
            .replace(r#"<c:order val="1"/>"#, r#"<c:order val="0"/>"#);
        let d = parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{a}{b}</c:barChart>"
        )))
        .unwrap();
        assert_eq!(d.series[0].name, "second");
        assert_eq!(d.series[0].color, "#ED7D31"); // colour follows c:idx, not position
    }

    #[test]
    fn data_labels_series_level_overrides_and_delete() {
        let on = r#"<c:dLbls><c:showVal val="1"/></c:dLbls>"#;
        let d = parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{}</c:barChart>",
            ser(0, "A", &["x"], &[1.0], on)
        )))
        .unwrap();
        assert!(d.data_labels);
        let d = parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{}<c:dLbls><c:showVal val=\"1\"/></c:dLbls></c:barChart>",
            ser(0, "A", &["x"], &[1.0], "").replace("<c:cat>", r#"<c:dLbls><c:delete val="1"/></c:dLbls><c:cat>"#)
        )))
        .unwrap();
        assert!(!d.data_labels);
    }

    #[test]
    fn unsupported_kinds_are_named() {
        for (el, want) in [
            ("bar3DChart", "unsupported chart: 3-D bar"),
            ("pie3DChart", "unsupported chart: 3-D pie"),
            ("radarChart", "unsupported chart: radar"),
            ("bubbleChart", "unsupported chart: bubble"),
            ("stockChart", "unsupported chart: stock"),
            ("surfaceChart", "unsupported chart: surface"),
            ("ofPieChart", "unsupported chart: bar-of-pie"),
        ] {
            let xml = plot(&format!(
                "<c:{el}>{}</c:{el}>",
                ser(0, "A", &["x"], &[1.0], "")
            ));
            assert_eq!(parse(&xml).unwrap_err(), want, "{el}");
        }
        let combo = plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{}</c:barChart><c:lineChart>{}</c:lineChart>",
            ser(0, "A", &["x"], &[1.0], ""),
            ser(1, "B", &["x"], &[1.0], "")
        ));
        assert_eq!(
            parse(&combo).unwrap_err(),
            "unsupported chart: combo of bar + line"
        );
        let cx = r#"<cx:chartSpace xmlns:cx="http://schemas.microsoft.com/office/drawing/2014/chartex"><cx:chart/></cx:chartSpace>"#;
        assert!(parse(cx).unwrap_err().starts_with("unsupported chart:"));
    }

    #[test]
    fn caps_series_and_points() {
        let many: String = (0..=MAX_SERIES)
            .map(|i| ser(i, "s", &["x"], &[1.0], ""))
            .collect();
        let err = parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{many}</c:barChart>"
        )))
        .unwrap_err();
        assert!(err.contains("series"), "{err}");
        let exactly: String = (0..MAX_SERIES)
            .map(|i| ser(i, "s", &["x"], &[1.0], ""))
            .collect();
        assert!(parse(&plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{exactly}</c:barChart>"
        )))
        .is_ok());

        let big = r#"<c:barChart><c:barDir val="col"/><c:ser><c:val><c:numLit><c:ptCount val="4000000000"/></c:numLit></c:val></c:ser></c:barChart>"#;
        assert!(parse(&plot(big)).unwrap_err().contains("cap"));
        let far = r#"<c:barChart><c:barDir val="col"/><c:ser><c:val><c:numLit><c:pt idx="99999999"><c:v>1</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart>"#;
        assert!(parse(&plot(far)).unwrap_err().contains("cap"));
        // exactly at the cap is fine
        let at = format!(
            r#"<c:barChart><c:barDir val="col"/><c:ser><c:val><c:numLit><c:ptCount val="{MAX_POINTS}"/></c:numLit></c:val></c:ser></c:barChart>"#
        );
        assert_eq!(
            parse(&plot(&at)).unwrap().series[0].values.len(),
            MAX_POINTS
        );

        let huge = format!(
            "<c:chartSpace {NS}>{}</c:chartSpace>",
            " ".repeat(MAX_INPUT_BYTES)
        );
        assert!(parse(&huge).unwrap_err().contains("cap"));
    }

    #[test]
    fn malformed_inputs_never_panic() {
        let good = plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{}</c:barChart>",
            ser(0, "A", &["x", "y"], &[1.0, 2.0], &fill("123456"))
        ));
        assert!(parse(&good).is_ok());
        // every truncation point of a valid part is an Err or a sane value
        let mut boundaries: Vec<usize> = good.char_indices().map(|(i, _)| i).collect();
        boundaries.push(good.len());
        for cut in boundaries {
            let _ = parse(&good[..cut]);
        }
        for bad in [
            "",
            "not xml",
            "<c:chartSpace/>",
            r#"<c:chartSpace xmlns:c="x"><c:chart/></c:chartSpace>"#,
            r#"<c:chartSpace xmlns:c="x"><c:chart><c:plotArea/></c:chart></c:chartSpace>"#,
            r#"<c:chartSpace xmlns:c="x"><c:chart><c:plotArea><c:barChart/></c:plotArea></c:chart></c:chartSpace>"#,
            r#"<!DOCTYPE c [<!ENTITY a "b">]><c:chartSpace xmlns:c="x"/>"#,
            "<other/>",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        // plot with an empty series, no caches, junk attributes
        let junk = plot(
            r#"<c:pieChart><c:varyColors val="maybe"/><c:ser><c:idx val="-3"/><c:order val="x"/><c:tx><c:strRef/></c:tx><c:cat/><c:val><c:numRef><c:f>x</c:f></c:numRef></c:val><c:dPt><c:idx val="abc"/></c:dPt></c:ser><c:holeSize val="zz"/></c:pieChart>"#,
        );
        let d = parse(&junk).unwrap();
        assert_eq!(d.series[0].name, "Series 1");
        assert!(d.series[0].values.is_empty());
        // deep nesting is refused before the (recursive) XML parser runs
        let deep = format!("{}{}", "<a>".repeat(100_000), "</a>".repeat(100_000));
        assert!(parse(&deep).unwrap_err().contains("nesting"));
        let unclosed = "<a>".repeat(100_000);
        assert!(parse(&unclosed).is_err());
    }

    #[test]
    fn depth_scan_understands_xml_syntax() {
        // '<' inside comments, CDATA, PIs and quoted attributes is not a tag
        let tricky = r#"<?xml version="1.0"?><!-- <a><a><a><a> --><r x="<<<<" y='<<<<'><![CDATA[<a><a><a><a>]]><e/><e/></r>"#;
        assert!(check_xml_depth(tricky, 2).is_ok());
        assert!(check_xml_depth("<a><b><c/></b></a>", 2).is_ok());
        assert!(check_xml_depth("<a><b><c></c></b></a>", 2).is_err());
        assert!(check_xml_depth("<a", 2).is_ok());
    }

    #[test]
    fn hostile_text_is_data() {
        let xml = plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{}</c:barChart>",
            ser(
                0,
                "a &lt;b&gt; &amp; \u{202E}RTL שלום",
                &["&lt;x&gt;"],
                &[1.0],
                ""
            )
        ));
        let d = parse(&xml).unwrap();
        assert_eq!(d.series[0].name, "a <b> & \u{202E}RTL שלום");
        assert_eq!(d.categories, ["<x>"]);
    }

    #[test]
    fn axis_scaling_rejects_inverted_range() {
        let xml = plot(&format!(
            "<c:barChart><c:barDir val=\"col\"/>{}</c:barChart><c:valAx><c:scaling><c:max val=\"1\"/><c:min val=\"5\"/></c:scaling></c:valAx>",
            ser(0, "A", &["x"], &[1.0], "")
        ));
        let d = parse(&xml).unwrap();
        assert_eq!((d.axis_min, d.axis_max), (None, None));
    }
}
