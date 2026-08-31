use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::cancellation::CancelToken;

/// Save text to `<save_dir>/<stem>.txt`. If the file already exists, append ` (N)` before .txt.
pub fn save_text(save_dir: &Path, stem: &str, text: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(save_dir)?;
    let safe = sanitize_stem(stem);
    let mut candidate = save_dir.join(format!("{safe}.txt"));
    let mut n: u32 = 2;
    while candidate.exists() {
        candidate = save_dir.join(format!("{safe} ({n}).txt"));
        n += 1;
    }
    std::fs::write(&candidate, text)?;
    Ok(candidate)
}

/// Save a Markdown summary next to the transcript: `<stem>.summary.md`.
/// Derives save directory and stem from the transcript's saved .txt path.
/// Overwrites if file already exists (re-generation replaces the previous summary).
pub fn save_summary(transcript_path: &Path, summary: &str) -> Result<PathBuf> {
    let dir = transcript_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("не удалось определить папку из пути конспекта"))?;
    let stem = transcript_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("transcript");
    std::fs::create_dir_all(dir)?;
    let candidate = dir.join(format!("{stem}.summary.md"));
    std::fs::write(&candidate, summary)?;
    Ok(candidate)
}

/// Save a Russian translation next to the transcript without exposing a
/// partially-written destination file.
pub fn save_translation(
    transcript_path: &Path,
    translation: &str,
    cancel: &CancelToken,
) -> Result<PathBuf> {
    let destination = translation_path(transcript_path)?;
    let dir = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("не удалось определить папку из пути перевода"))?;
    std::fs::create_dir_all(dir)?;
    let temporary = destination.with_extension("txt.tmp");
    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }
    std::fs::write(&temporary, translation)?;
    if cancel.is_cancelled() {
        let _ = std::fs::remove_file(&temporary);
        anyhow::bail!("cancelled");
    }
    if let Err(error) = std::fs::rename(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(destination)
}

pub fn translation_path(transcript_path: &Path) -> Result<PathBuf> {
    let dir = transcript_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("не удалось определить папку из пути перевода"))?;
    let stem = transcript_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("transcript");
    Ok(dir.join(format!("{stem}.translation.ru.txt")))
}

/// Strip control chars and filesystem-hostile characters from a filename stem.
fn sanitize_stem(stem: &str) -> String {
    let trimmed = stem.trim();
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => out.push('_'),
            c if c.is_control() => out.push('_'),
            c => out.push(c),
        }
    }
    let result = out.trim().trim_matches('.').to_string();
    if result.is_empty() {
        "transcript".to_string()
    } else if result.len() > 120 {
        result.chars().take(120).collect()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_removes_hostile_chars() {
        assert_eq!(sanitize_stem("foo/bar\\baz:qux"), "foo_bar_baz_qux");
        assert_eq!(sanitize_stem("  name  "), "name");
        assert_eq!(sanitize_stem(""), "transcript");
    }

    #[test]
    fn translation_is_saved_atomically_next_to_transcript() {
        let dir = std::env::temp_dir().join(format!("parrot-translation-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join("interview.txt");
        std::fs::write(&transcript, "Hello").unwrap();

        let token = crate::cancellation::CancelToken::new();
        let output = save_translation(&transcript, "Привет", token.as_ref()).unwrap();

        assert_eq!(output, dir.join("interview.translation.ru.txt"));
        assert_eq!(std::fs::read_to_string(output).unwrap(), "Привет");
        assert!(!dir.join("interview.translation.ru.txt.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn canceled_translation_keeps_the_previous_file() {
        let dir =
            std::env::temp_dir().join(format!("parrot-translation-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join("interview.txt");
        let destination = dir.join("interview.translation.ru.txt");
        std::fs::write(&transcript, "Hello").unwrap();
        std::fs::write(&destination, "Старый перевод").unwrap();
        let token = crate::cancellation::CancelToken::new();
        token.cancel();

        let error = save_translation(&transcript, "Новый перевод", token.as_ref()).unwrap_err();

        assert_eq!(error.to_string(), "cancelled");
        assert_eq!(
            std::fs::read_to_string(destination).unwrap(),
            "Старый перевод"
        );
        assert!(!dir.join("interview.translation.ru.txt.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
