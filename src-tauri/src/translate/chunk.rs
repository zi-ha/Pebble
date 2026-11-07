//! Splitting long mail bodies into request-sized pieces.
//!
//! Every vendor caps a single request (Baidu counts bytes, Tencent and
//! Volcengine count characters), so long mails are cut — preferring line breaks,
//! which keeps the bilingual pairing intact after the pieces are re-joined.

pub fn split_chunks(text: &str, max_chars: usize) -> Vec<&str> {
    if text.len() <= max_chars {
        return vec![text];
    }

    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut last_newline: Option<usize> = None;

    for (index, ch) in text.char_indices() {
        if index - start >= max_chars {
            let cut = last_newline
                .filter(|newline| *newline > start && *newline <= index)
                .unwrap_or(index);
            chunks.push(&text[start..cut]);
            start = cut;
        }
        if ch == '\n' {
            last_newline = Some(index + ch.len_utf8());
        }
    }

    if start < text.len() {
        chunks.push(&text[start..]);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_short_text_in_one_piece() {
        assert_eq!(split_chunks("Hello\nWorld", 100), vec!["Hello\nWorld"]);
    }

    #[test]
    fn splits_on_line_boundaries_without_losing_content() {
        let line = "x".repeat(40);
        let text = std::iter::repeat_n(line.as_str(), 20)
            .collect::<Vec<_>>()
            .join("\n");

        let chunks = split_chunks(&text, 100);

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= 100, "chunk exceeded the limit");
        }
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn hard_cuts_text_that_has_no_line_breaks() {
        let text = "y".repeat(250);
        let chunks = split_chunks(&text, 100);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks.concat(), text);
    }
}
