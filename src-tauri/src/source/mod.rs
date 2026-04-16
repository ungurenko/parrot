pub mod local;
pub mod youtube;

use std::path::Path;

pub const AUDIO_EXTS: &[&str] = &["mp3", "wav", "m4a", "flac", "ogg", "opus", "aac", "wma"];
pub const VIDEO_EXTS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v"];

pub fn is_supported_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let lo = ext.to_ascii_lowercase();
            AUDIO_EXTS.contains(&lo.as_str()) || VIDEO_EXTS.contains(&lo.as_str())
        }
        None => false,
    }
}

pub fn is_youtube_url(s: &str) -> bool {
    let s = s.trim();
    let Ok(url) = reqwest::Url::parse(s) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    matches!(
        url.host_str(),
        Some(
            "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_youtube_url_should_accept_real_youtube_hosts() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_youtube_url("https://youtu.be/abc"));
    }

    #[test]
    fn is_youtube_url_should_reject_lookalike_hosts() {
        assert!(!is_youtube_url(
            "https://example.com/watch?next=youtube.com/watch"
        ));
        assert!(!is_youtube_url("https://youtube.com.evil.test/watch"));
    }
}
