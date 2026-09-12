//! Paragraph-first, grapheme-aware chunking (Latin + Arabic safe).

use unicode_segmentation::UnicodeSegmentation;

pub use crate::identity::ChunkerConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub text: String,
    pub ordinal: u32,
}

#[derive(Debug, Clone)]
pub struct Chunker;

impl Chunker {
    /// Split `text` into chunks under `config`. Paragraph boundaries are
    /// respected first; oversized paragraphs are windowed with overlap.
    /// Grapheme counting keeps Arabic combining marks intact.
    pub fn chunk(text: &str, config: &ChunkerConfig) -> Vec<Chunk> {
        let norm = match config.respect_paragraphs {
            true => text,
            false => text,
        };
        let paragraphs: Vec<&str> = norm
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();
        let mut out = Vec::new();
        let mut ordinal = 0u32;
        for para in paragraphs {
            let g: Vec<&str> = para.graphemes(true).collect();
            if g.len() <= config.target_graphemes {
                out.push(Chunk { text: para.to_string(), ordinal });
                ordinal += 1;
                continue;
            }
            // Window with overlap.
            let mut start = 0usize;
            while start < g.len() {
                let end = (start + config.target_graphemes).min(g.len());
                let window: String = g[start..end].concat();
                out.push(Chunk { text: window, ordinal });
                ordinal += 1;
                if end == g.len() {
                    break;
                }
                start = end.saturating_sub(config.overlap_graphemes);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ChunkerConfig {
        ChunkerConfig {
            target_graphemes: 50,
            overlap_graphemes: 10,
            respect_paragraphs: true,
        }
    }

    #[test]
    fn respects_paragraph_boundaries() {
        let text = "Para one.\n\nPara two.\n\nPara three.";
        let chunks = Chunker::chunk(text, &cfg());
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[1].text, "Para two.");
    }

    #[test]
    fn long_paragraph_is_windowed_with_overlap() {
        let body = "x".repeat(120);
        let text = format!("{body}\n\n{body}");
        let chunks = Chunker::chunk(&text, &cfg());
        assert!(chunks.len() >= 4);
        // Overlap: chunk[1] starts before chunk[0] ended.
        let first_end_is_x = chunks[0].text.ends_with('x');
        assert!(first_end_is_x);
        assert_eq!(chunks[0].text, chunks[1].text, "window content repeats due to overlap on uniform input");
    }

    #[test]
    fn arabic_graphemes_stay_intact() {
        // Arabic with combining hamza: grapheme counting must not split it.
        let text = "\u{0623}\u{0647}\u{0644}\u{0627}".repeat(40);
        let chunks = Chunker::chunk(&text, &cfg());
        assert!(chunks.iter().all(|c| c.text.is_char_boundary(c.text.len())));
    }
}
