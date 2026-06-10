use std::cmp::Reverse;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,
    pub command: String,
    pub query: String,
    pub directory: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HistoryStore {
    entries: Vec<HistoryEntry>,
}

pub fn record(command: &str, query: &str, directory: Option<&String>) -> Result<()> {
    if query.trim().is_empty() {
        return Ok(());
    }

    let mut store = load_store()?;
    store.entries.push(HistoryEntry {
        timestamp: now_secs(),
        command: command.to_string(),
        query: query.to_string(),
        directory: directory.cloned(),
    });
    save_store(&store)
}

pub fn list() -> Result<Vec<HistoryEntry>> {
    let mut entries = load_store()?.entries;
    entries.sort_by_key(|entry| Reverse(entry.timestamp));
    Ok(entries)
}

pub fn clear() -> Result<()> {
    save_store(&HistoryStore::default())
}

pub fn format_entry(entry: &HistoryEntry) -> String {
    let directory = entry.directory.as_deref().unwrap_or(".");
    format!("{} [{}] {} {}", entry.timestamp, entry.command, directory, entry.query)
}

fn load_store() -> Result<HistoryStore> {
    let path = history_path()?;
    if !path.exists() {
        return Ok(HistoryStore::default());
    }

    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents).map_err(|e| AppError::General(e.to_string()))
}

fn save_store(store: &HistoryStore) -> Result<()> {
    let path = history_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(store).map_err(|e| AppError::General(e.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn history_path() -> Result<PathBuf> {
    Ok(crate::cache::user_cache_dir()?.join("history.toml"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_history_entry() {
        let entry = HistoryEntry {
            timestamp: 1,
            command: "search".to_string(),
            query: "main".to_string(),
            directory: Some("src".to_string()),
        };

        assert_eq!(format_entry(&entry), "1 [search] src main");
    }
}
