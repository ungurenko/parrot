use anyhow::Result;
use std::path::{Path, PathBuf};

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
}
