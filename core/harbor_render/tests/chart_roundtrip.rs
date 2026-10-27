//! Round trips between Harbor's chart writers (`harbor_artifacts`) and the
//! scene chart reader (`harbor_render::scene_chart`).

use harbor_artifacts::chart_xml::{
    lint_chart_xml_order, styled_chart_space_xml_variant, ChartLegend, ChartStyle, ChartVariant,
};
use harbor_artifacts::pptx::{chart_space_xml, ChartKind};
use harbor_artifacts::styled_chart_space_xml;
use harbor_render::scene_chart::parse_chart;

fn theme(name: &str) -> Option<String> {
    Some(
        match name {
            "accent1" => "#4472C4",
            "accent2" => "#ED7D31",
            "accent3" => "#A5A5A5",
            "accent4" => "#FFC000",
            "accent5" => "#5B9BD5",
            "accent6" => "#70AD47",
            "tx1" => "#000000",
            "bg1" => "#FFFFFF",
            _ => return None,
        }
        .to_string(),
    )
}

fn cats() -> Vec<String> {
    ["Q1", "Q2", "Q3", "Q4"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn data() -> Vec<(String, Vec<f64>)> {
    vec![
        ("Revenue".into(), vec![10.0, 12.5, 9.0, 15.25]),
        ("Cost".into(), vec![6.0, 7.0, 6.5, 8.0]),
        ("Profit".into(), vec![4.0, -5.5, 2.5, 7.0]),
    ]
}

const ACCENTS: [&str; 6] = [
    "#4472C4", "#ED7D31", "#A5A5A5", "#FFC000", "#5B9BD5", "#70AD47",
];

#[test]
fn styled_bar_round_trips_everything() {
    let st = ChartStyle {
        data_labels: true,
        legend: ChartLegend::Right,
        gap_width: 120,
        ..ChartStyle::default()
    };
    let xml = styled_chart_space_xml(ChartKind::Bar, "Revenue by quarter", &cats(), &data(), &st);
    let d = parse_chart(&xml, &theme).unwrap();
    assert_eq!(d.kind, "column");
    assert_eq!(d.title.as_deref(), Some("Revenue by quarter"));
    assert_eq!(d.categories, cats());
    assert_eq!(d.series.len(), 3);
    for (i, (name, vals)) in data().iter().enumerate() {
        assert_eq!(&d.series[i].name, name);
        assert_eq!(&d.series[i].values, vals);
        assert_eq!(
            d.series[i].color,
            ACCENTS[i],
            "series {i} follows accent{}",
            i + 1
        );
        assert!(d.series[i].point_colors.is_empty());
        assert_eq!(d.series[i].x, None);
    }
    assert_eq!(d.legend, "r");
    assert!(d.data_labels);
    assert_eq!(d.gap_width, 1.2);
    assert!(d.show_gridlines);
    assert!(!d.stacked && !d.percent_stacked && !d.smooth);
    assert_eq!((d.axis_min, d.axis_max), (None, None));
    // tx1 (black) at lumMod 65% / lumOff 35% -> #595959
    assert_eq!(d.text_color, "#595959");
}

#[test]
fn legend_gridlines_and_labels_toggle_back() {
    for (l, want) in [
        (ChartLegend::None, "none"),
        (ChartLegend::Top, "t"),
        (ChartLegend::Bottom, "b"),
        (ChartLegend::Right, "r"),
    ] {
        let st = ChartStyle {
            legend: l,
            data_labels: false,
            show_gridlines: false,
            ..ChartStyle::default()
        };
        let xml = styled_chart_space_xml(ChartKind::Line, "", &cats(), &data(), &st);
        let d = parse_chart(&xml, &theme).unwrap();
        assert_eq!(d.legend, want);
        assert!(!d.data_labels);
        assert!(!d.show_gridlines);
        assert_eq!(d.title, None);
    }
}

#[test]
fn every_variant_round_trips_structure() {
    let st = ChartStyle {
        data_labels: true,
        smooth: true,
        ..ChartStyle::default()
    };
    let cases = [
        (
            ChartKind::Bar,
            ChartVariant::Standard,
            "column",
            false,
            false,
        ),
        (
            ChartKind::Bar,
            ChartVariant::Horizontal,
            "bar",
            false,
            false,
        ),
        (ChartKind::Bar, ChartVariant::Stacked, "column", true, false),
        (
            ChartKind::Bar,
            ChartVariant::PercentStacked,
            "column",
            true,
            true,
        ),
        (
            ChartKind::Line,
            ChartVariant::Standard,
            "line",
            false,
            false,
        ),
        (ChartKind::Line, ChartVariant::Stacked, "line", true, false),
        (ChartKind::Line, ChartVariant::Area, "area", false, false),
        (
            ChartKind::Line,
            ChartVariant::StackedArea,
            "area",
            true,
            false,
        ),
        (ChartKind::Pie, ChartVariant::Standard, "pie", false, false),
        (
            ChartKind::Pie,
            ChartVariant::Doughnut,
            "doughnut",
            false,
            false,
        ),
        (
            ChartKind::Scatter,
            ChartVariant::Standard,
            "scatter",
            false,
            false,
        ),
    ];
    for (kind, variant, want_kind, stacked, percent) in cases {
        let xml = styled_chart_space_xml_variant(kind, variant, "T", &cats(), &data(), &st);
        lint_chart_xml_order(&xml).unwrap();
        let d = parse_chart(&xml, &theme).unwrap_or_else(|e| panic!("{kind:?}/{variant:?}: {e}"));
        assert_eq!(d.kind, want_kind, "{kind:?}/{variant:?}");
        assert_eq!(
            (d.stacked, d.percent_stacked),
            (stacked, percent),
            "{kind:?}/{variant:?}"
        );
        assert!(d.data_labels, "{kind:?}/{variant:?}");
        let expect_series = if kind == ChartKind::Pie && variant == ChartVariant::Standard {
            1
        } else {
            3
        };
        assert_eq!(d.series.len(), expect_series, "{kind:?}/{variant:?}");
        for (i, s) in d.series.iter().enumerate() {
            assert_eq!(s.name, data()[i].0);
            assert_eq!(s.values, data()[i].1);
        }
        match want_kind {
            "scatter" => {
                assert!(d.categories.is_empty());
                // categories are not numeric -> 1-based positions
                assert_eq!(d.series[0].x.as_deref(), Some(&[1.0, 2.0, 3.0, 4.0][..]));
                assert!(d.smooth);
            }
            "line" => assert!(d.smooth),
            _ => assert_eq!(d.categories, cats()),
        }
        if want_kind == "doughnut" {
            assert_eq!(d.hole, 0.6);
        }
        if want_kind == "pie" || want_kind == "doughnut" {
            // slice k is accent k+1, cycling past six
            assert_eq!(d.series[0].point_colors.len(), 4);
            assert_eq!(d.series[0].point_colors, ACCENTS[..4]);
            assert!(!d.show_gridlines);
        }
    }
}

#[test]
fn numeric_categories_become_scatter_x() {
    let cats: Vec<String> = ["0.5", "1", "2.25"].iter().map(|s| s.to_string()).collect();
    let series = vec![("y".to_string(), vec![3.0, 1.0, 2.0])];
    let xml = styled_chart_space_xml(
        ChartKind::Scatter,
        "",
        &cats,
        &series,
        &ChartStyle::default(),
    );
    let d = parse_chart(&xml, &theme).unwrap();
    assert_eq!(d.series[0].x.as_deref(), Some(&[0.5, 1.0, 2.25][..]));
    assert_eq!(d.series[0].values, [3.0, 1.0, 2.0]);
}

#[test]
fn series_accents_and_cycling_survive() {
    let st = ChartStyle {
        series_accents: vec![5, 2],
        ..ChartStyle::default()
    };
    let xml = styled_chart_space_xml(ChartKind::Line, "", &cats(), &data(), &st);
    let d = parse_chart(&xml, &theme).unwrap();
    assert_eq!(d.series[0].color, ACCENTS[4]);
    assert_eq!(d.series[1].color, ACCENTS[1]);
    // third series wraps to the first accent, varied (lumMod 60%)
    assert_ne!(d.series[2].color, ACCENTS[4]);
    // enough series that every colour must be distinct
    let many: Vec<_> = (0..13).map(|i| (format!("s{i}"), vec![i as f64])).collect();
    let xml = styled_chart_space_xml(
        ChartKind::Bar,
        "",
        &["x".to_string()],
        &many,
        &ChartStyle::default(),
    );
    let d = parse_chart(&xml, &theme).unwrap();
    let set: std::collections::HashSet<_> = d.series.iter().map(|s| s.color.clone()).collect();
    assert_eq!(
        set.len(),
        13,
        "{:?}",
        d.series.iter().map(|s| &s.color).collect::<Vec<_>>()
    );
}

#[test]
fn theme_switch_recolours_without_rewriting_the_chart() {
    let xml = styled_chart_space_xml(ChartKind::Bar, "", &cats(), &data(), &ChartStyle::default());
    let dark = |n: &str| -> Option<String> {
        Some(
            match n {
                "accent1" => "#112233",
                "tx1" => "#EEEEEE",
                _ => return theme(n),
            }
            .to_string(),
        )
    };
    let a = parse_chart(&xml, &theme).unwrap();
    let b = parse_chart(&xml, &dark).unwrap();
    assert_eq!(a.series[0].color, "#4472C4");
    assert_eq!(b.series[0].color, "#112233");
    assert_ne!(a.text_color, b.text_color);
    // a lighter-than-black tx1 stays light after the 65/35 lightening
    assert!(b.text_color.starts_with("#E") || b.text_color.starts_with("#F"));
}

#[test]
fn hostile_text_round_trips_as_data() {
    let title = "Tom & \"Jerry\" <script> \u{0}\u{1} שלום";
    let cats = vec!["<a>".to_string(), "b&c".to_string()];
    let series = vec![("x </c:v><evil/> ]]> عربى".to_string(), vec![1.0, 2.0])];
    for (kind, variant) in [
        (ChartKind::Bar, ChartVariant::Standard),
        (ChartKind::Line, ChartVariant::Area),
        (ChartKind::Pie, ChartVariant::Doughnut),
    ] {
        let xml = styled_chart_space_xml_variant(
            kind,
            variant,
            title,
            &cats,
            &series,
            &ChartStyle::default(),
        );
        let d = parse_chart(&xml, &theme).unwrap();
        assert_eq!(d.title.as_deref(), Some("Tom & \"Jerry\" <script>  שלום"));
        assert_eq!(d.categories, cats);
        assert_eq!(d.series[0].name, "x </c:v><evil/> ]]> عربى");
    }
}

#[test]
fn deterministic_and_reparsable() {
    for (kind, variant) in [
        (ChartKind::Bar, ChartVariant::Stacked),
        (ChartKind::Pie, ChartVariant::Standard),
    ] {
        let st = ChartStyle::default();
        let a = styled_chart_space_xml_variant(kind, variant, "t", &cats(), &data(), &st);
        let b = styled_chart_space_xml_variant(kind, variant, "t", &cats(), &data(), &st);
        assert_eq!(a, b);
        assert_eq!(
            parse_chart(&a, &theme).unwrap(),
            parse_chart(&b, &theme).unwrap()
        );
    }
}

// ---- Harbor's original writer ------------------------------------------

/// `pptx::chart_space_xml` currently interpolates the plot element name as
/// *text* (`...<c:layout/>barChart<c:grouping .../>`): there is no
/// `<c:barChart>` element at all, so the part is not a valid chart and the
/// reader must say so rather than invent data. The test accepts the day
/// that writer is fixed (then the data must round-trip exactly).
#[test]
fn legacy_writer_is_refused_or_exact() {
    for (kind, want) in [
        (ChartKind::Bar, "column"),
        (ChartKind::Line, "line"),
        (ChartKind::Pie, "pie"),
        (ChartKind::Scatter, "scatter"),
    ] {
        let xml = chart_space_xml(kind, "Legacy <1> & co", &cats(), &data());
        match parse_chart(&xml, &theme) {
            Err(e) => assert_eq!(e, "chart: plot area has no chart", "{kind:?}"),
            Ok(d) => {
                assert_eq!(d.kind, want);
                assert_eq!(d.title.as_deref(), Some("Legacy <1> & co"));
                // A pie draws one series; the others are not written.
                let expect = if kind == ChartKind::Pie { 1 } else { 3 };
                assert_eq!(d.series.len(), expect, "{kind:?}");
                for (i, s) in d.series.iter().enumerate() {
                    assert_eq!(s.name, data()[i].0);
                    assert_eq!(s.values, data()[i].1);
                }
            }
        }
    }
}

#[test]
fn writer_output_survives_every_truncation() {
    let xml = styled_chart_space_xml(
        ChartKind::Bar,
        "t",
        &cats(),
        &data(),
        &ChartStyle::default(),
    );
    let mut cuts: Vec<usize> = xml.char_indices().map(|(i, _)| i).step_by(7).collect();
    cuts.push(xml.len());
    for cut in cuts {
        // Err or a sane value, never a panic
        let _ = parse_chart(&xml[..cut], &theme);
    }
}
