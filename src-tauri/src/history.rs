use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::AppHandle;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::paths;

const MAX_ENTRIES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub source_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_value: Option<String>,
    pub engine: String,
    pub language: String,
    /// ISO 8601 UTC timestamp, e.g. "2026-04-22T14:30:00Z".
    pub created_at: String,
    pub output_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_path: Option<String>,
}

pub fn load(app: &AppHandle) -> Vec<HistoryEntry> {
    let Ok(path) = paths::history_path(app) else {
        return Vec::new();
    };
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<HistoryEntry>>(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, entries: &[HistoryEntry]) -> Result<()> {
    let path = paths::history_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(entries)?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// Add a new entry to the top of the list; truncate to MAX_ENTRIES.
pub fn append(app: &AppHandle, entry: HistoryEntry) -> Result<()> {
    let mut entries = load(app);
    // If an entry with the same id already exists, replace it (idempotent).
    entries.retain(|e| e.id != entry.id);
    entries.insert(0, entry);
    if entries.len() > MAX_ENTRIES {
        entries.truncate(MAX_ENTRIES);
    }
    save(app, &entries)
}

pub fn attach_summary(app: &AppHandle, id: &str, summary_path: &Path) -> Result<()> {
    let mut entries = load(app);
    let mut changed = false;
    for entry in entries.iter_mut() {
        if entry.id == id {
            entry.summary_path = Some(summary_path.to_string_lossy().to_string());
            changed = true;
            break;
        }
    }
    if changed {
        save(app, &entries)?;
    }
    Ok(())
}

pub fn remove(app: &AppHandle, id: &str) -> Result<()> {
    let mut entries = load(app);
    entries.retain(|e| e.id != id);
    save(app, &entries)
}

pub fn clear(app: &AppHandle) -> Result<()> {
    save(app, &[])
}

pub fn get(app: &AppHandle, id: &str) -> Option<HistoryEntry> {
    load(app).into_iter().find(|e| e.id == id)
}

/// UTC ISO-8601 timestamp for "now".
/// Format: "YYYY-MM-DDTHH:MM:SSZ".
pub fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_secs(secs)
}

fn format_unix_secs(secs: u64) -> String {
    let timestamp = i64::try_from(secs).unwrap_or(0);
    OffsetDateTime::from_unix_timestamp(timestamp)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_entry_should_load_legacy_entries_without_repeat_source() {
        let json = r#"[
          {
            "id": "old",
            "sourceName": "meeting.m4a",
            "engine": "parakeet",
            "language": "auto",
            "createdAt": "2026-06-20T09:00:00Z",
            "outputPath": "/tmp/meeting.txt"
          }
        ]"#;

        let entries: Vec<HistoryEntry> = serde_json::from_str(json).expect("legacy history");

        assert_eq!(entries[0].source_kind, None);
        assert_eq!(entries[0].source_value, None);
    }

    #[test]
    fn history_entry_should_store_repeat_source_for_new_entries() {
        let entry = HistoryEntry {
            id: "new".to_string(),
            source_name: "video title".to_string(),
            source_kind: Some("youtube".to_string()),
            source_value: Some("https://youtu.be/abc".to_string()),
            engine: "qwen-0.6b".to_string(),
            language: "ru".to_string(),
            created_at: "2026-06-20T09:00:00Z".to_string(),
            output_path: "/tmp/video.txt".to_string(),
            summary_path: None,
        };

        let json = serde_json::to_string(&entry).expect("history json");

        assert!(json.contains("\"sourceKind\":\"youtube\""));
        assert!(json.contains("\"sourceValue\":\"https://youtu.be/abc\""));
    }

    #[test]
    fn format_unix_secs_matches_known_timestamps() {
        assert_eq!(format_unix_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix_secs(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(format_unix_secs(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(format_unix_secs(1_735_689_600), "2025-01-01T00:00:00Z");
    }
}
