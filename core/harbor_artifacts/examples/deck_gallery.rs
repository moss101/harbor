//! Developer tool: build the same sample deck in a theme, for design review.
//! `cargo run -p harbor_artifacts --example deck_gallery -- <theme|all> <motion> <out_dir> [rtl]`

use harbor_artifacts::deck::{
    Block, Design, DesignedDeck, DesignedSlide, LayoutKind, MotionPreset, THEMES,
};
use harbor_artifacts::pptx::{ChartKind, ChartSpec};

fn sample(theme: &str, motion: MotionPreset, rtl: bool) -> DesignedDeck {
    let ar = rtl;
    let t = |en: &'static str, a: &'static str| if ar { a.to_string() } else { en.to_string() };
    DesignedDeck {
        title: t("Harbor", "ميناء"),
        subtitle: Some(t("Documents that stay yours", "مستندات تبقى ملكك")),
        design: Design {
            theme: theme.into(),
            motion,
            rtl,
            ..Default::default()
        },
        slides: vec![
            DesignedSlide::new(t(
                "Harbor: office work that stays on your device",
                "ميناء: عمل مكتبي يبقى على جهازك",
            ))
            .kicker(t("Pitch deck", "عرض تقديمي"))
            .bullets([t("Series A · October 2026", "الجولة أ · أكتوبر ٢٠٢٦")]),
            DesignedSlide::new(t(
                "Every office tool sends your files somewhere else",
                "كل أداة مكتبية ترسل ملفاتك إلى مكان آخر",
            ))
            .kicker(t("Problem", "المشكلة"))
            .layout(LayoutKind::Statement),
            DesignedSlide::new(t(
                "Three ideas, one quiet app",
                "ثلاث أفكار في تطبيق هادئ واحد",
            ))
            .kicker(t("Solution", "الحل"))
            .blocks(vec![
                Block::card(
                    t("Local first", "محلي أولاً"),
                    t(
                        "Documents never leave the device unless you say so.",
                        "لا تغادر المستندات الجهاز إلا بإذنك.",
                    ),
                ),
                Block::card(
                    t("Verified formulas", "صيغ موثّقة"),
                    t(
                        "Only functions proven against Excel are marked verified.",
                        "الدوال المثبتة فقط تُعلَّم موثّقة.",
                    ),
                ),
                Block::card(
                    t("One app", "تطبيق واحد"),
                    t(
                        "Documents, sheets, decks and PDF in one place.",
                        "مستندات وجداول وعروض وPDF في مكان واحد.",
                    ),
                ),
            ]),
            DesignedSlide::new(t("A market that is ready", "سوق جاهز"))
                .kicker(t("Market", "السوق"))
                .blocks(vec![
                    Block::stat("$4.2B", t("addressable market", "السوق المتاح")),
                    Block::stat("38%", t("yearly growth", "نمو سنوي")),
                    Block::stat("12M", t("teams on legacy suites", "فريق على الحزم القديمة")),
                    Block::stat("2.1x", t("pipeline coverage", "تغطية خط المبيعات")),
                ]),
            DesignedSlide::new(t("Revenue is compounding", "الإيرادات تتراكم"))
                .kicker(t("Traction", "الزخم"))
                .bullets([
                    t("Q3 up 24% on Q2", "الربع الثالث أعلى بـ ٢٤٪"),
                    t("Churn fell to 2.1%", "انخفض التسرب إلى ٢٫١٪"),
                ])
                .chart(ChartSpec {
                    kind: ChartKind::Bar,
                    title: t("Revenue ($M)", "الإيرادات (مليون)"),
                    categories: vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()],
                    series: vec![("2026".into(), vec![1.2, 1.9, 2.4, 3.1])],
                }),
            DesignedSlide::new(t("Build vs buy", "بناء أم شراء"))
                .kicker(t("Competition", "المنافسة"))
                .blocks(vec![
                    Block {
                        heading: t("Legacy suites", "الحزم القديمة"),
                        text: vec![
                            t("Cloud by default", "سحابية افتراضياً"),
                            t("Per-seat pricing", "تسعير لكل مقعد"),
                            t("Opaque formulas", "صيغ غامضة"),
                        ],
                        ..Default::default()
                    },
                    Block {
                        heading: t("Harbor", "ميناء"),
                        text: vec![
                            t("Local by default", "محلية افتراضياً"),
                            t("Flat pricing", "تسعير ثابت"),
                            t("Verified formulas", "صيغ موثّقة"),
                        ],
                        ..Default::default()
                    },
                ]),
            DesignedSlide::new(t(
                "From beta to scale in four quarters",
                "من التجربة إلى التوسع في أربعة أرباع",
            ))
            .kicker(t("Go to market", "الوصول للسوق"))
            .blocks(vec![
                Block::step(
                    "Q1",
                    t("Private beta", "تجربة خاصة"),
                    t("Ten design partners", "عشرة شركاء تصميم"),
                ),
                Block::step(
                    "Q2",
                    t("Public launch", "إطلاق عام"),
                    t("Self-serve onboarding", "تسجيل ذاتي"),
                ),
                Block::step(
                    "Q3",
                    t("Teams", "الفرق"),
                    t("Shared workspaces", "مساحات مشتركة"),
                ),
                Block::step(
                    "Q4",
                    t("EMEA", "المنطقة"),
                    t("Arabic first-class", "العربية بالكامل"),
                ),
            ]),
            DesignedSlide::new(t("The people building it", "الفريق الذي يبنيه"))
                .kicker(t("Team", "الفريق"))
                .layout(LayoutKind::Team)
                .blocks(vec![
                    Block {
                        heading: "Layla Hassan".into(),
                        text: vec![
                            t("CEO", "الرئيسة التنفيذية"),
                            t("Ex-Office product lead", "قائدة منتج سابقة"),
                        ],
                        ..Default::default()
                    },
                    Block {
                        heading: "Omar Saleh".into(),
                        text: vec![
                            t("CTO", "المدير التقني"),
                            t("Rust and local AI", "رست والذكاء المحلي"),
                        ],
                        ..Default::default()
                    },
                    Block {
                        heading: "Mina Costa".into(),
                        text: vec![
                            t("Design", "التصميم"),
                            t("Made Keynote feel human", "جعلت الواجهات إنسانية"),
                        ],
                        ..Default::default()
                    },
                ]),
            DesignedSlide::new(t("Customers say it plainly", "العملاء يقولونها ببساطة"))
                .layout(LayoutKind::Quote)
                .bullets([t(
                    "“It is the first office app I trust with client files.”",
                    "«إنه أول تطبيق مكتبي أثق به بملفات العملاء.»",
                )])
                .attribution(t("COO, regional law firm", "مديرة العمليات، شركة محاماة")),
            DesignedSlide::new(t("What we will do with the money", "ماذا سنفعل بالتمويل"))
                .layout(LayoutKind::Agenda)
                .bullets([
                    t("Ship teams and sync", "إطلاق الفرق والمزامنة"),
                    t("Qualify more formulas", "توثيق مزيد من الصيغ"),
                    t("Open the EMEA market", "فتح سوق المنطقة"),
                    t("Hire eight engineers", "توظيف ثمانية مهندسين"),
                ]),
            DesignedSlide::new(t("Part two: the plan", "الجزء الثاني: الخطة"))
                .layout(LayoutKind::Section)
                .bullets([t("Where we go from here", "إلى أين نتجه")]),
            DesignedSlide::new(t("Let's build it together", "لنبنيه معاً"))
                .bullets(["hello@harbor.dev", "harbor.dev"]),
        ],
    }
}

fn main() {
    let mut a = std::env::args().skip(1);
    let theme = a.next().expect("theme or all");
    let motion = MotionPreset::parse(&a.next().expect("motion")).expect("motion id");
    let out = a.next().expect("out dir");
    let rtl = a.next().as_deref() == Some("rtl");
    std::fs::create_dir_all(&out).unwrap();
    let ids: Vec<&str> = if theme == "all" {
        THEMES.iter().map(|t| t.id).collect()
    } else {
        theme.split(',').collect()
    };
    for id in ids {
        let deck = sample(id, motion, rtl);
        let bytes = deck.to_pptx_bytes().expect("build");
        let path = format!("{out}/{id}{}.pptx", if rtl { "-rtl" } else { "" });
        std::fs::write(&path, bytes).unwrap();
        println!("{path}");
    }
}
