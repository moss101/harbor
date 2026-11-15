//! Reasoning ("thinking") blocks are the model's scratch work, not its
//! answer. Hybrid reasoning models (Qwen 3, LFM, DeepSeek-R1 distills,
//! MiniCPM) wrap it in `<think>…</think>` — sometimes empty, sometimes with
//! the opening tag already in the prompt template — and a person must never
//! see it as the answer.

const TAGS: &[(&str, &str)] = &[("<think>", "</think>"), ("<thinking>", "</thinking>")];

/// The answer with every reasoning block removed, trimmed.
///
/// - Closed blocks are removed wherever they appear.
/// - A closing tag with no opening tag (the template opened the block)
///   drops everything before it.
/// - An opening tag that never closes (generation ran out of tokens mid
///   thought) drops everything from it on: half a thought is not an answer.
pub fn strip_reasoning(text: &str) -> String {
    let mut out = text.to_string();
    for (open, close) in TAGS {
        if let Some(end) = out.find(close) {
            if !out[..end].contains(open) {
                out = out[end + close.len()..].to_string();
            }
        }
        while let Some(start) = out.find(open) {
            match out[start..].find(close) {
                Some(rel) => out.replace_range(start..start + rel + close.len(), ""),
                None => out.truncate(start),
            }
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::strip_reasoning;

    #[test]
    fn empty_block_from_a_hybrid_model_is_removed() {
        assert_eq!(
            strip_reasoning("<think></think>3-7*5 equals -32."),
            "3-7*5 equals -32."
        );
    }

    #[test]
    fn reasoning_is_removed_and_the_answer_kept() {
        assert_eq!(
            strip_reasoning("<think>\n7*5=35, 3-35=-32\n</think>\n\nIt is -32."),
            "It is -32."
        );
        assert_eq!(
            strip_reasoning("<thinking>scratch</thinking>Answer"),
            "Answer"
        );
    }

    #[test]
    fn template_opened_block_drops_the_text_before_its_close() {
        assert_eq!(strip_reasoning("scratch work</think>Answer"), "Answer");
    }

    #[test]
    fn unfinished_thought_is_not_an_answer() {
        assert_eq!(strip_reasoning("<think>still thinking about"), "");
        assert_eq!(strip_reasoning("Answer <think>more"), "Answer");
    }

    #[test]
    fn plain_text_is_untouched_except_trim() {
        assert_eq!(
            strip_reasoning("  Paris is the capital.\n"),
            "Paris is the capital."
        );
        assert_eq!(strip_reasoning("{\"a\":1}"), "{\"a\":1}");
    }
}
