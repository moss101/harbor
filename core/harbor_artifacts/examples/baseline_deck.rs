use harbor_artifacts::{PptxDeck, SlideContent, DeckStyle};
fn main() {
    let out = std::env::args().nth(1).expect("out path");
    let deck = PptxDeck { title: "Baseline".into(), slides: vec![
        SlideContent { title: "Harbor Q3 review".into(), bullets: vec!["Prepared for the board".into()], notes: None, chart: None, image: None },
        SlideContent { title: "Revenue grew 24%".into(), bullets: vec!["EMEA led growth".into(), "Churn fell to 2.1%".into(), "Three enterprise wins".into()], notes: Some("notes".into()), chart: None, image: None },
    ]};
    std::fs::write(out, deck.to_pptx_bytes_with(DeckStyle { first_slide_is_cover: true }).unwrap()).unwrap();
}
