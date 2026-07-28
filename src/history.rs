use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,
    pub command: String,
    pub query: String,
    pub directory: Option<String>,
}

pub fn record(command: &str, query: &str, directory: Option<&String>) -> Result<()> {
    if query.trim().is_empty() {
        return Ok(());
    }

    let connection = crate::state_db::open()?;
    record_with_connection(&connection, now_secs(), command, query, directory.map(String::as_str))
}

pub fn list() -> Result<Vec<HistoryEntry>> {
    let connection = crate::state_db::open()?;
    list_with_connection(&connection)
}

pub fn clear() -> Result<()> {
    let connection = crate::state_db::open()?;
    connection.execute("DELETE FROM history", [])?;
    Ok(())
}

pub fn format_entry(entry: &HistoryEntry) -> String {
    let directory = entry.directory.as_deref().unwrap_or(".");
    format!("{} [{}] {} {}", entry.timestamp, entry.command, directory, entry.query)
}

fn record_with_connection(
    connection: &Connection,
    timestamp: u64,
    command: &str,
    query: &str,
    directory: Option<&str>,
) -> Result<()> {
    connection.execute(
        "INSERT INTO history (timestamp, command, query, directory) VALUES (?1, ?2, ?3, ?4)",
        params![
            sqlite_integer(timestamp, "history timestamp")?,
            command,
            query,
            directory
        ],
    )?;
    Ok(())
}

fn list_with_connection(connection: &Connection) -> Result<Vec<HistoryEntry>> {
    let mut statement = connection.prepare(
        "SELECT timestamp, command, query, directory
         FROM history
         ORDER BY timestamp DESC, id DESC",
    )?;
    let entries = statement
        .query_map([], |row| {
            Ok(HistoryEntry {
                timestamp: row.get::<_, i64>(0)? as u64,
                command: row.get(1)?,
                query: row.get(2)?,
                directory: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(entries)
}

fn sqlite_integer(value: u64, field: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| AppError::General(format!("{field} exceeds SQLite INTEGER range")))
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

    fn temp_database(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("fcs_history_{name}_{}", std::process::id()));
        (root.join("state.sqlite3"), root)
    }

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

    #[test]
    fn persists_lists_and_clears_history_in_sqlite() {
        let (path, root) = temp_database("persistence");
        let _ = std::fs::remove_dir_all(&root);
        let connection = crate::state_db::open_at(&path).unwrap();

        record_with_connection(&connection, 1, "search", "first", Some("src")).unwrap();
        record_with_connection(&connection, 2, "files", "second", None).unwrap();

        let entries = list_with_connection(&connection).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].query, "second");
        assert_eq!(entries[1].directory.as_deref(), Some("src"));

        connection.execute("DELETE FROM history", []).unwrap();
        assert!(list_with_connection(&connection).unwrap().is_empty());
        drop(connection);
        let _ = std::fs::remove_dir_all(&root);
    }
}
