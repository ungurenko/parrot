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
    (s.contains("youtube.com/") || s.contains("youtu.be/"))
        && (s.starts_with("http://") || s.starts_with("https://"))
}
