//! Styled, theme-aware `c:chartSpace` writer.
//!
//! [`crate::pptx::chart_space_xml`] writes a basic chart in one hard-coded
//! blue. This module writes the same data the way a designer would set it
//! up in PowerPoint: series coloured by *theme accents* (`a:schemeClr
//! accentN`, never RGB, so switching the deck's theme or variant recolours
//! the chart), no chart background or border (the chart sits on any slide
//! colour), thin light horizontal gridlines, a quiet category axis, no
//! value-axis line, theme font (`+mn-lt`) and `tx1`-based text so dark
//! slides stay legible, rounded line caps and small circle markers, bars
//! without outline, pie slices separated by a thin background-coloured
//! line.
//!
//! Layout of the cached data, which the embedded workbook (relationship
//! `rId1` -> `../embeddings/chartdataN.xlsx`, referenced by
//! `c:externalData`) must mirror for "Edit Data" to work: row 1 holds the
//! series names (`B1`, `C1`, ...), column `A` from row 2 holds the
//! categories (the X values for a scatter chart), and series `k` has its
//! values in column `B + k`, rows `2..=n+1`.
//!
//! Every element is written in ECMA-376 schema order; [`lint_chart_xml_order`]
//! checks a document against the order tables so a writer change that
//! breaks PowerPoint's strict reader fails a test instead of a customer.
//! Output is deterministic, and text is XML-escaped with characters that
//! are illegal in XML 1.0 dropped.

use crate::pptx::ChartKind;
use std::fmt::Write as _;

/// Where the legend sits (`None` = no legend element at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartLegend {
    None,
    Top,
    Bottom,
    Right,
}

/// Structural variant of a [`ChartKind`]. A variant that does not apply to
/// the kind is treated as `Standard`:
///
/// * `Bar`: `Standard` (clustered columns), `Horizontal` (clustered bars),
///   `Stacked`, `PercentStacked` (columns).
/// * `Line`: `Standard`, `Stacked`, `PercentStacked` (lines), `Area`,
///   `StackedArea` (filled areas drawn from the same series).
/// * `Pie`: `Standard` (pie), `Doughnut`.
/// * `Scatter`: `Standard` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartVariant {
    #[default]
    Standard,
    Horizontal,
    Stacked,
    PercentStacked,
    Doughnut,
    Area,
    StackedArea,
}

/// Visual style. [`ChartStyle::default`] is the restrained house style.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartStyle {
    /// A concrete font family, or `None` for the theme's minor font
    /// (`+mn-lt`, with `+mn-ea` / `+mn-cs` for East Asian / complex
    /// scripts).
    pub font: Option<String>,
    /// Scheme colour of chart text (`tx1`, `tx2`, `bg1`, ...). It is
    /// lightened a little (`lumMod 65% lumOff 35%`) the way PowerPoint's
    /// own chart styles do; on a dark slide `tx1` maps to a light colour
    /// and stays light.
    pub text_scheme: String,
    /// Scheme colour of the gridlines and the category axis line. `bg*` /
    /// `lt*` colours are darkened a little (`lumMod`), anything else
    /// (`tx1`, `dk1`, ...) is made translucent, which suits dark decks.
    pub gridline_scheme: String,
    /// Which `accentN` (1..=6) colours series `k` (cycled); pie and
    /// doughnut slices use the same list per slice. Invalid entries are
    /// ignored; empty means `1..=6`. Past one full cycle the accents are
    /// varied with `lumMod`/`lumOff` as PowerPoint does.
    pub series_accents: Vec<u8>,
    pub data_labels: bool,
    pub legend: ChartLegend,
    /// Bar gap width in percent (0..=500).
    pub gap_width: u32,
    /// Smoothed lines (line and scatter).
    pub smooth: bool,
    /// Doughnut hole size in percent (10..=90).
    pub doughnut_hole: u32,
    pub label_font_pt: u32,
    pub title_font_pt: u32,
    pub show_gridlines: bool,
    /// Circle markers on lines (`false`: none on lines, squares on
    /// scatter).
    pub rounded_markers: bool,
}

impl Default for ChartStyle {
    fn default() -> Self {
        ChartStyle {
            font: None,
            text_scheme: "tx1".into(),
            gridline_scheme: "bg2".into(),
            series_accents: vec![1, 2, 3, 4, 5, 6],
            data_labels: false,
            legend: ChartLegend::Bottom,
            gap_width: 80,
            smooth: false,
            doughnut_hole: 60,
            label_font_pt: 10,
            title_font_pt: 14,
            show_gridlines: true,
            rounded_markers: true,
        }
    }
}

/// Write a styled chart for [`ChartVariant::Standard`].
pub fn styled_chart_space_xml(
    kind: ChartKind,
    title: &str,
    categories: &[String],
    series: &[(String, Vec<f64>)],
    style: &ChartStyle,
) -> String {
    styled_chart_space_xml_variant(
        kind,
        ChartVariant::Standard,
        title,
        categories,
        series,
        style,
    )
}

/// Write a styled chart of the given structural variant.
pub fn styled_chart_space_xml_variant(
    kind: ChartKind,
    variant: ChartVariant,
    title: &str,
    categories: &[String],
    series: &[(String, Vec<f64>)],
    style: &ChartStyle,
) -> String {
    let w = Writer::new(style, categories, series);
    let plot = Plot::of(kind, variant);
    let plot_xml = w.plot(plot);
    let axes_xml = w.axes(plot);
    let title_xml = w.title(title);
    let legend_xml = w.legend();
    let auto_deleted = if title_xml.is_empty() { 1 } else { 0 };
    let text_pr = w.txpr(style.label_font_pt, false, "");
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            "\n",
            r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
            r#"<c:date1904 val="0"/><c:lang val="en-US"/><c:roundedCorners val="0"/>"#,
            "<c:chart>{title}<c:autoTitleDeleted val=\"{atd}\"/>",
            "<c:plotArea><c:layout/>{plot}{axes}<c:spPr>{nofill}</c:spPr></c:plotArea>",
            "{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>",
            "<c:spPr>{nofill}</c:spPr>{txpr}",
            r#"<c:externalData r:id="rId1"><c:autoUpdate val="0"/></c:externalData>"#,
            "</c:chartSpace>"
        ),
        title = title_xml,
        atd = auto_deleted,
        plot = plot_xml,
        axes = axes_xml,
        nofill = NO_FILL_NO_LINE,
        legend = legend_xml,
        txpr = text_pr,
    )
}

// ---------------------------------------------------------------------------
// Plot selection

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grouping {
    Clustered,
    Stacked,
    Percent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plot {
    Bar {
        horizontal: bool,
        grouping: Grouping,
    },
    Line {
        grouping: Grouping,
    },
    Area {
        grouping: Grouping,
    },
    Pie,
    Doughnut,
    Scatter,
}

impl Plot {
    fn of(kind: ChartKind, variant: ChartVariant) -> Plot {
        use ChartVariant as V;
        match kind {
            ChartKind::Bar => match variant {
                V::Horizontal => Plot::Bar {
                    horizontal: true,
                    grouping: Grouping::Clustered,
                },
                V::Stacked => Plot::Bar {
                    horizontal: false,
                    grouping: Grouping::Stacked,
                },
                V::PercentStacked => Plot::Bar {
                    horizontal: false,
                    grouping: Grouping::Percent,
                },
                _ => Plot::Bar {
                    horizontal: false,
                    grouping: Grouping::Clustered,
                },
            },
            ChartKind::Line => match variant {
                V::Stacked => Plot::Line {
                    grouping: Grouping::Stacked,
                },
                V::PercentStacked => Plot::Line {
                    grouping: Grouping::Percent,
                },
                V::Area => Plot::Area {
                    grouping: Grouping::Clustered,
                },
                V::StackedArea => Plot::Area {
                    grouping: Grouping::Stacked,
                },
                _ => Plot::Line {
                    grouping: Grouping::Clustered,
                },
            },
            ChartKind::Pie => {
                if variant == V::Doughnut {
                    Plot::Doughnut
                } else {
                    Plot::Pie
                }
            }
            ChartKind::Scatter => Plot::Scatter,
        }
    }

    fn grouping_name(self) -> &'static str {
        let g = match self {
            Plot::Bar { grouping, .. } | Plot::Line { grouping } | Plot::Area { grouping } => {
                grouping
            }
            _ => Grouping::Clustered,
        };
        match (self, g) {
            (Plot::Bar { .. }, Grouping::Clustered) => "clustered",
            (_, Grouping::Clustered) => "standard",
            (_, Grouping::Stacked) => "stacked",
            (_, Grouping::Percent) => "percentStacked",
        }
    }

    fn is_percent(self) -> bool {
        matches!(
            self,
            Plot::Bar {
                grouping: Grouping::Percent,
                ..
            } | Plot::Line {
                grouping: Grouping::Percent
            }
        )
    }

    fn is_radial(self) -> bool {
        matches!(self, Plot::Pie | Plot::Doughnut)
    }
}

const AX_CAT: u32 = 500_000_001;
const AX_VAL: u32 = 500_000_002;
const NO_FILL_NO_LINE: &str = "<a:noFill/><a:ln><a:noFill/></a:ln>";
const SCHEMES: [&str; 16] = [
    "bg1", "tx1", "bg2", "tx2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6",
    "hlink", "folHlink", "dk1", "lt1", "dk2", "lt2",
];

// ---------------------------------------------------------------------------
// Writer

struct Writer<'a> {
    st: &'a ChartStyle,
    cats: &'a [String],
    series: &'a [(String, Vec<f64>)],
    accents: Vec<u8>,
    text_scheme: &'static str,
    grid_scheme: &'static str,
    /// Points per series as cached: categories, or the longest series if
    /// there are more values than categories.
    n: usize,
}

impl<'a> Writer<'a> {
    fn new(st: &'a ChartStyle, cats: &'a [String], series: &'a [(String, Vec<f64>)]) -> Self {
        let mut accents: Vec<u8> = st
            .series_accents
            .iter()
            .copied()
            .filter(|a| (1..=6).contains(a))
            .collect();
        if accents.is_empty() {
            accents = vec![1, 2, 3, 4, 5, 6];
        }
        let n = series
            .iter()
            .map(|(_, v)| v.len())
            .max()
            .unwrap_or(0)
            .max(cats.len());
        Writer {
            st,
            cats,
            series,
            accents,
            text_scheme: scheme_or(&st.text_scheme, "tx1"),
            grid_scheme: scheme_or(&st.gridline_scheme, "bg2"),
            n,
        }
    }

    // ---- colour ----------------------------------------------------------

    /// `<a:schemeClr val="accentN">` for series (or slice) `k`, with the
    /// PowerPoint-style variation once the accent list has been cycled.
    fn accent(&self, k: usize, extra: &str) -> String {
        let len = self.accents.len();
        let a = self.accents[k % len];
        let mods = match (k / len) % 4 {
            1 => r#"<a:lumMod val="60000"/>"#,
            2 => r#"<a:lumMod val="80000"/><a:lumOff val="20000"/>"#,
            3 => r#"<a:lumMod val="40000"/><a:lumOff val="60000"/>"#,
            _ => "",
        };
        format!(r#"<a:schemeClr val="accent{a}">{mods}{extra}</a:schemeClr>"#)
    }

    fn text_clr(&self, strong: bool) -> String {
        let (m, o) = if strong {
            (85000, 15000)
        } else {
            (65000, 35000)
        };
        format!(
            r#"<a:schemeClr val="{}"><a:lumMod val="{m}"/><a:lumOff val="{o}"/></a:schemeClr>"#,
            self.text_scheme
        )
    }

    /// Gridline (`strong == false`) or axis-line colour.
    fn line_clr(&self, strong: bool) -> String {
        let g = self.grid_scheme;
        if matches!(g, "bg1" | "bg2" | "lt1" | "lt2") {
            let m = if strong { 75000 } else { 85000 };
            format!(r#"<a:schemeClr val="{g}"><a:lumMod val="{m}"/></a:schemeClr>"#)
        } else {
            let a = if strong { 40000 } else { 18000 };
            format!(r#"<a:schemeClr val="{g}"><a:alpha val="{a}"/></a:schemeClr>"#)
        }
    }

    fn fonts(&self) -> String {
        match self
            .st
            .font
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
        {
            Some(f) => {
                let f = esc(f);
                format!(r#"<a:latin typeface="{f}"/><a:cs typeface="{f}"/>"#)
            }
            None => {
                r#"<a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/>"#
                    .to_string()
            }
        }
    }

    // ---- text ------------------------------------------------------------

    fn txpr(&self, pt: u32, strong: bool, body_attrs: &str) -> String {
        format!(
            r#"<c:txPr><a:bodyPr{body_attrs}/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="{sz}" b="0" i="0" u="none" strike="noStrike" kern="1200" baseline="0"><a:solidFill>{clr}</a:solidFill>{fonts}</a:defRPr></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr>"#,
            sz = pt_to_sz(pt),
            clr = self.text_clr(strong),
            fonts = self.fonts(),
        )
    }

    fn title(&self, title: &str) -> String {
        let title = title.trim();
        if title.is_empty() {
            return String::new();
        }
        let sz = pt_to_sz(self.st.title_font_pt);
        let props =
            format!(r#"sz="{sz}" b="0" i="0" u="none" strike="noStrike" kern="1200" baseline="0""#);
        let fill = format!(
            "<a:solidFill>{}</a:solidFill>{}",
            self.text_clr(true),
            self.fonts()
        );
        let mut paras = String::new();
        for line in title.split('\n') {
            let rtl = if starts_rtl(line) { r#" rtl="1""# } else { "" };
            let _ = write!(
                paras,
                r#"<a:p><a:pPr{rtl}><a:defRPr {props}>{fill}</a:defRPr></a:pPr><a:r><a:rPr lang="en-US" {props}>{fill}</a:rPr><a:t>{t}</a:t></a:r></a:p>"#,
                t = esc(line.trim_end_matches('\r')),
            );
        }
        format!(
            r#"<c:title><c:tx><c:rich><a:bodyPr rot="0" spcFirstLastPara="1" vertOverflow="ellipsis" vert="horz" wrap="square" anchor="ctr" anchorCtr="1"/><a:lstStyle/>{paras}</c:rich></c:tx><c:overlay val="0"/><c:spPr>{NO_FILL_NO_LINE}</c:spPr></c:title>"#
        )
    }

    fn legend(&self) -> String {
        let pos = match self.st.legend {
            ChartLegend::None => return String::new(),
            ChartLegend::Top => "t",
            ChartLegend::Bottom => "b",
            ChartLegend::Right => "r",
        };
        format!(
            r#"<c:legend><c:legendPos val="{pos}"/><c:overlay val="0"/><c:spPr>{NO_FILL_NO_LINE}</c:spPr>{}</c:legend>"#,
            self.txpr(self.st.label_font_pt, false, "")
        )
    }

    // ---- cached data -----------------------------------------------------

    fn ser_tx(&self, k: usize) -> String {
        format!(
            r#"<c:tx><c:strRef><c:f>Sheet1!${col}$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx>"#,
            col = col_letter(k + 1),
            name = esc(&self.series[k].0),
        )
    }

    fn range(&self, col: usize) -> String {
        format!(
            "Sheet1!${c}$2:${c}${last}",
            c = col_letter(col),
            last = self.n.max(1) + 1
        )
    }

    fn cat(&self) -> String {
        let mut pts = String::new();
        for (j, c) in self.cats.iter().enumerate() {
            let _ = write!(pts, r#"<c:pt idx="{j}"><c:v>{}</c:v></c:pt>"#, esc(c));
        }
        format!(
            r#"<c:cat><c:strRef><c:f>{}</c:f><c:strCache><c:ptCount val="{}"/>{pts}</c:strCache></c:strRef></c:cat>"#,
            self.range(0),
            self.n
        )
    }

    fn num_ref(&self, tag: &str, col: usize, values: &[f64]) -> String {
        let mut pts = String::new();
        for (j, v) in values.iter().enumerate() {
            let _ = write!(pts, r#"<c:pt idx="{j}"><c:v>{}</c:v></c:pt>"#, num(*v));
        }
        format!(
            r#"<c:{tag}><c:numRef><c:f>{}</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="{}"/>{pts}</c:numCache></c:numRef></c:{tag}>"#,
            self.range(col),
            self.n
        )
    }

    fn val(&self, k: usize) -> String {
        self.num_ref("val", k + 1, &self.series[k].1)
    }

    /// Scatter X values: the categories read as numbers, else 1-based
    /// positions.
    fn xval(&self) -> String {
        let xs: Vec<f64> = (0..self.n)
            .map(|j| {
                self.cats
                    .get(j)
                    .and_then(|c| c.trim().parse::<f64>().ok())
                    .filter(|x| x.is_finite())
                    .unwrap_or((j + 1) as f64)
            })
            .collect();
        self.num_ref("xVal", 0, &xs)
    }

    // ---- data labels -----------------------------------------------------

    /// `c:dLbls` content. `on == false` writes the "nothing shown" form
    /// PowerPoint itself writes at group level.
    fn dlbls(&self, plot: Plot, on: bool) -> String {
        let flag = |b: bool| if b { 1 } else { 0 };
        let radial = plot.is_radial();
        let show_val = on && !radial;
        let show_pct = on && radial;
        let mut s = String::from("<c:dLbls>");
        if on {
            let fmt = if radial { "0%" } else { "General" };
            let _ = write!(
                s,
                r#"<c:numFmt formatCode="{fmt}" sourceLinked="0"/><c:spPr>{NO_FILL_NO_LINE}</c:spPr>{}"#,
                self.txpr(self.st.label_font_pt, false, "")
            );
            // dLblPos is invalid for doughnut and area charts, and
            // `outEnd` is invalid for stacked bars (PowerPoint repairs the
            // file), so only write what each plot allows.
            let pos = match plot {
                Plot::Bar {
                    grouping: Grouping::Clustered,
                    ..
                } => Some("outEnd"),
                Plot::Bar { .. } => Some("ctr"),
                Plot::Line { .. } | Plot::Scatter => Some("t"),
                Plot::Pie => Some("bestFit"),
                Plot::Doughnut | Plot::Area { .. } => None,
            };
            if let Some(p) = pos {
                let _ = write!(s, r#"<c:dLblPos val="{p}"/>"#);
            }
        }
        let _ = write!(
            s,
            r#"<c:showLegendKey val="0"/><c:showVal val="{}"/><c:showCatName val="0"/><c:showSerName val="0"/><c:showPercent val="{}"/><c:showBubbleSize val="0"/>"#,
            flag(show_val),
            flag(show_pct)
        );
        if on && plot == Plot::Pie {
            s.push_str(r#"<c:showLeaderLines val="1"/>"#);
        }
        s.push_str("</c:dLbls>");
        s
    }

    // ---- series ----------------------------------------------------------

    fn marker(&self, k: usize, scatter: bool) -> String {
        if !self.st.rounded_markers && !scatter {
            return r#"<c:marker><c:symbol val="none"/></c:marker>"#.to_string();
        }
        let (symbol, size) = if self.st.rounded_markers {
            ("circle", if scatter { 6 } else { 5 })
        } else {
            ("square", 6)
        };
        format!(
            r#"<c:marker><c:symbol val="{symbol}"/><c:size val="{size}"/><c:spPr><a:solidFill>{}</a:solidFill><a:ln w="9525"><a:solidFill><a:schemeClr val="bg1"/></a:solidFill></a:ln></c:spPr></c:marker>"#,
            self.accent(k, "")
        )
    }

    fn line_sp(&self, k: usize) -> String {
        format!(
            r#"<c:spPr><a:ln w="28575" cap="rnd"><a:solidFill>{}</a:solidFill><a:round/></a:ln></c:spPr>"#,
            self.accent(k, "")
        )
    }

    fn plot(&self, plot: Plot) -> String {
        let labels = self.st.data_labels;
        let ser_dl = |plot: Plot| {
            if labels {
                self.dlbls(plot, true)
            } else {
                String::new()
            }
        };
        let smooth = if self.st.smooth { 1 } else { 0 };
        let mut sers = String::new();
        let take = if plot == Plot::Pie {
            // A pie draws one series; writing more only confuses readers.
            self.series.len().min(1)
        } else {
            self.series.len()
        };
        for k in 0..take {
            let head = format!(
                r#"<c:ser><c:idx val="{k}"/><c:order val="{k}"/>{}"#,
                self.ser_tx(k)
            );
            let body = match plot {
                Plot::Bar { .. } => format!(
                    r#"<c:spPr><a:solidFill>{}</a:solidFill><a:ln><a:noFill/></a:ln></c:spPr><c:invertIfNegative val="0"/>{}{}{}"#,
                    self.accent(k, ""),
                    ser_dl(plot),
                    self.cat(),
                    self.val(k)
                ),
                Plot::Line { .. } => format!(
                    r#"{}{}{}{}{}<c:smooth val="{smooth}"/>"#,
                    self.line_sp(k),
                    self.marker(k, false),
                    ser_dl(plot),
                    self.cat(),
                    self.val(k)
                ),
                Plot::Area { grouping } => {
                    // Overlapping areas need transparency to stay readable;
                    // stacked areas do not.
                    let extra = if grouping == Grouping::Clustered {
                        r#"<a:alpha val="75000"/>"#
                    } else {
                        ""
                    };
                    format!(
                        r#"<c:spPr><a:solidFill>{}</a:solidFill><a:ln><a:noFill/></a:ln></c:spPr>{}{}{}"#,
                        self.accent(k, extra),
                        ser_dl(plot),
                        self.cat(),
                        self.val(k)
                    )
                }
                Plot::Pie | Plot::Doughnut => {
                    let mut dpts = String::new();
                    for j in 0..self.n {
                        let _ = write!(
                            dpts,
                            r#"<c:dPt><c:idx val="{j}"/><c:bubble3D val="0"/><c:spPr><a:solidFill>{}</a:solidFill><a:ln w="12700"><a:solidFill><a:schemeClr val="bg1"/></a:solidFill></a:ln></c:spPr></c:dPt>"#,
                            self.accent(j, "")
                        );
                    }
                    format!(
                        r#"<c:spPr><a:ln w="12700"><a:solidFill><a:schemeClr val="bg1"/></a:solidFill></a:ln></c:spPr>{dpts}{}{}{}"#,
                        ser_dl(plot),
                        self.cat(),
                        self.val(k)
                    )
                }
                Plot::Scatter => format!(
                    r#"{}{}{}{}{}<c:smooth val="{smooth}"/>"#,
                    self.line_sp(k),
                    self.marker(k, true),
                    ser_dl(plot),
                    self.xval(),
                    self.num_ref("yVal", k + 1, &self.series[k].1)
                ),
            };
            let _ = write!(sers, "{head}{body}</c:ser>");
        }

        let group_dl = self.dlbls(plot, labels);
        let ax = format!(r#"<c:axId val="{AX_CAT}"/><c:axId val="{AX_VAL}"/>"#);
        match plot {
            Plot::Bar {
                horizontal,
                grouping,
            } => {
                let overlap = match grouping {
                    Grouping::Clustered if self.series.len() > 1 => r#"<c:overlap val="-8"/>"#,
                    Grouping::Clustered => "",
                    _ => r#"<c:overlap val="100"/>"#,
                };
                format!(
                    r#"<c:barChart><c:barDir val="{dir}"/><c:grouping val="{g}"/><c:varyColors val="0"/>{sers}{group_dl}<c:gapWidth val="{gw}"/>{overlap}{ax}</c:barChart>"#,
                    dir = if horizontal { "bar" } else { "col" },
                    g = plot.grouping_name(),
                    gw = self.st.gap_width.min(500),
                )
            }
            Plot::Line { .. } => format!(
                r#"<c:lineChart><c:grouping val="{g}"/><c:varyColors val="0"/>{sers}{group_dl}<c:marker val="1"/>{ax}</c:lineChart>"#,
                g = plot.grouping_name()
            ),
            Plot::Area { .. } => format!(
                r#"<c:areaChart><c:grouping val="{g}"/><c:varyColors val="0"/>{sers}{group_dl}{ax}</c:areaChart>"#,
                g = plot.grouping_name()
            ),
            Plot::Pie => format!(
                r#"<c:pieChart><c:varyColors val="1"/>{sers}{group_dl}<c:firstSliceAng val="0"/></c:pieChart>"#
            ),
            Plot::Doughnut => format!(
                r#"<c:doughnutChart><c:varyColors val="1"/>{sers}{group_dl}<c:firstSliceAng val="0"/><c:holeSize val="{}"/></c:doughnutChart>"#,
                self.st.doughnut_hole.clamp(10, 90)
            ),
            Plot::Scatter => format!(
                r#"<c:scatterChart><c:scatterStyle val="{}"/><c:varyColors val="0"/>{sers}{group_dl}{ax}</c:scatterChart>"#,
                if self.st.smooth {
                    "smoothMarker"
                } else {
                    "lineMarker"
                }
            ),
        }
    }

    // ---- axes ------------------------------------------------------------

    fn axes(&self, plot: Plot) -> String {
        if plot.is_radial() {
            return String::new();
        }
        let txpr = self.txpr(self.st.label_font_pt, false, "");
        let grid = if self.st.show_gridlines {
            format!(
                r#"<c:majorGridlines><c:spPr><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill>{}</a:solidFill><a:round/></a:ln></c:spPr></c:majorGridlines>"#,
                self.line_clr(false)
            )
        } else {
            String::new()
        };
        let axis_line = format!(
            r#"<c:spPr><a:noFill/><a:ln w="9525" cap="flat" cmpd="sng" algn="ctr"><a:solidFill>{}</a:solidFill><a:round/></a:ln></c:spPr>"#,
            self.line_clr(true)
        );
        let hidden_line = format!("<c:spPr>{NO_FILL_NO_LINE}</c:spPr>");
        let val_fmt = if plot.is_percent() { "0%" } else { "General" };
        let scaling = r#"<c:scaling><c:orientation val="minMax"/></c:scaling>"#;
        let ticks = r#"<c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/>"#;
        if plot == Plot::Scatter {
            let x = format!(
                r#"<c:valAx><c:axId val="{AX_CAT}"/>{scaling}<c:delete val="0"/><c:axPos val="b"/><c:numFmt formatCode="General" sourceLinked="0"/>{ticks}{axis_line}{txpr}<c:crossAx val="{AX_VAL}"/><c:crosses val="autoZero"/><c:crossBetween val="midCat"/></c:valAx>"#
            );
            let y = format!(
                r#"<c:valAx><c:axId val="{AX_VAL}"/>{scaling}<c:delete val="0"/><c:axPos val="l"/>{grid}<c:numFmt formatCode="General" sourceLinked="0"/>{ticks}{hidden_line}{txpr}<c:crossAx val="{AX_CAT}"/><c:crosses val="autoZero"/><c:crossBetween val="midCat"/></c:valAx>"#
            );
            return x + &y;
        }
        let horizontal = matches!(
            plot,
            Plot::Bar {
                horizontal: true,
                ..
            }
        );
        let (cat_pos, val_pos) = if horizontal { ("l", "b") } else { ("b", "l") };
        let between = if matches!(plot, Plot::Area { .. }) {
            "midCat"
        } else {
            "between"
        };
        format!(
            r#"<c:catAx><c:axId val="{AX_CAT}"/>{scaling}<c:delete val="0"/><c:axPos val="{cat_pos}"/><c:numFmt formatCode="General" sourceLinked="1"/>{ticks}{axis_line}{txpr}<c:crossAx val="{AX_VAL}"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/><c:noMultiLvlLbl val="0"/></c:catAx><c:valAx><c:axId val="{AX_VAL}"/>{scaling}<c:delete val="0"/><c:axPos val="{val_pos}"/>{grid}<c:numFmt formatCode="{val_fmt}" sourceLinked="0"/>{ticks}{hidden_line}{txpr}<c:crossAx val="{AX_CAT}"/><c:crosses val="autoZero"/><c:crossBetween val="{between}"/></c:valAx>"#
        )
    }
}

// ---------------------------------------------------------------------------
// Small helpers

fn scheme_or(s: &str, default: &'static str) -> &'static str {
    SCHEMES
        .iter()
        .copied()
        .find(|v| *v == s.trim())
        .unwrap_or(default)
}

fn pt_to_sz(pt: u32) -> u32 {
    // ST_TextFontSize: 100..=400000 (1..=4000 pt)
    pt.clamp(1, 4000) * 100
}

/// 0 -> `A`, 25 -> `Z`, 26 -> `AA`.
fn col_letter(mut i: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// A finite number as xsd:double text (`-0` and NaN/inf become `0`).
fn num(v: f64) -> String {
    if !v.is_finite() || v == 0.0 {
        "0".into()
    } else {
        format!("{v}")
    }
}

/// XML-escape text and attribute values; characters illegal in XML 1.0
/// (controls other than tab/newline, U+FFFE/U+FFFF) are dropped and `\r`
/// is normalised away.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' => out.push(c),
            '\r' => {}
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// First strongly-directional character is Hebrew / Arabic / Syriac /
/// Thaana / NKo or a right-to-left presentation form.
fn starts_rtl(s: &str) -> bool {
    for c in s.chars() {
        let u = c as u32;
        let rtl = (0x0590..=0x08FF).contains(&u)
            || (0xFB1D..=0xFDFF).contains(&u)
            || (0xFE70..=0xFEFF).contains(&u);
        if rtl {
            return true;
        }
        if c.is_alphabetic() {
            return false;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Schema-order lint

/// Child order of the `c:` / `a:` complex types this writer emits
/// (ECMA-376 Part 1, dml-chart.xsd / dml-main.xsd). `|` separates names
/// that share a position (a choice group). A parent not listed here is
/// not checked; a *listed* parent with a child not listed is an error.
const ORDER: &[(&str, &[&str])] = &[
    (
        "chartSpace",
        &[
            "date1904", "lang", "roundedCorners", "AlternateContent", "style", "clrMapOvr",
            "pivotSource", "protection", "chart", "spPr", "txPr", "externalData",
            "printSettings", "userShapes", "extLst",
        ],
    ),
    (
        "chart",
        &[
            "title", "autoTitleDeleted", "pivotFmts", "view3D", "floor", "sideWall", "backWall",
            "plotArea", "legend", "plotVisOnly", "dispBlanksAs", "showDLblsOverMax", "extLst",
        ],
    ),
    ("title", &["tx", "layout", "overlay", "spPr", "txPr", "extLst"]),
    (
        "plotArea",
        &[
            "layout",
            "areaChart|area3DChart|lineChart|line3DChart|stockChart|radarChart|scatterChart|pieChart|pie3DChart|doughnutChart|barChart|bar3DChart|ofPieChart|surfaceChart|surface3DChart|bubbleChart",
            "valAx|catAx|dateAx|serAx",
            "dTable",
            "spPr",
            "extLst",
        ],
    ),
    (
        "barChart",
        &[
            "barDir", "grouping", "varyColors", "ser", "dLbls", "gapWidth", "overlap",
            "serLines", "axId", "extLst",
        ],
    ),
    (
        "lineChart",
        &[
            "grouping", "varyColors", "ser", "dLbls", "dropLines", "hiLowLines", "upDownBars",
            "marker", "smooth", "axId", "extLst",
        ],
    ),
    (
        "areaChart",
        &["grouping", "varyColors", "ser", "dLbls", "dropLines", "axId", "extLst"],
    ),
    ("pieChart", &["varyColors", "ser", "dLbls", "firstSliceAng", "extLst"]),
    (
        "doughnutChart",
        &["varyColors", "ser", "dLbls", "firstSliceAng", "holeSize", "extLst"],
    ),
    (
        "scatterChart",
        &["scatterStyle", "varyColors", "ser", "dLbls", "axId", "extLst"],
    ),
    (
        "barChart.ser",
        &[
            "idx", "order", "tx", "spPr", "invertIfNegative", "pictureOptions", "dPt", "dLbls",
            "trendline", "errBars", "cat", "val", "shape", "extLst",
        ],
    ),
    (
        "lineChart.ser",
        &[
            "idx", "order", "tx", "spPr", "marker", "dPt", "dLbls", "trendline", "errBars",
            "cat", "val", "smooth", "extLst",
        ],
    ),
    (
        "areaChart.ser",
        &[
            "idx", "order", "tx", "spPr", "pictureOptions", "dPt", "dLbls", "trendline",
            "errBars", "cat", "val", "extLst",
        ],
    ),
    (
        "pieChart.ser",
        &["idx", "order", "tx", "spPr", "explosion", "dPt", "dLbls", "cat", "val", "extLst"],
    ),
    (
        "doughnutChart.ser",
        &["idx", "order", "tx", "spPr", "explosion", "dPt", "dLbls", "cat", "val", "extLst"],
    ),
    (
        "scatterChart.ser",
        &[
            "idx", "order", "tx", "spPr", "marker", "dPt", "dLbls", "trendline", "errBars",
            "xVal", "yVal", "smooth", "extLst",
        ],
    ),
    (
        "dPt",
        &[
            "idx", "invertIfNegative", "marker", "bubble3D", "explosion", "spPr",
            "pictureOptions", "extLst",
        ],
    ),
    ("marker", &["symbol", "size", "spPr", "extLst"]),
    (
        "dLbls",
        &[
            "dLbl", "delete", "numFmt", "spPr", "txPr", "dLblPos", "showLegendKey", "showVal",
            "showCatName", "showSerName", "showPercent", "showBubbleSize", "separator",
            "showLeaderLines", "leaderLines", "extLst",
        ],
    ),
    (
        "catAx",
        &[
            "axId", "scaling", "delete", "axPos", "majorGridlines", "minorGridlines", "title",
            "numFmt", "majorTickMark", "minorTickMark", "tickLblPos", "spPr", "txPr",
            "crossAx", "crosses|crossesAt", "auto", "lblAlgn", "lblOffset", "tickLblSkip",
            "tickMarkSkip", "noMultiLvlLbl", "extLst",
        ],
    ),
    (
        "valAx",
        &[
            "axId", "scaling", "delete", "axPos", "majorGridlines", "minorGridlines", "title",
            "numFmt", "majorTickMark", "minorTickMark", "tickLblPos", "spPr", "txPr",
            "crossAx", "crosses|crossesAt", "crossBetween", "majorUnit", "minorUnit",
            "dispUnits", "extLst",
        ],
    ),
    ("scaling", &["logBase", "orientation", "max", "min", "extLst"]),
    (
        "legend",
        &["legendPos", "legendEntry", "layout", "overlay", "spPr", "txPr", "extLst"],
    ),
    ("strRef", &["f", "strCache", "extLst"]),
    ("numRef", &["f", "numCache", "extLst"]),
    ("strCache", &["ptCount", "pt", "extLst"]),
    ("numCache", &["formatCode", "ptCount", "pt", "extLst"]),
    ("pt", &["v"]),
    ("externalData", &["autoUpdate"]),
    ("majorGridlines", &["spPr"]),
    (
        "spPr",
        &[
            "xfrm",
            "custGeom|prstGeom",
            "noFill|solidFill|gradFill|blipFill|pattFill|grpFill",
            "ln",
            "effectLst|effectDag",
            "scene3d",
            "sp3d",
            "extLst",
        ],
    ),
    (
        "ln",
        &[
            "noFill|solidFill|gradFill|pattFill",
            "prstDash|custDash",
            "round|bevel|miter",
            "headEnd",
            "tailEnd",
            "extLst",
        ],
    ),
    (
        "defRPr",
        &[
            "ln", "noFill|solidFill|gradFill|blipFill|pattFill|grpFill",
            "effectLst|effectDag", "highlight", "uLnTx|uLn", "uFillTx|uFill", "latin", "ea",
            "cs", "sym", "hlinkClick", "hlinkMouseOver", "rtl", "extLst",
        ],
    ),
    (
        "rPr",
        &[
            "ln", "noFill|solidFill|gradFill|blipFill|pattFill|grpFill",
            "effectLst|effectDag", "highlight", "uLnTx|uLn", "uFillTx|uFill", "latin", "ea",
            "cs", "sym", "hlinkClick", "hlinkMouseOver", "rtl", "extLst",
        ],
    ),
    ("txPr", &["bodyPr", "lstStyle", "p"]),
    ("rich", &["bodyPr", "lstStyle", "p"]),
    ("p", &["pPr", "r|br|fld", "endParaRPr"]),
    ("r", &["rPr", "t"]),
];

/// Check every element of `xml` against the schema child order for the
/// complex types in [`ORDER`]. `Err` names the first offending element.
pub fn lint_chart_xml_order(xml: &str) -> Result<(), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("not well-formed: {e}"))?;
    for node in doc.descendants().filter(|n| n.is_element()) {
        let name = node.tag_name().name();
        let key = if name == "ser" {
            let parent = node.parent_element().map_or("", |p| p.tag_name().name());
            format!("{parent}.ser")
        } else {
            name.to_string()
        };
        let Some((_, order)) = ORDER.iter().find(|(k, _)| *k == key) else {
            continue;
        };
        let mut last = 0usize;
        for child in node.children().filter(|n| n.is_element()) {
            let cname = child.tag_name().name();
            let Some(rank) = order
                .iter()
                .position(|slot| slot.split('|').any(|alt| alt == cname))
            else {
                return Err(format!("<{key}> may not contain <{cname}>"));
            };
            if rank < last {
                return Err(format!("<{key}>: <{cname}> is out of schema order"));
            }
            last = rank;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    fn cats() -> Vec<String> {
        ["Q1", "Q2", "Q3", "Q4"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn data() -> Vec<(String, Vec<f64>)> {
        vec![
            ("Revenue".into(), vec![10.0, 12.5, 9.0, 15.0]),
            ("Cost".into(), vec![6.0, 7.0, 6.5, 8.0]),
            ("Profit".into(), vec![4.0, 5.5, 2.5, 7.0]),
        ]
    }

    const KINDS: [(ChartKind, ChartVariant); 14] = [
        (ChartKind::Bar, ChartVariant::Standard),
        (ChartKind::Bar, ChartVariant::Horizontal),
        (ChartKind::Bar, ChartVariant::Stacked),
        (ChartKind::Bar, ChartVariant::PercentStacked),
        (ChartKind::Line, ChartVariant::Standard),
        (ChartKind::Line, ChartVariant::Stacked),
        (ChartKind::Line, ChartVariant::PercentStacked),
        (ChartKind::Line, ChartVariant::Area),
        (ChartKind::Line, ChartVariant::StackedArea),
        (ChartKind::Pie, ChartVariant::Standard),
        (ChartKind::Pie, ChartVariant::Doughnut),
        (ChartKind::Scatter, ChartVariant::Standard),
        // inapplicable variants fall back to Standard
        (ChartKind::Pie, ChartVariant::Horizontal),
        (ChartKind::Scatter, ChartVariant::Area),
    ];

    fn styles() -> Vec<ChartStyle> {
        let base = ChartStyle::default();
        vec![
            base.clone(),
            ChartStyle {
                data_labels: true,
                smooth: true,
                legend: ChartLegend::None,
                show_gridlines: false,
                rounded_markers: false,
                font: Some("Georgia".into()),
                ..base.clone()
            },
            ChartStyle {
                data_labels: true,
                legend: ChartLegend::Right,
                gridline_scheme: "tx1".into(),
                text_scheme: "bg1".into(),
                series_accents: vec![3, 9, 5],
                ..base.clone()
            },
            ChartStyle {
                legend: ChartLegend::Top,
                gap_width: 9999,
                doughnut_hole: 1,
                label_font_pt: 0,
                title_font_pt: 99999,
                ..base
            },
        ]
    }

    fn write(kind: ChartKind, v: ChartVariant, st: &ChartStyle) -> String {
        styled_chart_space_xml_variant(kind, v, "Quarterly results", &cats(), &data(), st)
    }

    #[test]
    fn every_variant_and_style_is_well_formed_and_in_schema_order() {
        for (kind, v) in KINDS {
            for st in styles() {
                let xml = write(kind, v, &st);
                roxmltree::Document::parse(&xml)
                    .unwrap_or_else(|e| panic!("{kind:?}/{v:?}: {e}\n{xml}"));
                lint_chart_xml_order(&xml).unwrap_or_else(|e| panic!("{kind:?}/{v:?}: {e}\n{xml}"));
            }
        }
    }

    #[test]
    fn lint_catches_misordered_and_unknown_children() {
        let ok = write(
            ChartKind::Bar,
            ChartVariant::Standard,
            &ChartStyle::default(),
        );
        // swap gapWidth before the series
        let bad = ok.replace(
            "<c:varyColors val=\"0\"/>",
            "<c:varyColors val=\"0\"/><c:gapWidth val=\"10\"/>",
        );
        assert!(lint_chart_xml_order(&bad).is_err());
        let bad = ok.replace("<c:invertIfNegative val=\"0\"/>", "<c:bogus/>");
        assert!(lint_chart_xml_order(&bad).unwrap_err().contains("bogus"));
        assert!(lint_chart_xml_order("<broken").is_err());
    }

    #[test]
    fn theme_aware_no_hard_coded_colours() {
        for (kind, v) in KINDS {
            for st in styles() {
                let xml = write(kind, v, &st);
                assert!(!xml.contains("srgbClr"), "{kind:?}/{v:?} hard-codes RGB");
                assert!(xml.contains("schemeClr"));
                // the theme font, unless a concrete one was asked for
                if st.font.is_none() {
                    assert!(xml.contains("+mn-lt"));
                } else {
                    assert!(xml.contains("Georgia") && !xml.contains("+mn-lt"));
                }
            }
        }
        let xml = write(
            ChartKind::Bar,
            ChartVariant::Standard,
            &ChartStyle::default(),
        );
        for n in 1..=3 {
            assert!(xml.contains(&format!(r#"val="accent{n}""#)));
        }
    }

    #[test]
    fn series_accents_are_honoured_cycled_and_sanitised() {
        let st = ChartStyle {
            series_accents: vec![4, 9, 2, 0],
            ..ChartStyle::default()
        };
        let xml = write(ChartKind::Bar, ChartVariant::Standard, &st);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let sers: Vec<_> = doc
            .descendants()
            .filter(|n| n.tag_name().name() == "ser")
            .collect();
        let accent = |s: &roxmltree::Node| {
            s.descendants()
                .find(|n| n.tag_name().name() == "schemeClr")
                .and_then(|n| n.attribute("val"))
                .unwrap()
                .to_string()
        };
        // 9 and 0 are not accents: the list is [4, 2]
        assert_eq!(accent(&sers[0]), "accent4");
        assert_eq!(accent(&sers[1]), "accent2");
        assert_eq!(accent(&sers[2]), "accent4");
        // past a full cycle the colour is varied
        let many: Vec<_> = (0..14).map(|i| (format!("s{i}"), vec![1.0])).collect();
        let xml = styled_chart_space_xml(
            ChartKind::Bar,
            "",
            &["x".to_string()],
            &many,
            &ChartStyle::default(),
        );
        assert!(xml.contains(r#"<a:lumMod val="60000"/>"#));
        lint_chart_xml_order(&xml).unwrap();
    }

    #[test]
    fn invalid_scheme_names_cannot_inject_markup() {
        let st = ChartStyle {
            text_scheme: r#"tx1"/><evil/>"#.into(),
            gridline_scheme: "<x>".into(),
            font: Some(r#"A" onload="x"#.into()),
            ..ChartStyle::default()
        };
        let xml = write(ChartKind::Line, ChartVariant::Standard, &st);
        roxmltree::Document::parse(&xml).unwrap();
        assert!(!xml.contains("evil"));
        assert!(!xml.contains("onload=\"x\""));
    }

    #[test]
    fn deterministic() {
        for (kind, v) in KINDS {
            for st in styles() {
                assert_eq!(write(kind, v, &st), write(kind, v, &st));
            }
        }
    }

    fn q<'a>(doc: &'a roxmltree::Document, name: &str) -> Vec<roxmltree::Node<'a, 'a>> {
        doc.descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == name)
            .collect()
    }

    fn val_of(n: roxmltree::Node) -> String {
        n.attribute("val").unwrap_or("").to_string()
    }

    #[test]
    fn chart_space_boilerplate() {
        let xml = write(
            ChartKind::Bar,
            ChartVariant::Standard,
            &ChartStyle::default(),
        );
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let root = doc.root_element();
        let kids: Vec<_> = root
            .children()
            .filter(|n| n.is_element())
            .map(|n| n.tag_name().name().to_string())
            .collect();
        assert_eq!(
            kids,
            [
                "date1904",
                "lang",
                "roundedCorners",
                "chart",
                "spPr",
                "txPr",
                "externalData"
            ]
        );
        let ext = q(&doc, "externalData")[0];
        assert_eq!(
            ext.attributes()
                .find(|a| a.name() == "id")
                .map(|a| a.value()),
            Some("rId1")
        );
        assert_eq!(val_of(q(&doc, "autoUpdate")[0]), "0");
        // transparent chart space and plot area
        let sp = q(&doc, "spPr");
        let last = sp.last().unwrap();
        assert!(last.children().any(|c| c.tag_name().name() == "noFill"));
        assert_eq!(val_of(q(&doc, "autoTitleDeleted")[0]), "0");
        assert_eq!(val_of(q(&doc, "overlay")[0]), "0");
    }

    #[test]
    fn bar_structure() {
        let st = ChartStyle {
            data_labels: true,
            gap_width: 60,
            ..ChartStyle::default()
        };
        let xml = write(ChartKind::Bar, ChartVariant::Standard, &st);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(val_of(q(&doc, "barDir")[0]), "col");
        assert_eq!(val_of(q(&doc, "grouping")[0]), "clustered");
        assert_eq!(val_of(q(&doc, "gapWidth")[0]), "60");
        assert_eq!(val_of(q(&doc, "overlap")[0]), "-8");
        assert_eq!(q(&doc, "ser").len(), 3);
        // bars have no outline; labels sit outside the end
        assert!(q(&doc, "dLblPos").iter().all(|n| val_of(*n) == "outEnd"));
        // axis ids pair up
        let ids: Vec<_> = q(&doc, "axId").iter().map(|n| val_of(*n)).collect();
        assert_eq!(ids.len(), 4);
        assert_eq!(ids[0], ids[2]);
        assert_eq!(ids[1], ids[3]);
        // value axis line hidden, only horizontal gridlines, one set
        let val_ax = q(&doc, "valAx")[0];
        assert_eq!(q(&doc, "majorGridlines").len(), 1);
        let sp = val_ax
            .children()
            .find(|n| n.tag_name().name() == "spPr")
            .unwrap();
        assert!(sp.descendants().any(|n| n.tag_name().name() == "noFill"));
        assert!(!sp.descendants().any(|n| n.tag_name().name() == "solidFill"));
        // category axis keeps a subtle line
        let cat_ax = q(&doc, "catAx")[0];
        assert!(cat_ax
            .descendants()
            .any(|n| n.tag_name().name() == "solidFill"));
        // horizontal / stacked variants
        let h = write(ChartKind::Bar, ChartVariant::Horizontal, &st);
        let d = roxmltree::Document::parse(&h).unwrap();
        assert_eq!(val_of(q(&d, "barDir")[0]), "bar");
        assert_eq!(val_of(q(&d, "axPos")[0]), "l");
        for (v, g, fmt) in [
            (ChartVariant::Stacked, "stacked", "General"),
            (ChartVariant::PercentStacked, "percentStacked", "0%"),
        ] {
            let s = write(ChartKind::Bar, v, &st);
            let d = roxmltree::Document::parse(&s).unwrap();
            assert_eq!(val_of(q(&d, "grouping")[0]), g);
            assert_eq!(val_of(q(&d, "overlap")[0]), "100");
            // outEnd is invalid for stacked bars
            assert!(q(&d, "dLblPos").iter().all(|n| val_of(*n) == "ctr"));
            let nf = q(&d, "valAx")[0]
                .children()
                .find(|n| n.tag_name().name() == "numFmt")
                .unwrap();
            assert_eq!(nf.attribute("formatCode"), Some(fmt));
        }
    }

    #[test]
    fn line_structure() {
        let st = ChartStyle {
            smooth: true,
            ..ChartStyle::default()
        };
        let xml = write(ChartKind::Line, ChartVariant::Standard, &st);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert!(q(&doc, "ln")
            .iter()
            .any(|n| n.attribute("cap") == Some("rnd")));
        assert!(q(&doc, "round").len() >= 3);
        assert!(q(&doc, "symbol").iter().all(|n| val_of(*n) == "circle"));
        assert!(q(&doc, "size").iter().all(|n| val_of(*n) == "5"));
        assert!(q(&doc, "smooth").iter().all(|n| val_of(*n) == "1"));
        let no_markers = write(
            ChartKind::Line,
            ChartVariant::Standard,
            &ChartStyle {
                rounded_markers: false,
                ..ChartStyle::default()
            },
        );
        assert!(no_markers.contains(r#"<c:symbol val="none"/>"#));
    }

    #[test]
    fn area_scatter_pie_doughnut_label_positions() {
        let st = ChartStyle {
            data_labels: true,
            ..ChartStyle::default()
        };
        // area and doughnut: dLblPos is not allowed by the schema
        for (k, v) in [
            (ChartKind::Line, ChartVariant::Area),
            (ChartKind::Line, ChartVariant::StackedArea),
            (ChartKind::Pie, ChartVariant::Doughnut),
        ] {
            assert!(!write(k, v, &st).contains("dLblPos"), "{k:?}/{v:?}");
        }
        let pie = write(ChartKind::Pie, ChartVariant::Standard, &st);
        let d = roxmltree::Document::parse(&pie).unwrap();
        assert!(q(&d, "dLblPos").iter().all(|n| val_of(*n) == "bestFit"));
        assert_eq!(q(&d, "ser").len(), 1, "a pie draws one series");
        assert_eq!(q(&d, "dPt").len(), 4);
        assert!(q(&d, "catAx").is_empty() && q(&d, "valAx").is_empty());
        // slices are separated by a thin bg1 line
        assert!(pie.contains(r#"<a:ln w="12700"><a:solidFill><a:schemeClr val="bg1"/>"#));
        let dn = write(ChartKind::Pie, ChartVariant::Doughnut, &st);
        let d = roxmltree::Document::parse(&dn).unwrap();
        assert_eq!(val_of(q(&d, "holeSize")[0]), "60");
        assert_eq!(q(&d, "ser").len(), 3);
        // scatter: two value axes, X without gridlines
        let sc = write(ChartKind::Scatter, ChartVariant::Standard, &st);
        let d = roxmltree::Document::parse(&sc).unwrap();
        assert_eq!(q(&d, "valAx").len(), 2);
        assert_eq!(q(&d, "catAx").len(), 0);
        assert_eq!(q(&d, "majorGridlines").len(), 1);
        assert_eq!(val_of(q(&d, "scatterStyle")[0]), "lineMarker");
        assert_eq!(q(&d, "xVal").len(), 3);
    }

    #[test]
    fn legend_and_title_options() {
        for (l, pos) in [
            (ChartLegend::Top, Some("t")),
            (ChartLegend::Bottom, Some("b")),
            (ChartLegend::Right, Some("r")),
            (ChartLegend::None, None),
        ] {
            let st = ChartStyle {
                legend: l,
                ..ChartStyle::default()
            };
            let xml = write(ChartKind::Bar, ChartVariant::Standard, &st);
            let d = roxmltree::Document::parse(&xml).unwrap();
            assert_eq!(
                q(&d, "legendPos").first().map(|n| val_of(*n)).as_deref(),
                pos
            );
            if pos.is_some() {
                assert_eq!(
                    val_of(
                        q(&d, "legend")[0]
                            .children()
                            .find(|n| n.tag_name().name() == "overlay")
                            .unwrap()
                    ),
                    "0"
                );
            }
        }
        // empty title: no c:title and autoTitleDeleted=1
        let xml = styled_chart_space_xml(
            ChartKind::Bar,
            "  ",
            &cats(),
            &data(),
            &ChartStyle::default(),
        );
        let d = roxmltree::Document::parse(&xml).unwrap();
        assert!(q(&d, "title").is_empty());
        assert_eq!(val_of(q(&d, "autoTitleDeleted")[0]), "1");
        // multi-line title: one paragraph per line
        let xml = styled_chart_space_xml(
            ChartKind::Bar,
            "a\nb",
            &cats(),
            &data(),
            &ChartStyle::default(),
        );
        let d = roxmltree::Document::parse(&xml).unwrap();
        let title = q(&d, "title")[0];
        assert_eq!(
            title
                .descendants()
                .filter(|n| n.tag_name().name() == "p")
                .count(),
            2
        );
    }

    #[test]
    fn cached_data_and_cell_layout() {
        let xml = write(
            ChartKind::Bar,
            ChartVariant::Standard,
            &ChartStyle::default(),
        );
        let d = roxmltree::Document::parse(&xml).unwrap();
        let fs: Vec<String> = q(&d, "f")
            .iter()
            .take(4)
            .map(|n| n.text().unwrap().to_string())
            .collect();
        assert_eq!(
            fs,
            [
                "Sheet1!$B$1",
                "Sheet1!$A$2:$A$5",
                "Sheet1!$B$2:$B$5",
                "Sheet1!$C$1"
            ]
        );
        let counts: Vec<_> = q(&d, "ptCount").iter().map(|n| val_of(*n)).collect();
        assert_eq!(&counts[..3], ["1", "4", "4"]);
        // a ragged input never panics and keeps ref ranges consistent
        let ragged = vec![(
            "A".to_string(),
            vec![1.0, 2.0, 3.0, 4.0, 5.0, f64::NAN, f64::INFINITY],
        )];
        let xml = styled_chart_space_xml(
            ChartKind::Line,
            "t",
            &cats(),
            &ragged,
            &ChartStyle::default(),
        );
        roxmltree::Document::parse(&xml).unwrap();
        assert!(xml.contains("$A$2:$A$8"));
        assert!(!xml.contains("NaN") && !xml.contains("inf"));
        // empty input is still a loadable skeleton
        for (k, _) in KINDS {
            let xml = styled_chart_space_xml(k, "", &[], &[], &ChartStyle::default());
            roxmltree::Document::parse(&xml).unwrap();
            lint_chart_xml_order(&xml).unwrap();
        }
    }

    #[test]
    fn column_letters() {
        assert_eq!(col_letter(0), "A");
        assert_eq!(col_letter(1), "B");
        assert_eq!(col_letter(25), "Z");
        assert_eq!(col_letter(26), "AA");
        assert_eq!(col_letter(27), "AB");
        assert_eq!(col_letter(51), "AZ");
        assert_eq!(col_letter(52), "BA");
        assert_eq!(col_letter(701), "ZZ");
        assert_eq!(col_letter(702), "AAA");
    }

    #[test]
    fn hostile_text_is_escaped_not_interpreted() {
        let title = "A <b>&</b> \"q\" 'a' \u{0}\u{1}\u{8}\u{FFFE} \u{202E}שלום";
        let cats = vec!["<c1>".to_string(), "x&y".to_string(), "]]>".to_string()];
        let series = vec![(
            r#"</c:v><c:evil attr="1"/><![CDATA[x]]> שלום عربى"#.to_string(),
            vec![1.0, 2.0, 3.0],
        )];
        for (kind, v) in KINDS {
            let xml = styled_chart_space_xml_variant(
                kind,
                v,
                title,
                &cats,
                &series,
                &ChartStyle::default(),
            );
            let d = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert!(q(&d, "evil").is_empty());
            lint_chart_xml_order(&xml).unwrap();
            let t: String = q(&d, "t").iter().map(|n| n.text().unwrap_or("")).collect();
            assert_eq!(t, "A <b>&</b> \"q\" 'a'  \u{202E}שלום");
            let names: Vec<_> = q(&d, "v")
                .iter()
                .map(|n| n.text().unwrap_or("").to_string())
                .collect();
            assert!(names
                .iter()
                .any(|s| s.contains("<c:evil") && s.contains("שלום")));
            if kind != ChartKind::Scatter {
                // scatter writes categories as numbers, not text
                assert!(names.iter().any(|s| s == "]]>"));
            }
        }
        // RTL title paragraph is flagged right-to-left
        let xml = styled_chart_space_xml(
            ChartKind::Bar,
            "שלום עולם",
            &[],
            &[],
            &ChartStyle::default(),
        );
        assert!(xml.contains(r#"<a:pPr rtl="1">"#));
        let xml = styled_chart_space_xml(
            ChartKind::Bar,
            "Hello שלום",
            &[],
            &[],
            &ChartStyle::default(),
        );
        assert!(!xml.contains("rtl="));
        // hostile font name stays inside its attribute
        let st = ChartStyle {
            font: Some("Evil\"/><x y=\"".into()),
            ..ChartStyle::default()
        };
        let xml = write(ChartKind::Bar, ChartVariant::Standard, &st);
        let d = roxmltree::Document::parse(&xml).unwrap();
        assert!(q(&d, "x").is_empty());
    }

    #[test]
    fn scatter_x_values_come_from_numeric_categories() {
        let cats: Vec<String> = ["1.5", "abc", " 4 "]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let series = vec![("y".to_string(), vec![1.0, 2.0, 3.0])];
        let xml = styled_chart_space_xml(
            ChartKind::Scatter,
            "",
            &cats,
            &series,
            &ChartStyle::default(),
        );
        let d = roxmltree::Document::parse(&xml).unwrap();
        let xs: Vec<String> = q(&d, "xVal")[0]
            .descendants()
            .filter(|n| n.tag_name().name() == "v")
            .map(|n| n.text().unwrap().to_string())
            .collect();
        assert_eq!(xs, ["1.5", "2", "4"]);
    }
}
