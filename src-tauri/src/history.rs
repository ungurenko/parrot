use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::AppHandle;

use crate::paths;

const MAX_ENTRIES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub source_name: String,
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
/// Format: "YYYY-MM-DDTHH:MM:SSZ". No chrono dep — use std::time.
pub fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_secs(secs)
}

fn format_unix_secs(secs: u64) -> String {
    // Days since 1970-01-01
    let mut days = (secs / 86_400) as i64;
    let mut rem = secs % 86_400;
    let hour = rem / 3600;
    rem %= 3600;
    let minute = rem / 60;
    let second = rem % 60;

    // Convert days to (year, month, day) using civil-from-days (Hinnant).
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = (days - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, m, d, hour, minute, second
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_unix_secs_matches_known_timestamps() {
        assert_eq!(format_unix_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix_secs(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(format_unix_secs(1_735_689_600), "2025-01-01T00:00:00Z");
    }
}
