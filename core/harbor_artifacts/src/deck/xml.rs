//! Building blocks for the design engine: theme-referenced colours, fills,
//! lines, shadows, text and shapes, serialised to DrawingML / PresentationML.
//!
//! Colours are *scheme references* (`accent1`, `tx1`, `bg2`...) with
//! transforms, never literals, so a theme swap in PowerPoint (or a Harbor
//! re-theme) recolours every shape, card and chart at once. Deterministic by
//! construction: no clock, no randomness, ids assigned in order.

use std::fmt::Write as _;

pub const SLIDE_W: i64 = 12_192_000;
pub const SLIDE_H: i64 = 6_858_000;

/// Escape text for XML content and attribute values.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Control characters other than tab / newline are not legal XML.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Colour

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Bg1,
    Tx1,
    Bg2,
    Tx2,
    Accent(u8),
    Dk1,
    Lt1,
    Dk2,
    Lt2,
}

impl Slot {
    fn name(self) -> String {
        match self {
            Slot::Bg1 => "bg1".into(),
            Slot::Tx1 => "tx1".into(),
            Slot::Bg2 => "bg2".into(),
            Slot::Tx2 => "tx2".into(),
            Slot::Accent(n) => format!("accent{}", n.clamp(1, 6)),
            Slot::Dk1 => "dk1".into(),
            Slot::Lt1 => "lt1".into(),
            Slot::Dk2 => "dk2".into(),
            Slot::Lt2 => "lt2".into(),
        }
    }
}

/// A theme colour plus transforms (all in percent, `100` = unchanged).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clr {
    pub slot: Slot,
    pub lum_mod: Option<u32>,
    pub lum_off: Option<u32>,
    pub alpha: Option<u32>,
}

impl Clr {
    pub const fn new(slot: Slot) -> Self {
        Clr {
            slot,
            lum_mod: None,
            lum_off: None,
            alpha: None,
        }
    }
    pub const fn bg1() -> Self {
        Self::new(Slot::Bg1)
    }
    pub const fn tx1() -> Self {
        Self::new(Slot::Tx1)
    }
    pub const fn bg2() -> Self {
        Self::new(Slot::Bg2)
    }
    pub const fn tx2() -> Self {
        Self::new(Slot::Tx2)
    }
    pub const fn accent(n: u8) -> Self {
        Self::new(Slot::Accent(n))
    }
    pub const fn dk1() -> Self {
        Self::new(Slot::Dk1)
    }
    pub const fn lt1() -> Self {
        Self::new(Slot::Lt1)
    }
    pub const fn dk2() -> Self {
        Self::new(Slot::Dk2)
    }
    pub const fn lt2() -> Self {
        Self::new(Slot::Lt2)
    }
    /// Opacity in percent (`alpha(30)` = 30% opaque).
    pub const fn alpha(mut self, pct: u32) -> Self {
        self.alpha = Some(pct);
        self
    }
    /// HSL luminance modulation and offset in percent (PowerPoint's
    /// "Lighter 80%" is `lum(20, 80)`, "Darker 25%" is `lum(75, 0)`).
    pub const fn lum(mut self, mod_pct: u32, off_pct: u32) -> Self {
        self.lum_mod = Some(mod_pct);
        if off_pct > 0 {
            self.lum_off = Some(off_pct);
        }
        self
    }

    pub fn xml(&self) -> String {
        let mut inner = String::new();
        if let Some(v) = self.lum_mod {
            let _ = write!(inner, "<a:lumMod val=\"{}\"/>", v * 1000);
        }
        if let Some(v) = self.lum_off {
            let _ = write!(inner, "<a:lumOff val=\"{}\"/>", v * 1000);
        }
        if let Some(v) = self.alpha {
            let _ = write!(inner, "<a:alpha val=\"{}\"/>", v * 1000);
        }
        if inner.is_empty() {
            format!("<a:schemeClr val=\"{}\"/>", self.slot.name())
        } else {
            format!("<a:schemeClr val=\"{}\">{inner}</a:schemeClr>", self.slot.name())
        }
    }
}

// ---------------------------------------------------------------------------
// Fill, line, shadow

#[derive(Debug, Clone, PartialEq)]
pub enum Fill {
    None,
    Solid(Clr),
    /// Linear gradient, `angle` in degrees clockwise from left→right.
    Linear {
        angle: u32,
        stops: Vec<(u32, Clr)>,
    },
    /// Circular gradient from the centre outward (percent positions).
    Radial {
        stops: Vec<(u32, Clr)>,
    },
}

impl Fill {
    pub fn xml(&self) -> String {
        let stops = |stops: &[(u32, Clr)]| {
            let mut s = String::from("<a:gsLst>");
            for (pos, c) in stops {
                let _ = write!(s, "<a:gs pos=\"{}\">{}</a:gs>", pos * 1000, c.xml());
            }
            s.push_str("</a:gsLst>");
            s
        };
        match self {
            Fill::None => "<a:noFill/>".into(),
            Fill::Solid(c) => format!("<a:solidFill>{}</a:solidFill>", c.xml()),
            Fill::Linear { angle, stops: st } => format!(
                "<a:gradFill rotWithShape=\"1\">{}<a:lin ang=\"{}\" scaled=\"0\"/></a:gradFill>",
                stops(st),
                (*angle % 360) * 60000
            ),
            Fill::Radial { stops: st } => format!(
                "<a:gradFill rotWithShape=\"1\">{}<a:path path=\"circle\"><a:fillToRect l=\"50000\" t=\"50000\" r=\"50000\" b=\"50000\"/></a:path></a:gradFill>",
                stops(st)
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub clr: Clr,
    /// Width in points.
    pub w_pt: f64,
    pub dash: Option<&'static str>,
}

impl Line {
    pub fn new(clr: Clr, w_pt: f64) -> Self {
        Line {
            clr,
            w_pt,
            dash: None,
        }
    }
    pub fn xml(&self) -> String {
        format!(
            "<a:ln w=\"{}\" cap=\"flat\"><a:solidFill>{}</a:solidFill><a:prstDash val=\"{}\"/><a:round/></a:ln>",
            (self.w_pt * 12700.0).round() as i64,
            self.clr.xml(),
            self.dash.unwrap_or("solid")
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub blur_pt: f64,
    pub dist_pt: f64,
    /// Direction in degrees clockwise from +x (90 = straight down).
    pub dir: u32,
    pub alpha: u32,
}

impl Shadow {
    pub const SOFT: Shadow = Shadow {
        blur_pt: 18.0,
        dist_pt: 6.0,
        dir: 90,
        alpha: 14,
    };
    pub fn xml(&self) -> String {
        format!(
            "<a:effectLst><a:outerShdw blurRad=\"{}\" dist=\"{}\" dir=\"{}\" algn=\"t\" rotWithShape=\"0\"><a:srgbClr val=\"000000\"><a:alpha val=\"{}\"/></a:srgbClr></a:outerShdw></a:effectLst>",
            (self.blur_pt * 12700.0).round() as i64,
            (self.dist_pt * 12700.0).round() as i64,
            self.dir * 60000,
            self.alpha * 1000
        )
    }
}

// ---------------------------------------------------------------------------
// Geometry

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct R {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl R {
    pub const fn new(x: i64, y: i64, w: i64, h: i64) -> Self {
        R { x, y, w, h }
    }
    /// Mirror horizontally inside the slide (for right-to-left decks).
    pub fn mirrored(self) -> R {
        R {
            x: SLIDE_W - self.x - self.w,
            ..self
        }
    }
    pub fn right(&self) -> i64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i64 {
        self.y + self.h
    }
    pub fn inset(self, d: i64) -> R {
        R {
            x: self.x + d,
            y: self.y + d,
            w: (self.w - 2 * d).max(0),
            h: (self.h - 2 * d).max(0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Geom {
    Rect,
    /// Corner radius as a percent of the short side (0–50).
    Round(u32),
    Ellipse,
    Line,
    Triangle,
    RtTriangle,
    Chevron,
    HomePlate,
    /// Quarter/partial ring: preset `blockArc`/`arc` need adjust values; we
    /// only use full rings via `Donut(hole_pct)`.
    Donut(u32),
}

impl Geom {
    fn xml(&self) -> String {
        match self {
            Geom::Rect => "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>".into(),
            Geom::Round(pct) => format!(
                "<a:prstGeom prst=\"roundRect\"><a:avLst><a:gd name=\"adj\" fmla=\"val {}\"/></a:avLst></a:prstGeom>",
                (*pct).min(50) * 1000
            ),
            Geom::Ellipse => "<a:prstGeom prst=\"ellipse\"><a:avLst/></a:prstGeom>".into(),
            Geom::Line => "<a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom>".into(),
            Geom::Triangle => "<a:prstGeom prst=\"triangle\"><a:avLst/></a:prstGeom>".into(),
            Geom::RtTriangle => "<a:prstGeom prst=\"rtTriangle\"><a:avLst/></a:prstGeom>".into(),
            Geom::Chevron => "<a:prstGeom prst=\"chevron\"><a:avLst/></a:prstGeom>".into(),
            Geom::HomePlate => "<a:prstGeom prst=\"homePlate\"><a:avLst/></a:prstGeom>".into(),
            Geom::Donut(hole) => format!(
                "<a:prstGeom prst=\"donut\"><a:avLst><a:gd name=\"adj\" fmla=\"val {}\"/></a:avLst></a:prstGeom>",
                (*hole).min(50) * 1000
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Text

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontRef {
    Major,
    Minor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    /// Points; `None` inherits.
    pub size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: bool,
    pub color: Option<Clr>,
    pub font: Option<FontRef>,
    /// Character spacing in points (letter-spaced small caps labels).
    pub spacing: Option<f64>,
    pub caps: bool,
    /// A slide-number field instead of literal text.
    pub slide_number: bool,
}

impl Run {
    pub fn new(text: impl Into<String>) -> Self {
        Run {
            text: text.into(),
            size: None,
            bold: None,
            italic: false,
            color: None,
            font: None,
            spacing: None,
            caps: false,
            slide_number: false,
        }
    }
    /// The current slide's number (a live field, not a literal).
    pub fn slide_number() -> Self {
        let mut r = Run::new("\u{2039}#\u{203a}");
        r.slide_number = true;
        r
    }
    pub fn size(mut self, pt: f64) -> Self {
        self.size = Some(pt);
        self
    }
    pub fn bold(mut self) -> Self {
        self.bold = Some(true);
        self
    }
    pub fn regular(mut self) -> Self {
        self.bold = Some(false);
        self
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub fn color(mut self, c: Clr) -> Self {
        self.color = Some(c);
        self
    }
    pub fn font(mut self, f: FontRef) -> Self {
        self.font = Some(f);
        self
    }
    pub fn spacing(mut self, pt: f64) -> Self {
        self.spacing = Some(pt);
        self
    }
    pub fn caps(mut self) -> Self {
        self.caps = true;
        self
    }

    fn xml(&self, rtl: bool) -> String {
        let mut attrs = String::from("lang=\"");
        attrs.push_str(if rtl { "ar-SA" } else { "en-US" });
        attrs.push('"');
        if let Some(s) = self.size {
            let _ = write!(attrs, " sz=\"{}\"", (s * 100.0).round() as i64);
        }
        if let Some(b) = self.bold {
            let _ = write!(attrs, " b=\"{}\"", if b { 1 } else { 0 });
        }
        if self.italic {
            attrs.push_str(" i=\"1\"");
        }
        if self.caps {
            attrs.push_str(" cap=\"all\"");
        }
        // Tracking breaks the joins of connected scripts (Arabic): never
        // letter-space a run that contains them.
        let connected = self.text.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c) || ('\u{0750}'..='\u{077F}').contains(&c));
        if let Some(sp) = self.spacing.filter(|_| !connected) {
            let _ = write!(attrs, " spc=\"{}\"", (sp * 100.0).round() as i64);
        }
        attrs.push_str(" dirty=\"0\"");
        let mut inner = String::new();
        if let Some(c) = &self.color {
            let _ = write!(inner, "<a:solidFill>{}</a:solidFill>", c.xml());
        }
        if let Some(f) = self.font {
            let tag = match f {
                FontRef::Major => "+mj",
                FontRef::Minor => "+mn",
            };
            let _ = write!(inner, "<a:latin typeface=\"{tag}-lt\"/><a:cs typeface=\"{tag}-cs\"/>");
        }
        if self.slide_number {
            return format!(
                "<a:fld id=\"{{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}}\" type=\"slidenum\"><a:rPr {attrs}>{inner}</a:rPr><a:t>{}</a:t></a:fld>",
                esc(&self.text)
            );
        }
        format!(
            "<a:r><a:rPr {attrs}>{inner}</a:rPr><a:t>{}</a:t></a:r>",
            esc(&self.text)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Bullet {
    Inherit,
    None,
    Char(&'static str, Clr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Para {
    pub runs: Vec<Run>,
    pub align: Option<Align>,
    pub bullet: Bullet,
    /// Space before/after in points.
    pub before: Option<f64>,
    pub after: Option<f64>,
    /// Line spacing, percent.
    pub line: Option<u32>,
    pub level: u32,
    /// Left margin and indent (EMU) when overriding the inherited ones.
    pub margin: Option<(i64, i64)>,
}

impl Para {
    pub fn new(text: impl Into<String>) -> Self {
        Para::from_runs(vec![Run::new(text)])
    }
    pub fn from_runs(runs: Vec<Run>) -> Self {
        Para {
            runs,
            align: None,
            bullet: Bullet::Inherit,
            before: None,
            after: None,
            line: None,
            level: 0,
            margin: None,
        }
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = Some(a);
        self
    }
    pub fn no_bullet(mut self) -> Self {
        self.bullet = Bullet::None;
        self
    }
    pub fn bullet(mut self, ch: &'static str, c: Clr) -> Self {
        self.bullet = Bullet::Char(ch, c);
        self
    }
    pub fn before(mut self, pt: f64) -> Self {
        self.before = Some(pt);
        self
    }
    pub fn after(mut self, pt: f64) -> Self {
        self.after = Some(pt);
        self
    }
    pub fn line(mut self, pct: u32) -> Self {
        self.line = Some(pct);
        self
    }
    pub fn level(mut self, l: u32) -> Self {
        self.level = l.min(8);
        self
    }
    pub fn margin(mut self, mar_l: i64, indent: i64) -> Self {
        self.margin = Some((mar_l, indent));
        self
    }

    fn xml(&self, rtl: bool) -> String {
        let mut attrs = String::new();
        if self.level > 0 {
            let _ = write!(attrs, " lvl=\"{}\"", self.level);
        }
        if let Some((m, i)) = self.margin {
            // In an RTL paragraph marL is the start (right) margin.
            let _ = write!(attrs, " marL=\"{m}\" indent=\"{i}\"");
        }
        // Alignment is absolute in DrawingML: an RTL deck mirrors it.
        let algn = match (self.align, rtl) {
            (Some(Align::Left), false) => Some("l"),
            (Some(Align::Left), true) => Some("r"),
            (Some(Align::Right), false) => Some("r"),
            (Some(Align::Right), true) => Some("l"),
            (Some(Align::Center), _) => Some("ctr"),
            (None, true) => Some("r"),
            (None, false) => None,
        };
        if let Some(a) = algn {
            let _ = write!(attrs, " algn=\"{a}\"");
        }
        if rtl {
            attrs.push_str(" rtl=\"1\"");
        }
        let mut inner = String::new();
        if let Some(l) = self.line {
            let _ = write!(inner, "<a:lnSpc><a:spcPct val=\"{}\"/></a:lnSpc>", l * 1000);
        }
        if let Some(b) = self.before {
            let _ = write!(
                inner,
                "<a:spcBef><a:spcPts val=\"{}\"/></a:spcBef>",
                (b * 100.0).round() as i64
            );
        }
        if let Some(a) = self.after {
            let _ = write!(
                inner,
                "<a:spcAft><a:spcPts val=\"{}\"/></a:spcAft>",
                (a * 100.0).round() as i64
            );
        }
        match &self.bullet {
            Bullet::Inherit => {}
            Bullet::None => inner.push_str("<a:buNone/>"),
            Bullet::Char(ch, c) => {
                let _ = write!(
                    inner,
                    "<a:buClr>{}</a:buClr><a:buFont typeface=\"Arial\"/><a:buChar char=\"{}\"/>",
                    c.xml(),
                    esc(ch)
                );
            }
        }
        let ppr = if attrs.is_empty() && inner.is_empty() {
            String::new()
        } else {
            format!("<a:pPr{attrs}>{inner}</a:pPr>")
        };
        let runs: String = self.runs.iter().map(|r| r.xml(rtl)).collect();
        format!("<a:p>{ppr}{runs}</a:p>")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub paras: Vec<Para>,
    pub anchor: Anchor,
    /// Insets (left, top, right, bottom) in EMU.
    pub insets: (i64, i64, i64, i64),
    pub shrink: bool,
}

impl Text {
    pub fn new(paras: Vec<Para>) -> Self {
        Text {
            paras,
            anchor: Anchor::Top,
            insets: (0, 0, 0, 0),
            shrink: true,
        }
    }
    pub fn one(p: Para) -> Self {
        Text::new(vec![p])
    }
    pub fn anchor(mut self, a: Anchor) -> Self {
        self.anchor = a;
        self
    }
    pub fn insets(mut self, l: i64, t: i64, r: i64, b: i64) -> Self {
        self.insets = (l, t, r, b);
        self
    }
    pub fn inset(mut self, all: i64) -> Self {
        self.insets = (all, all, all, all);
        self
    }

    fn body_xml(&self, rtl: bool) -> String {
        let anchor = match self.anchor {
            Anchor::Top => "t",
            Anchor::Middle => "ctr",
            Anchor::Bottom => "b",
        };
        let fit = if self.shrink {
            "<a:normAutofit/>"
        } else {
            "<a:noAutofit/>"
        };
        let paras: String = self.paras.iter().map(|p| p.xml(rtl)).collect();
        format!(
            "<p:txBody><a:bodyPr wrap=\"square\" lIns=\"{}\" tIns=\"{}\" rIns=\"{}\" bIns=\"{}\" rtlCol=\"{}\" anchor=\"{anchor}\">{fit}</a:bodyPr><a:lstStyle/>{paras}</p:txBody>",
            self.insets.0,
            self.insets.1,
            self.insets.2,
            self.insets.3,
            if rtl { 1 } else { 0 }
        )
    }
}

// ---------------------------------------------------------------------------
// Shapes

#[derive(Debug, Clone, PartialEq)]
pub struct Ph {
    /// `title`, `ctrTitle`, `subTitle`, `body`, `ftr`, `sldNum`, `pic`.
    pub ty: &'static str,
    pub idx: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub name: String,
    pub alt: Option<String>,
    /// Decorative shapes are skipped by screen readers.
    pub decorative: bool,
    pub geom: Geom,
    pub rect: Option<R>,
    /// Degrees clockwise.
    pub rot: f64,
    pub flip_h: bool,
    pub fill: Fill,
    pub line: Option<Line>,
    pub shadow: Option<Shadow>,
    pub text: Option<Text>,
    pub ph: Option<Ph>,
    pub text_box: bool,
}

impl Shape {
    pub fn new(name: impl Into<String>, geom: Geom, rect: R) -> Self {
        Shape {
            name: name.into(),
            alt: None,
            decorative: false,
            geom,
            rect: Some(rect),
            rot: 0.0,
            flip_h: false,
            fill: Fill::None,
            line: None,
            shadow: None,
            text: None,
            ph: None,
            text_box: false,
        }
    }
    /// A purely decorative shape (hidden from assistive technology).
    pub fn deco(name: impl Into<String>, geom: Geom, rect: R, fill: Fill) -> Self {
        let mut s = Shape::new(name, geom, rect);
        s.decorative = true;
        s.fill = fill;
        s
    }
    pub fn text_box(name: impl Into<String>, rect: R, text: Text) -> Self {
        let mut s = Shape::new(name, Geom::Rect, rect);
        s.text_box = true;
        s.text = Some(text);
        s
    }
    /// A placeholder instance; geometry is inherited unless `rect` is set.
    pub fn placeholder(name: impl Into<String>, ph: Ph, text: Text) -> Self {
        let mut s = Shape::new(name, Geom::Rect, R::new(0, 0, 0, 0));
        s.rect = None;
        s.ph = Some(ph);
        s.text = Some(text);
        s
    }
    pub fn fill(mut self, f: Fill) -> Self {
        self.fill = f;
        self
    }
    pub fn line(mut self, l: Line) -> Self {
        self.line = Some(l);
        self
    }
    pub fn shadow(mut self, s: Shadow) -> Self {
        self.shadow = Some(s);
        self
    }
    pub fn rot(mut self, deg: f64) -> Self {
        self.rot = deg;
        self
    }
    pub fn alt(mut self, a: impl Into<String>) -> Self {
        self.alt = Some(a.into());
        self
    }
    pub fn at(mut self, r: R) -> Self {
        self.rect = Some(r);
        self
    }
    pub fn text(mut self, t: Text) -> Self {
        self.text = Some(t);
        self
    }

    /// Serialise as `p:sp` with the given shape id.
    pub fn xml(&self, id: u32, rtl: bool) -> String {
        let alt = self
            .alt
            .as_deref()
            .map(|a| format!(" descr=\"{}\"", esc(a)))
            .unwrap_or_default();
        let deco_ext = if self.decorative {
            "<a:extLst><a:ext uri=\"{C183D7F6-B498-43B3-948B-1728B52AA6E4}\"><adec:decorative xmlns:adec=\"http://schemas.microsoft.com/office/drawing/2017/decorative\" val=\"1\"/></a:ext></a:extLst>"
        } else {
            ""
        };
        let c_nv_pr = if deco_ext.is_empty() {
            format!("<p:cNvPr id=\"{id}\" name=\"{}\"{alt}/>", esc(&self.name))
        } else {
            format!(
                "<p:cNvPr id=\"{id}\" name=\"{}\"{alt}>{deco_ext}</p:cNvPr>",
                esc(&self.name)
            )
        };
        let (c_nv_sp_pr, nv_pr) = match &self.ph {
            Some(ph) => {
                let idx = ph.idx.map(|i| format!(" idx=\"{i}\"")).unwrap_or_default();
                (
                    "<p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr>".to_string(),
                    format!("<p:nvPr><p:ph type=\"{}\"{idx}/></p:nvPr>", ph.ty),
                )
            }
            None => (
                if self.text_box {
                    "<p:cNvSpPr txBox=\"1\"/>".to_string()
                } else {
                    "<p:cNvSpPr/>".to_string()
                },
                "<p:nvPr/>".to_string(),
            ),
        };
        let xfrm = self
            .rect
            .map(|r| {
                let r = if rtl { r.mirrored() } else { r };
                let rot = if self.rot != 0.0 {
                    format!(" rot=\"{}\"", (self.rot * 60000.0).round() as i64)
                } else {
                    String::new()
                };
                let flip = if self.flip_h != rtl && self.ph.is_none() && self.flip_h {
                    " flipH=\"1\""
                } else {
                    ""
                };
                format!(
                    "<a:xfrm{rot}{flip}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
                    r.x, r.y, r.w, r.h
                )
            })
            .unwrap_or_default();
        let geom = if self.ph.is_some() && self.rect.is_none() {
            String::new()
        } else {
            self.geom.xml()
        };
        let fill = if self.ph.is_some() && matches!(self.fill, Fill::None) {
            String::new()
        } else {
            self.fill.xml()
        };
        let line = match &self.line {
            Some(l) => l.xml(),
            None if self.ph.is_some() => String::new(),
            None => "<a:ln><a:noFill/></a:ln>".to_string(),
        };
        let shadow = self.shadow.map(|s| s.xml()).unwrap_or_default();
        let text = self
            .text
            .as_ref()
            .map(|t| t.body_xml(rtl))
            .unwrap_or_default();
        format!(
            "<p:sp><p:nvSpPr>{c_nv_pr}{c_nv_sp_pr}{nv_pr}</p:nvSpPr><p:spPr>{xfrm}{geom}{fill}{line}{shadow}</p:spPr>{text}</p:sp>"
        )
    }
}

/// A picture on a slide referencing an image part by relationship id.
pub fn pic_xml(id: u32, name: &str, alt: &str, rid: &str, r: R, geom: &Geom, rtl: bool) -> String {
    let r = if rtl { r.mirrored() } else { r };
    format!(
        "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"{}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>{}</p:spPr></p:pic>",
        esc(name),
        esc(alt),
        r.x,
        r.y,
        r.w,
        r.h,
        geom.xml()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_scripts_are_never_letter_spaced() {
        assert!(Run::new("PITCH").spacing(1.6).xml(false).contains("spc=\"160\""));
        assert!(!Run::new("\u{0627}\u{0644}\u{0639}\u{0631}\u{0636}").spacing(1.6).xml(true).contains("spc="));
    }

    #[test]
    fn colours_serialise_with_transforms_in_order() {
        assert_eq!(Clr::accent(1).xml(), "<a:schemeClr val=\"accent1\"/>");
        assert_eq!(
            Clr::accent(2).lum(20, 80).alpha(40).xml(),
            "<a:schemeClr val=\"accent2\"><a:lumMod val=\"20000\"/><a:lumOff val=\"80000\"/><a:alpha val=\"40000\"/></a:schemeClr>"
        );
    }

    #[test]
    fn text_is_escaped_and_rtl_flips_alignment() {
        let p = Para::new("R&D <2026> \"plan\"").align(Align::Left);
        assert!(p.xml(false).contains("R&amp;D &lt;2026&gt; &quot;plan&quot;"));
        assert!(p.xml(false).contains("algn=\"l\""), "an explicit alignment is always written");
        assert!(!Para::new("x").xml(false).contains("algn"), "no alignment stated, none written");
        let x = p.xml(true);
        assert!(x.contains("algn=\"r\"") && x.contains("rtl=\"1\""), "{x}");
        assert!(Para::new("x").align(Align::Center).xml(true).contains("algn=\"ctr\""));
        // Illegal control characters never reach the XML.
        assert!(!Para::new("a\u{0}b\u{1b}c").xml(false).contains('\u{0}'));
    }

    #[test]
    fn shapes_are_well_formed_and_decorations_are_marked() {
        let s = Shape::deco("Orb", Geom::Ellipse, R::new(0, 0, 100, 100), Fill::Solid(Clr::accent(1).alpha(30)));
        let xml = format!(
            "<p:spTree xmlns:p=\"p\" xmlns:a=\"a\">{}</p:spTree>",
            s.xml(5, false)
        );
        roxmltree::Document::parse(&xml).unwrap();
        assert!(xml.contains("decorative"));
        assert!(xml.contains("<a:ln><a:noFill/></a:ln>"));
        let ph = Shape::placeholder("Title 1", Ph { ty: "title", idx: None }, Text::one(Para::new("Hi")));
        let px = ph.xml(2, false);
        assert!(px.contains("<p:ph type=\"title\"/>"));
        assert!(!px.contains("<a:xfrm"), "placeholders inherit geometry");
    }

    #[test]
    fn mirroring_stays_inside_the_slide() {
        let r = R::new(1_000_000, 0, 2_000_000, 10);
        let m = r.mirrored();
        assert_eq!(m.x, SLIDE_W - 3_000_000);
        assert_eq!(m.mirrored(), r);
    }
}
