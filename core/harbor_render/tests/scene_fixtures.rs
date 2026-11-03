//! Scene fixtures produced by the real reader, consumed by the Flutter
//! painter's tests (`packages/harbor_ui/test/slides`). Regenerate with
//! `HARBOR_UPDATE_FIXTURES=1 cargo test -p harbor_render --test scene_fixtures`;
//! without it the test fails if a stored fixture has drifted from what the
//! reader now produces — the contract between the two sides stays honest.

use harbor_artifacts::{ChartKind, ChartSpec, DeckStyle, PptxDeck, SlideContent};

fn legacy_deck() -> Vec<u8> {
    PptxDeck {
        title: "Q3 review".into(),
        slides: vec![
            SlideContent {
                title: "Harbor Q3 review".into(),
                bullets: vec!["Prepared for the board".into()],
                notes: None,
                chart: None,
                image: None,
            },
            SlideContent {
                title: "Revenue grew 24%".into(),
                bullets: vec!["EMEA led growth".into(), "Churn fell to 2.1%".into()],
                notes: Some("Say it slowly.".into()),
                chart: None,
                image: None,
            },
            SlideContent {
                title: "Quarterly revenue".into(),
                bullets: vec![],
                notes: None,
                chart: Some(ChartSpec {
                    kind: ChartKind::Bar,
                    title: "Revenue".into(),
                    categories: vec!["Q1".into(), "Q2".into(), "Q3".into()],
                    series: vec![("2026".into(), vec![10.0, 14.0, 18.0])],
                }),
                image: None,
            },
        ],
    }
    .to_pptx_bytes_with(DeckStyle {
        first_slide_is_cover: true,
    })
    .unwrap()
}

fn check(name: &str, deck: &[u8]) {
    let scene = harbor_render::deck_scene(deck).unwrap();
    let json = serde_json::to_string_pretty(&scene).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    if std::env::var("HARBOR_UPDATE_FIXTURES").is_ok() {
        std::fs::write(&path, &json).unwrap();
        return;
    }
    let stored = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing {name}; run with HARBOR_UPDATE_FIXTURES=1"));
    assert_eq!(
        stored, json,
        "{name} drifted; rerun with HARBOR_UPDATE_FIXTURES=1"
    );
}

#[test]
fn legacy_deck_scene_fixture_is_current() {
    check("scene_legacy.json", &legacy_deck());
}
