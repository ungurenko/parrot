// Keep well below the 4,096-token output ceiling. Four thousand source
// characters leave ample room for a potentially longer Russian translation.
pub const TRANSLATION_CHUNK_CHARS: usize = 4_000;

#[derive(Debug)]
struct TranslationChunk {
    text: String,
    separator: String,
}

pub fn split_translation_chunks(text: &str, max_chars: usize) -> Vec<String> {
    split_translation_segments(text, max_chars)
        .into_iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator))
        .collect()
}

fn split_translation_segments(text: &str, max_chars: usize) -> Vec<TranslationChunk> {
    if text.is_empty() || max_chars == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let remaining = &text[start..];
        let Some((limit_offset, _)) = remaining.char_indices().nth(max_chars) else {
            chunks.push(TranslationChunk {
                text: remaining.to_string(),
                separator: String::new(),
            });
            break;
        };
        let limit = start + limit_offset;
        let window = &text[start..limit];
        let (content_end, next_start) = preferred_boundary(window)
            .map(|(content, next)| (start + content, start + next))
            .unwrap_or((limit, limit));

        if content_end == start {
            chunks.push(TranslationChunk {
                text: text[start..limit].to_string(),
                separator: String::new(),
            });
            start = limit;
            continue;
        }

        chunks.push(TranslationChunk {
            text: text[start..content_end].to_string(),
            separator: text[content_end..next_start].to_string(),
        });
        start = next_start;
    }
    chunks
}

fn preferred_boundary(window: &str) -> Option<(usize, usize)> {
    let mut paragraph = None;
    let mut sentence = None;
    let mut whitespace = None;
    let mut cursor = 0;

    while cursor < window.len() {
        let ch = window[cursor..].chars().next()?;
        if !ch.is_whitespace() {
            cursor += ch.len_utf8();
            continue;
        }

        let separator_start = cursor;
        cursor += ch.len_utf8();
        while cursor < window.len() {
            let next = window[cursor..].chars().next()?;
            if !next.is_whitespace() {
                break;
            }
            cursor += next.len_utf8();
        }
        if separator_start == 0 {
            continue;
        }

        let boundary = (separator_start, cursor);
        let separator = &window[separator_start..cursor];
        if separator.matches('\n').count() >= 2 {
            paragraph = Some(boundary);
        } else if window[..separator_start]
            .chars()
            .next_back()
            .is_some_and(|ch| matches!(ch, '.' | '!' | '?' | '…'))
        {
            sentence = Some(boundary);
        } else {
            whitespace = Some(boundary);
        }
    }

    paragraph.or(sentence).or(whitespace)
}

pub fn translate_text_with<F, P>(
    text: &str,
    max_chars: usize,
    cancel: &crate::cancellation::CancelToken,
    mut generate: F,
    mut report_progress: P,
) -> anyhow::Result<String>
where
    F: FnMut(&str, usize, usize) -> anyhow::Result<String>,
    P: FnMut(usize, usize),
{
    let chunks = split_translation_segments(text, max_chars);
    let total = chunks.len();
    report_progress(0, total);

    let mut translated = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        if cancel.is_cancelled() {
            anyhow::bail!("cancelled");
        }
        let output = generate(&chunk.text, index + 1, total)?;
        if cancel.is_cancelled() {
            anyhow::bail!("cancelled");
        }
        let output = output.trim();
        if output.is_empty() {
            anyhow::bail!("Модель перевода вернула пустой ответ");
        }
        translated.push_str(output);
        translated.push_str(&chunk.separator);
        report_progress(index + 1, total);
    }

    Ok(translated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cancellation::CancelToken;

    #[test]
    fn short_text_stays_in_one_chunk() {
        assert_eq!(
            split_translation_chunks("Hello world", 8_000),
            vec!["Hello world"]
        );
    }

    #[test]
    fn long_text_splits_without_losing_any_input() {
        let text = format!(
            "{}\n\n{}",
            "First sentence. ".repeat(350),
            "Second paragraph. ".repeat(350)
        );

        let chunks = split_translation_chunks(&text, 8_000);

        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 8_000));
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn oversized_word_uses_a_safe_character_boundary() {
        let text = "я".repeat(8_050);

        let chunks = split_translation_chunks(&text, 8_000);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn translation_runs_every_chunk_and_reports_completed_parts() {
        let token = CancelToken::new();
        let mut progress = Vec::new();

        let translated = translate_text_with(
            "One two three four",
            8,
            token.as_ref(),
            |chunk, part, total| Ok(format!("{part}/{total}:{}", chunk.trim())),
            |completed, total| progress.push((completed, total)),
        )
        .unwrap();

        assert_eq!(translated, "1/3:One two 2/3:three 3/3:four");
        assert_eq!(progress, vec![(0, 3), (1, 3), (2, 3), (3, 3)]);
    }

    #[test]
    fn long_single_paragraph_does_not_gain_blank_lines() {
        let token = CancelToken::new();
        let text = "Sentence one. Sentence two. Sentence three.";

        let translated = translate_text_with(
            text,
            16,
            token.as_ref(),
            |chunk, _part, _total| Ok(chunk.trim().to_uppercase()),
            |_completed, _total| {},
        )
        .unwrap();

        assert!(!translated.contains("\n\n"));
        assert_eq!(translated, text.to_uppercase());
    }

    #[test]
    fn cancellation_between_chunks_discards_the_result() {
        let token = CancelToken::new();
        let token_for_generator = token.clone();
        let mut calls = 0;

        let error = translate_text_with(
            "One two three four",
            8,
            token.as_ref(),
            |_chunk, _part, _total| {
                calls += 1;
                token_for_generator.cancel();
                Ok("часть".to_string())
            },
            |_completed, _total| {},
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "cancelled");
        assert_eq!(calls, 1);
    }
}
