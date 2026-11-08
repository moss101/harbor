//! Developer tool: render a PPTX (via the real scene reader) to one SVG per
//! slide, for design review without PowerPoint. Text is wrapped with an
//! average-glyph-width estimate, so line breaks are approximate; geometry,
//! colour, gradients, rotation, shadows and layering are exact.
//!
//! `cargo run -p harbor_render --example scene_svg -- deck.pptx out_dir`

use std::fmt::Write as _;

use harbor_render::scene::*;

const PT: f64 = 1.0 / 12700.0;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

struct Defs {
    out: String,
    n: usize,
}

impl Defs {
    fn paint(&mut self, p: &Paint) -> String {
        match p {
            Paint::None => "none".into(),
            Paint::Solid { c, .. } => c.clone(),
            Paint::Linear { angle, stops } => {
                self.n += 1;
                let id = format!("g{}", self.n);
                let a = angle.to_radians();
                let (x2, y2) = (0.5 + a.cos() * 0.5, 0.5 + a.sin() * 0.5);
                let (x1, y1) = (1.0 - x2, 1.0 - y2);
                let _ = write!(
                    self.out,
                    r#"<linearGradient id="{id}" x1="{x1:.4}" y1="{y1:.4}" x2="{x2:.4}" y2="{y2:.4}">"#
                );
                for s in stops {
                    let _ = write!(
                        self.out,
                        r#"<stop offset="{:.4}" stop-color="{}" stop-opacity="{:.3}"/>"#,
                        s.p, s.c, s.a
                    );
                }
                self.out.push_str("</linearGradient>");
                format!("url(#{id})")
            }
            Paint::Radial { cx, cy, stops } => {
                self.n += 1;
                let id = format!("g{}", self.n);
                let _ = write!(
                    self.out,
                    r#"<radialGradient id="{id}" cx="{cx:.4}" cy="{cy:.4}" r="0.75">"#
                );
                for s in stops {
                    let _ = write!(
                        self.out,
                        r#"<stop offset="{:.4}" stop-color="{}" stop-opacity="{:.3}"/>"#,
                        s.p, s.c, s.a
                    );
                }
                self.out.push_str("</radialGradient>");
                format!("url(#{id})")
            }
        }
    }
}

fn paint_opacity(p: &Paint) -> f64 {
    match p {
        Paint::Solid { a, .. } => *a,
        _ => 1.0,
    }
}

fn preset_path(name: &str, adj: &[f64], w: f64, h: f64) -> String {
    let m = w.min(h);
    match name {
        "ellipse" => format!(
            "M0,{cy} A{cx},{cy} 0 1,0 {w},{cy} A{cx},{cy} 0 1,0 0,{cy} Z",
            cx = w / 2.0,
            cy = h / 2.0
        ),
        "roundRect" => {
            let r = adj.first().copied().unwrap_or(16667.0) / 100000.0 * m;
            format!(
                "M{r},0 H{x} Q{w},0 {w},{r} V{y} Q{w},{h} {x},{h} H{r} Q0,{h} 0,{y} V{r} Q0,0 {r},0 Z",
                x = w - r,
                y = h - r
            )
        }
        "triangle" => format!("M{},0 L{w},{h} L0,{h} Z", w / 2.0),
        "rtTriangle" => format!("M0,0 L{w},{h} L0,{h} Z"),
        "diamond" => format!(
            "M{},0 L{w},{} L{},{h} L0,{} Z",
            w / 2.0,
            h / 2.0,
            w / 2.0,
            h / 2.0
        ),
        "line" | "straightConnector1" => format!("M0,0 L{w},{h}"),
        "homePlate" => format!(
            "M0,0 H{} L{w},{} L{},{h} H0 Z",
            w - h / 2.0,
            h / 2.0,
            w - h / 2.0
        ),
        "chevron" => format!(
            "M0,0 H{} L{w},{} L{},{h} H0 L{},{} Z",
            w - h / 2.0,
            h / 2.0,
            w - h / 2.0,
            h / 2.0,
            h / 2.0
        ),
        _ => format!("M0,0 H{w} V{h} H0 Z"),
    }
}

fn geom_path(g: &Geom, w: f64, h: f64) -> String {
    match g {
        Geom::Preset { name, adj } => preset_path(name, adj, w, h),
        Geom::Path { w: pw, h: ph, cmds } => {
            let (sx, sy) = (w / pw, h / ph);
            let mut d = String::new();
            for c in cmds {
                match c {
                    PathCmd::M { x, y } => {
                        let _ = write!(d, "M{:.2},{:.2} ", x * sx, y * sy);
                    }
                    PathCmd::L { x, y } => {
                        let _ = write!(d, "L{:.2},{:.2} ", x * sx, y * sy);
                    }
                    PathCmd::C {
                        x1,
                        y1,
                        x2,
                        y2,
                        x,
                        y,
                    } => {
                        let _ = write!(
                            d,
                            "C{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} ",
                            x1 * sx,
                            y1 * sy,
                            x2 * sx,
                            y2 * sy,
                            x * sx,
                            y * sy
                        );
                    }
                    PathCmd::Q { x1, y1, x, y } => {
                        let _ = write!(
                            d,
                            "Q{:.2},{:.2} {:.2},{:.2} ",
                            x1 * sx,
                            y1 * sy,
                            x * sx,
                            y * sy
                        );
                    }
                    PathCmd::Z => d.push_str("Z "),
                }
            }
            d
        }
    }
}

fn text_svg(t: &TextBody, w: f64, h: f64, out: &mut String) {
    let (il, it, ir, ib) = (t.ins[0] * PT, t.ins[1] * PT, t.ins[2] * PT, t.ins[3] * PT);
    let avail_w = (w - il - ir).max(1.0);
    struct Line {
        runs: Vec<(String, Run)>,
        width: f64,
        size: f64,
        algn: String,
        bullet: Option<String>,
        rtl: bool,
        indent: f64,
        bullet_x: f64,
        before: f64,
    }
    let mut lines: Vec<Line> = Vec::new();
    let scale = t.font_scale.max(0.1);
    for p in &t.paras {
        let marl = p.mar_l * PT;
        let first_indent = marl + p.indent * PT;
        let mut cur: Option<Line> = None;
        let size0 = p.runs.first().map(|r| r.sz).unwrap_or(p.end_sz) * scale;
        let bullet = p.bullet.as_ref().map(|b| {
            if b.kind == "char" {
                b.ch.clone()
            } else {
                format!("{}.", b.start)
            }
        });
        let mut first = true;
        let mut new_line = |first: &mut bool, lines: &mut Vec<Line>, cur: &mut Option<Line>| {
            if let Some(l) = cur.take() {
                lines.push(l);
            }
            // A bulleted first line starts its TEXT at the paragraph margin
            // (the hanging indent) and puts the bullet at marL + indent.
            let ind = if *first && bullet.is_none() {
                first_indent
            } else {
                marl
            };
            let l = Line {
                runs: Vec::new(),
                width: ind.max(0.0),
                size: size0,
                algn: p.algn.clone(),
                bullet: if *first { bullet.clone() } else { None },
                rtl: p.rtl,
                indent: ind,
                bullet_x: first_indent,
                before: if *first { p.spc_bef } else { 0.0 },
            };
            *first = false;
            *cur = Some(l);
        };
        new_line(&mut first, &mut lines, &mut cur);
        if p.runs.is_empty() {
            if let Some(mut l) = cur.take() {
                l.size = p.end_sz * scale;
                lines.push(l);
            }
            continue;
        }
        for r in &p.runs {
            if r.t == "\n" {
                new_line(&mut first, &mut lines, &mut cur);
                continue;
            }
            let sz = r.sz * scale;
            for word in r.t.split_inclusive(' ') {
                let ww = word.chars().count() as f64 * sz * if r.b { 0.56 } else { 0.51 };
                let l = cur.as_mut().unwrap();
                if t.wrap && l.width + ww > avail_w && !l.runs.is_empty() {
                    new_line(&mut first, &mut lines, &mut cur);
                }
                let l = cur.as_mut().unwrap();
                l.width += ww;
                l.size = l.size.max(sz);
                l.runs.push((word.to_string(), r.clone()));
            }
        }
        if let Some(l) = cur.take() {
            lines.push(l);
        }
    }
    let total: f64 = lines.iter().map(|l| l.size * 1.2 + l.before).sum();
    let mut y = match t.anchor.as_str() {
        "ctr" => it + ((h - it - ib) - total) / 2.0,
        "b" => h - ib - total,
        _ => it,
    };
    for l in &lines {
        y += l.before + l.size * 1.2;
        let base = y - l.size * 0.25;
        let (x, anchor) = match (l.algn.as_str(), l.rtl) {
            ("ctr", _) => (il + avail_w / 2.0, "middle"),
            ("r", _) => (
                il + avail_w - if l.rtl { l.indent.max(0.0) } else { 0.0 },
                "end",
            ),
            (_, true) => (il + avail_w, "end"),
            _ => (il + l.indent.max(0.0), "start"),
        };
        if let Some(b) = &l.bullet {
            let bx = if l.rtl {
                il + avail_w
            } else {
                il + l.bullet_x.max(0.0)
            };
            let colr = l
                .runs
                .first()
                .map(|r| r.1.c.clone())
                .unwrap_or("#000".into());
            let _ = write!(
                out,
                r##"<text x="{bx:.1}" y="{base:.1}" font-size="{:.1}" fill="{colr}" text-anchor="{}">{}</text>"##,
                l.size,
                if l.rtl { "end" } else { "start" },
                esc(b)
            );
        }
        let _ = write!(
            out,
            r#"<text x="{x:.1}" y="{base:.1}" text-anchor="{anchor}" xml:space="preserve">"#
        );
        for (s, r) in &l.runs {
            let _ = write!(
                out,
                r#"<tspan font-family="{}, Helvetica, Arial, sans-serif" font-size="{:.1}" font-weight="{}" font-style="{}" fill="{}" fill-opacity="{:.2}" letter-spacing="{:.2}">{}</tspan>"#,
                esc(&r.font),
                r.sz * scale,
                if r.b { 700 } else { 400 },
                if r.i { "italic" } else { "normal" },
                r.c,
                r.a,
                r.spc,
                esc(if r.caps { s.as_str() } else { s.as_str() })
            );
        }
        out.push_str("</text>");
    }
}

fn slide_svg(deck: &DeckScene, s: &SlideScene) -> String {
    let (w, h) = (deck.width * PT, deck.height * PT);
    let mut defs = Defs {
        out: String::new(),
        n: 0,
    };
    let mut body = String::new();
    let bg = defs.paint(&s.background);
    let _ = write!(body, r#"<rect width="{w}" height="{h}" fill="{bg}"/>"#);
    for item in &s.items {
        match item {
            Item::Shape(sh) => {
                let f = &sh.frame;
                let (x, y, fw, fh) = (f.x * PT, f.y * PT, f.w * PT, f.h * PT);
                let fill = defs.paint(&sh.fill);
                let op = paint_opacity(&sh.fill);
                let sx = if f.flip_h { -1.0 } else { 1.0 };
                let sy = if f.flip_v { -1.0 } else { 1.0 };
                let _ = write!(
                    body,
                    r#"<g transform="translate({:.2},{:.2}) rotate({}) scale({sx},{sy}) translate({:.2},{:.2})">"#,
                    x + fw / 2.0,
                    y + fh / 2.0,
                    f.rot,
                    -fw / 2.0,
                    -fh / 2.0
                );
                let mut style = format!(r#"fill="{fill}" fill-opacity="{op:.3}""#);
                if let Some(l) = &sh.line {
                    let c = defs.paint(&l.paint);
                    let _ = write!(
                        style,
                        r#" stroke="{c}" stroke-opacity="{:.3}" stroke-width="{:.2}""#,
                        paint_opacity(&l.paint),
                        l.w * PT
                    );
                    if l.dash != "solid" {
                        style.push_str(r#" stroke-dasharray="6 4""#);
                    }
                }
                if let Some(s) = &sh.shadow {
                    let _ = write!(
                        body,
                        r#"<path d="{}" transform="translate({:.1},{:.1})" fill="{}" fill-opacity="{:.2}" filter="url(#blur{})"/>"#,
                        geom_path(&sh.geom, fw, fh),
                        s.dx * PT,
                        s.dy * PT,
                        s.c,
                        s.a,
                        (s.blur * PT / 2.0).round() as i32
                    );
                    let b = (s.blur * PT / 2.0).round() as i32;
                    if !defs.out.contains(&format!(r#"id="blur{b}""#)) {
                        let _ = write!(
                            defs.out,
                            r#"<filter id="blur{b}" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="{b}"/></filter>"#
                        );
                    }
                }
                let _ = write!(
                    body,
                    r#"<path d="{}" {style}/>"#,
                    geom_path(&sh.geom, fw, fh)
                );
                if let Some(t) = &sh.text {
                    // Clip to the frame.
                    let mut inner = String::new();
                    text_svg(t, fw, fh, &mut inner);
                    let _ = write!(body, "{inner}");
                }
                body.push_str("</g>");
            }
            Item::Picture(p) => {
                let f = &p.frame;
                if p.data_b64.is_empty() {
                    let _ = write!(
                        body,
                        r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#cccccc"/>"##,
                        f.x * PT,
                        f.y * PT,
                        f.w * PT,
                        f.h * PT
                    );
                } else {
                    let _ = write!(
                        body,
                        r#"<image x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" preserveAspectRatio="xMidYMid slice" href="data:{};base64,{}"/>"#,
                        f.x * PT,
                        f.y * PT,
                        f.w * PT,
                        f.h * PT,
                        p.mime,
                        p.data_b64
                    );
                }
            }
            Item::Chart(c) => {
                let f = &c.frame;
                let _ = write!(
                    body,
                    r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="none" stroke="#888" stroke-dasharray="4 3"/><text x="{:.1}" y="{:.1}" font-size="12" fill="#666">chart: {}</text>"##,
                    f.x * PT,
                    f.y * PT,
                    f.w * PT,
                    f.h * PT,
                    f.x * PT + 8.0,
                    f.y * PT + 18.0,
                    esc(&c.chart.kind)
                );
            }
            Item::Table(t) => {
                let f = &t.frame;
                let mut yy = f.y * PT;
                for row in &t.rows {
                    let mut xx = f.x * PT;
                    for (ci, cell) in row.cells.iter().enumerate() {
                        let cw = t.cols.get(ci).copied().unwrap_or(0.0) * PT;
                        let fill = defs.paint(&cell.fill);
                        let _ = write!(
                            body,
                            r##"<g transform="translate({xx:.1},{yy:.1})"><rect width="{cw:.1}" height="{:.1}" fill="{fill}" stroke="#ddd"/>"##,
                            row.h * PT
                        );
                        text_svg(&cell.text, cw, row.h * PT, &mut body);
                        body.push_str("</g>");
                        xx += cw;
                    }
                    yy += row.h * PT;
                }
            }
        }
    }
    // Quick Look scales an SVG to COVER its square canvas, cropping a 16:9
    // image; a square canvas with the slide centred in it comes out whole
    // (the slide is then the middle 1600x900 band of a 1600x1600 render).
    let pad = (w - h) / 2.0;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1600" viewBox="0 0 {w} {w}"><rect width="{w}" height="{w}" fill="#202020"/><g transform="translate(0,{pad})"><defs>{}</defs>{body}</g></svg>"##,
        defs.out
    )
}

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("deck.pptx");
    let out = args.next().expect("out_dir");
    std::fs::create_dir_all(&out).unwrap();
    let bytes = std::fs::read(&input).unwrap();
    let deck = harbor_render::deck_scene(&bytes).expect("scene");
    let stem = std::path::Path::new(&input)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    for s in &deck.slides {
        let svg = slide_svg(&deck, s);
        std::fs::write(format!("{out}/{stem}-{:02}.svg", s.index), svg).unwrap();
    }
    println!("{} slides -> {out}", deck.slides.len());
}
