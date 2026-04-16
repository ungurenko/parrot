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

/// Strip control chars and filesystem-hostile characters from a filename stem.
pub fn sanitize_stem(stem: &str) -> String {
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
