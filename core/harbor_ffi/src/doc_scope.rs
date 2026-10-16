//! Document-scoped grounding for `op.start_generate` ("Ask about this
//! file"): the host supplies the passages of ONE open document and the
//! answer is grounded only on them — the Knowledge index is never
//! searched.
//!
//! Everything here is pure (no model, no workspace) so validation,
//! passage selection and citation shape are unit-testable in every build.
//! The only model-dependent piece — embedding scores — is injected by the
//! caller as a closure; without it (no embedding model open, or it
//! failed) selection falls back to lexical BM25.
//!
//! Budget parity with the Knowledge path: that path grounds on the top 3
//! chunks of at most 800 graphemes each, so the document path keeps the
//! evidence to the same ~2400 characters (each passage capped at 800),
//! which keeps the prompt inside the same context the chat model already
//! serves for Ask.

use serde_json::Value;

/// Most passages one request may carry.
pub const MAX_PASSAGES: usize = 5_000;
/// Total UTF-8 bytes of passage text one request may carry.
pub const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;
/// UTF-8 bytes of one passage's text.
pub const MAX_PASSAGE_BYTES: usize = 64 * 1024;
/// UTF-8 bytes of the document name / a passage id / a locator.
pub const MAX_LABEL_BYTES: usize = 512;
/// Evidence characters per passage (= the Knowledge chunk size).
pub const PASSAGE_EVIDENCE_CHARS: usize = 800;
/// Total evidence characters (= 3 Knowledge chunks).
pub const EVIDENCE_CHAR_BUDGET: usize = 3 * PASSAGE_EVIDENCE_CHARS;
/// Most passages cited in one answer.
pub const MAX_SELECTED: usize = 8;
/// Passages embedded for semantic re-ranking (lexical pre-filter beyond).
pub const EMBED_CANDIDATES: usize = 64;
/// Characters of a citation snippet.
pub const SNIPPET_CHARS: usize = 240;

/// One host-supplied passage of the open document.
#[derive(Debug, Clone, PartialEq)]
pub struct Passage {
    pub id: String,
    pub text: String,
    pub locator: Option<String>,
}

/// The validated `document` argument.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopedDocument {
    pub name: String,
    pub passages: Vec<Passage>,
}

/// How the cited passages were chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ranking {
    /// The whole document fits the budget: every passage, document order.
    All,
    /// BM25 over the question (no embedding model, or it failed).
    Lexical,
    /// Embedding cosine (lexical pre-filter for very long documents).
    Embedding,
}

impl Ranking {
    pub fn as_str(self) -> &'static str {
        match self {
            Ranking::All => "all",
            Ranking::Lexical => "lexical",
            Ranking::Embedding => "embedding",
        }
    }
}

fn label(v: Option<&Value>, what: &str) -> Result<Option<String>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            if s.len() > MAX_LABEL_BYTES {
                return Err(format!("{what} exceeds {MAX_LABEL_BYTES} bytes"));
            }
            Ok(Some(s.clone()))
        }
        Some(_) => Err(format!("{what} must be a string")),
    }
}

/// Validate the `document` argument. Whitespace-only passages are
/// skipped (extractors emit blank paragraphs); a document with no usable
/// passage is an error, never a silent ungrounded answer.
pub fn parse_document(v: &Value) -> Result<ScopedDocument, String> {
    let obj = v
        .as_object()
        .ok_or("document must be an object with name and passages")?;
    let name = label(obj.get("name"), "document.name")?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("document.name must be a non-empty string")?;
    let raw = obj
        .get("passages")
        .and_then(|p| p.as_array())
        .ok_or("document.passages must be an array")?;
    if raw.len() > MAX_PASSAGES {
        return Err(format!(
            "document has {} passages; at most {MAX_PASSAGES} are accepted",
            raw.len()
        ));
    }
    let mut total = 0usize;
    let mut passages = Vec::with_capacity(raw.len());
    for (i, p) in raw.iter().enumerate() {
        let p = p
            .as_object()
            .ok_or_else(|| format!("document.passages[{i}] must be an object"))?;
        let text = p
            .get("text")
            .and_then(|t| t.as_str())
            .ok_or_else(|| format!("document.passages[{i}].text must be a string"))?;
        if text.len() > MAX_PASSAGE_BYTES {
            return Err(format!(
                "document.passages[{i}].text exceeds {MAX_PASSAGE_BYTES} bytes"
            ));
        }
        total += text.len();
        if total > MAX_TOTAL_BYTES {
            return Err(format!(
                "document passages exceed {MAX_TOTAL_BYTES} bytes in total"
            ));
        }
        let id = label(p.get("id"), &format!("document.passages[{i}].id"))?
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("p{i}"));
        let locator = label(p.get("locator"), &format!("document.passages[{i}].locator"))?;
        if text.trim().is_empty() {
            continue;
        }
        passages.push(Passage {
            id,
            text: text.trim().to_string(),
            locator,
        });
    }
    if passages.is_empty() {
        return Err("document has no passage with text".into());
    }
    Ok(ScopedDocument { name, passages })
}

/// Lowercased alphanumeric terms (script-agnostic: Arabic letters are
/// alphanumeric too). Single-character terms are noise for ranking.
pub fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() > 1)
        .map(|t| t.to_lowercase())
        .collect()
}

/// Okapi BM25 (k1 = 1.2, b = 0.75) of each passage against the question.
pub fn bm25_scores(question: &str, passages: &[Passage]) -> Vec<f32> {
    use std::collections::{HashMap, HashSet};
    let q: HashSet<String> = terms(question).into_iter().collect();
    if q.is_empty() || passages.is_empty() {
        return vec![0.0; passages.len()];
    }
    let docs: Vec<Vec<String>> = passages.iter().map(|p| terms(&p.text)).collect();
    let n = docs.len() as f32;
    let avgdl = (docs.iter().map(|d| d.len()).sum::<usize>() as f32 / n).max(1.0);
    let mut df: HashMap<&str, usize> = HashMap::new();
    for d in &docs {
        let uniq: HashSet<&str> = d.iter().map(|s| s.as_str()).collect();
        for t in uniq {
            if q.contains(t) {
                *df.entry(t).or_default() += 1;
            }
        }
    }
    let (k1, b) = (1.2f32, 0.75f32);
    docs.iter()
        .map(|d| {
            let mut tf: HashMap<&str, usize> = HashMap::new();
            for t in d {
                if q.contains(t.as_str()) {
                    *tf.entry(t.as_str()).or_default() += 1;
                }
            }
            let dl = d.len() as f32;
            tf.iter()
                .map(|(t, &f)| {
                    let dft = df[t] as f32;
                    let idf = ((n - dft + 0.5) / (dft + 0.5) + 1.0).ln();
                    let f = f as f32;
                    idf * f * (k1 + 1.0) / (f + k1 * (1.0 - b + b * dl / avgdl))
                })
                // fold from +0.0: an empty f32 `sum()` is -0.0.
                .fold(0.0f32, |a, x| a + x)
        })
        .collect()
}

/// Indices ordered by score (descending), ties in document order.
pub fn rank_indices(scores: &[f32]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..scores.len()).collect();
    idx.sort_by(|&a, &b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    idx
}

fn evidence_len(p: &Passage) -> usize {
    p.text.chars().count().min(PASSAGE_EVIDENCE_CHARS)
}

/// Greedy fill in rank order: take each passage whose evidence still fits
/// the budget (smaller later passages may fill the remainder), up to
/// [`MAX_SELECTED`]. The best passage is always taken (truncated if it
/// alone exceeds the budget).
pub fn select_within_budget(passages: &[Passage], ranked: &[usize]) -> Vec<usize> {
    let mut used = 0usize;
    let mut out = Vec::new();
    for &i in ranked {
        if out.len() >= MAX_SELECTED {
            break;
        }
        let len = evidence_len(&passages[i]);
        if out.is_empty() || used + len <= EVIDENCE_CHAR_BUDGET {
            used += len.min(EVIDENCE_CHAR_BUDGET);
            out.push(i);
        }
    }
    out
}

/// Whether every passage fits the budget as-is.
pub fn fits_whole(passages: &[Passage]) -> bool {
    passages.len() <= MAX_SELECTED
        && passages.iter().map(evidence_len).sum::<usize>() <= EVIDENCE_CHAR_BUDGET
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// Embedding scorer injected by the caller: given passage texts, return
/// `(query vector, one vector per passage)`.
pub type Embed<'a> = &'a dyn Fn(&[&str]) -> Result<(Vec<f32>, Vec<Vec<f32>>), String>;

/// Choose the evidence for `question`: `(passage index, score in 0..=1)`
/// in prompt order, plus how it was ranked.
pub fn select(
    doc: &ScopedDocument,
    question: &str,
    embed: Option<Embed<'_>>,
) -> (Vec<(usize, f32)>, Ranking) {
    let ps = &doc.passages;
    let lex = bm25_scores(question, ps);
    let max = lex.iter().cloned().fold(0.0f32, f32::max);
    let lex_norm: Vec<f32> = lex
        .iter()
        .map(|s| if max > 0.0 { s / max } else { 0.0 })
        .collect();
    if fits_whole(ps) {
        return (
            (0..ps.len()).map(|i| (i, lex_norm[i])).collect(),
            Ranking::All,
        );
    }
    let lex_ranked = rank_indices(&lex);
    if let Some(embed) = embed {
        let candidates: Vec<usize> = lex_ranked.iter().take(EMBED_CANDIDATES).copied().collect();
        let texts: Vec<&str> = candidates.iter().map(|&i| ps[i].text.as_str()).collect();
        if let Ok((q, vs)) = embed(&texts) {
            if vs.len() == candidates.len() {
                let mut scores = vec![f32::MIN; ps.len()];
                for (&i, v) in candidates.iter().zip(&vs) {
                    scores[i] = cosine(&q, v);
                }
                let ranked: Vec<usize> = rank_indices(&scores)
                    .into_iter()
                    .take(candidates.len())
                    .collect();
                let picked = select_within_budget(ps, &ranked);
                return (
                    picked
                        .into_iter()
                        .map(|i| (i, scores[i].clamp(0.0, 1.0)))
                        .collect(),
                    Ranking::Embedding,
                );
            }
        }
    }
    let picked = select_within_budget(ps, &lex_ranked);
    (
        picked.into_iter().map(|i| (i, lex_norm[i])).collect(),
        Ranking::Lexical,
    )
}

fn prefix_chars(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((cut, _)) => format!("{}…", &s[..cut]),
        None => s.to_string(),
    }
}

/// Citations in the Knowledge path's shape (`source_id`, `title`,
/// `chunk_id`, `score`, `state`, `content_hash`, `_text`) so the Ask UI
/// renders them unchanged, plus `passage_id`, `locator` and `snippet`.
/// `_text` (the evidence handed to the model) is capped at
/// [`PASSAGE_EVIDENCE_CHARS`].
pub fn citations(doc: &ScopedDocument, selected: &[(usize, f32)]) -> Vec<Value> {
    selected
        .iter()
        .map(|&(i, score)| {
            let p = &doc.passages[i];
            let evidence = match p.text.char_indices().nth(PASSAGE_EVIDENCE_CHARS) {
                Some((cut, _)) => &p.text[..cut],
                None => p.text.as_str(),
            };
            serde_json::json!({
                "source_id": format!("document:{}", doc.name),
                "title": doc.name,
                "chunk_id": p.id,
                "passage_id": p.id,
                "locator": p.locator,
                "score": score,
                "state": "Current",
                "content_hash": harbor_canonical::sha256_hex(p.text.as_bytes()),
                "snippet": prefix_chars(&p.text, SNIPPET_CHARS),
                "_text": evidence,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(texts: &[&str]) -> ScopedDocument {
        ScopedDocument {
            name: "Q3 report.docx".into(),
            passages: texts
                .iter()
                .enumerate()
                .map(|(i, t)| Passage {
                    id: format!("p{i}"),
                    text: t.to_string(),
                    locator: Some(format!("¶{i}")),
                })
                .collect(),
        }
    }

    #[test]
    fn parses_valid_document_and_skips_blank_passages() {
        let d = parse_document(&json!({
            "name": " Q3 report.docx ",
            "passages": [
                {"id": "p1", "text": "Revenue grew 12%.", "locator": "¶1"},
                {"text": "   "},
                {"text": "No id or locator."}
            ]
        }))
        .unwrap();
        assert_eq!(d.name, "Q3 report.docx");
        assert_eq!(d.passages.len(), 2);
        assert_eq!(d.passages[0].locator.as_deref(), Some("¶1"));
        assert_eq!(d.passages[1].id, "p2", "default id is the input position");
        assert_eq!(d.passages[1].locator, None);
    }

    #[test]
    fn rejects_malformed_documents() {
        let bad = [
            json!("not an object"),
            json!({"passages": [{"text": "x"}]}),
            json!({"name": "", "passages": [{"text": "x"}]}),
            json!({"name": "a", "passages": "nope"}),
            json!({"name": "a", "passages": ["text"]}),
            json!({"name": "a", "passages": [{"text": 3}]}),
            json!({"name": "a", "passages": [{"id": 3, "text": "x"}]}),
            json!({"name": "a", "passages": [{"text": "x", "locator": {}}]}),
            json!({"name": "a", "passages": []}),
            json!({"name": "a", "passages": [{"text": " \n "}]}),
        ];
        for b in bad {
            assert!(parse_document(&b).is_err(), "{b}");
        }
    }

    #[test]
    fn enforces_caps() {
        let many: Vec<Value> = (0..=MAX_PASSAGES).map(|_| json!({"text": "x"})).collect();
        let e = parse_document(&json!({"name": "a", "passages": many})).unwrap_err();
        assert!(e.contains("passages"), "{e}");
        let big = "y".repeat(MAX_PASSAGE_BYTES + 1);
        assert!(parse_document(&json!({"name": "a", "passages": [{"text": big}]})).is_err());
        let chunk = "z".repeat(MAX_PASSAGE_BYTES);
        let n = MAX_TOTAL_BYTES / MAX_PASSAGE_BYTES + 1;
        let total: Vec<Value> = (0..n).map(|_| json!({"text": chunk})).collect();
        let e = parse_document(&json!({"name": "a", "passages": total})).unwrap_err();
        assert!(e.contains("in total"), "{e}");
        let long_name = "n".repeat(MAX_LABEL_BYTES + 1);
        assert!(parse_document(&json!({"name": long_name, "passages": [{"text": "x"}]})).is_err());
    }

    #[test]
    fn bm25_prefers_matching_passages() {
        let d = doc(&[
            "The office moved to Dubai in 2021.",
            "Quarterly revenue grew twelve percent on cloud sales.",
            "Headcount was flat.",
        ]);
        let s = bm25_scores("How much did revenue grow?", &d.passages);
        assert_eq!(rank_indices(&s)[0], 1);
        assert_eq!(s[2], 0.0);
        // Arabic terms are tokenised too.
        let ar = doc(&["الإيرادات نمت", "عدد الموظفين ثابت"]);
        let s = bm25_scores("كم نمت الإيرادات", &ar.passages);
        assert!(s[0] > s[1]);
    }

    #[test]
    fn small_document_is_used_whole_in_document_order() {
        let d = doc(&["alpha", "revenue beta", "gamma"]);
        let (sel, r) = select(&d, "revenue", None);
        assert_eq!(r, Ranking::All);
        assert_eq!(sel.iter().map(|s| s.0).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(sel[1].1, 1.0);
    }

    #[test]
    fn large_document_is_selected_within_budget() {
        let filler = "lorem ipsum dolor sit amet ".repeat(30); // ~810 chars
        let mut texts: Vec<String> = (0..40).map(|_| filler.clone()).collect();
        texts[27] = "The renewal deadline is 30 November for the Acme contract.".into();
        let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let d = doc(&refs);
        let (sel, r) = select(&d, "When is the Acme renewal deadline?", None);
        assert_eq!(r, Ranking::Lexical);
        assert_eq!(sel[0].0, 27, "the relevant passage leads");
        let used: usize = sel.iter().map(|&(i, _)| evidence_len(&d.passages[i])).sum();
        assert!(used <= EVIDENCE_CHAR_BUDGET, "{used}");
        assert!(sel.len() <= MAX_SELECTED);
    }

    #[test]
    fn embedding_ranking_wins_and_failure_falls_back() {
        let filler = "x".repeat(900);
        let mut texts: Vec<String> = (0..10).map(|i| format!("{filler} {i}")).collect();
        texts[4] = "semantic target".into();
        let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let d = doc(&refs);
        let embed = |ts: &[&str]| -> Result<(Vec<f32>, Vec<Vec<f32>>), String> {
            Ok((
                vec![1.0, 0.0],
                ts.iter()
                    .map(|t| {
                        if t.starts_with("semantic") {
                            vec![1.0, 0.0]
                        } else {
                            vec![0.0, 1.0]
                        }
                    })
                    .collect(),
            ))
        };
        let (sel, r) = select(&d, "unrelated words", Some(&embed));
        assert_eq!(r, Ranking::Embedding);
        assert_eq!(sel[0], (4, 1.0));
        let failing =
            |_: &[&str]| -> Result<(Vec<f32>, Vec<Vec<f32>>), String> { Err("no model".into()) };
        let (_, r) = select(&d, "unrelated words", Some(&failing));
        assert_eq!(r, Ranking::Lexical);
    }

    #[test]
    fn oversized_single_passage_is_still_cited_but_truncated() {
        let d = doc(&[&"w".repeat(5000), &"v".repeat(5000)]);
        let (sel, _) = select(&d, "anything", None);
        assert_eq!(sel.len(), 2, "two 800-char evidence windows fit 2400");
        let c = citations(&d, &sel);
        assert_eq!(
            c[0]["_text"].as_str().unwrap().chars().count(),
            PASSAGE_EVIDENCE_CHARS
        );
    }

    #[test]
    fn citations_carry_ui_and_document_fields() {
        let d = doc(&["Revenue grew 12%."]);
        let c = citations(&d, &[(0, 0.5)]);
        let c = &c[0];
        assert_eq!(c["title"], "Q3 report.docx");
        assert_eq!(c["source_id"], "document:Q3 report.docx");
        assert_eq!(c["passage_id"], "p0");
        assert_eq!(c["chunk_id"], "p0");
        assert_eq!(c["locator"], "¶0");
        assert_eq!(c["state"], "Current");
        assert_eq!(c["score"], 0.5);
        assert_eq!(c["snippet"], "Revenue grew 12%.");
        assert_eq!(c["_text"], "Revenue grew 12%.");
        // And the existing composer grounds on them (abstention escape
        // hatch included).
        #[cfg(feature = "gguf")]
        {
            let (ctx, used) =
                crate::knowledge::compose_rag_context("How much?", std::slice::from_ref(c));
            assert!(used);
            assert!(ctx.contains("[1] Revenue grew 12%."));
            assert!(ctx.contains("INSUFFICIENT_EVIDENCE"));
        }
    }

    #[test]
    fn snippet_truncates_on_char_boundary() {
        let d = doc(&[&"é".repeat(SNIPPET_CHARS + 10)]);
        let c = citations(&d, &[(0, 0.0)]);
        let s = c[0]["snippet"].as_str().unwrap();
        assert_eq!(s.chars().count(), SNIPPET_CHARS + 1);
        assert!(s.ends_with('…'));
    }
}
