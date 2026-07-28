use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use rusqlite::Connection;

use crate::errors::{AppError, Result};

const SCHEMA_VERSION: i64 = 1;
static DATABASE_PATH: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn open() -> Result<Connection> {
    open_at(&database_path()?)
}

pub(crate) fn open_at(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(path)?;
    initialize(&connection)?;
    Ok(connection)
}

pub(crate) fn database_path() -> Result<PathBuf> {
    if let Some(path) = DATABASE_PATH.get() {
        return Ok(path.clone());
    }

    let path = crate::cache::user_data_dir()?.join("state.sqlite3");
    Ok(DATABASE_PATH.get_or_init(|| path).clone())
}

fn initialize(connection: &Connection) -> Result<()> {
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;

    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(AppError::General(format!(
            "Unsupported fcs state database schema version {version}; this build supports up to {SCHEMA_VERSION}"
        )));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }

    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS history (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             timestamp INTEGER NOT NULL CHECK (timestamp >= 0),
             command TEXT NOT NULL,
             query TEXT NOT NULL,
             directory TEXT
         );
         CREATE INDEX IF NOT EXISTS history_timestamp_idx
             ON history(timestamp DESC, id DESC);

         CREATE TABLE IF NOT EXISTS trace_entries (
             seq INTEGER PRIMARY KEY AUTOINCREMENT,
             id TEXT UNIQUE,
             timestamp INTEGER NOT NULL CHECK (timestamp >= 0),
             workspace TEXT,
             kind TEXT NOT NULL,
             label TEXT NOT NULL,
             path TEXT NOT NULL,
             line INTEGER CHECK (line IS NULL OR line >= 0),
             column INTEGER CHECK (column IS NULL OR column >= 0),
             note TEXT,
             status TEXT,
             priority TEXT,
             session TEXT,
             parent TEXT,
             branch TEXT,
             tags_json TEXT NOT NULL DEFAULT '[]'
         );
         CREATE INDEX IF NOT EXISTS trace_timestamp_idx
             ON trace_entries(timestamp DESC, seq);
         CREATE INDEX IF NOT EXISTS trace_workspace_timestamp_idx
             ON trace_entries(workspace, timestamp DESC, seq);
         CREATE INDEX IF NOT EXISTS trace_session_timestamp_idx
             ON trace_entries(session, timestamp DESC, seq);
         CREATE INDEX IF NOT EXISTS trace_kind_idx ON trace_entries(kind);
         CREATE INDEX IF NOT EXISTS trace_status_idx ON trace_entries(status);
         CREATE INDEX IF NOT EXISTS trace_priority_idx ON trace_entries(priority);
         CREATE INDEX IF NOT EXISTS trace_parent_idx ON trace_entries(parent);
         CREATE INDEX IF NOT EXISTS trace_dedup_idx
             ON trace_entries(workspace, kind, path, line, column, label, session, parent, branch, tags_json);

         CREATE TABLE IF NOT EXISTS trace_tags (
             trace_seq INTEGER NOT NULL REFERENCES trace_entries(seq) ON DELETE CASCADE,
             position INTEGER NOT NULL CHECK (position >= 0),
             tag TEXT NOT NULL,
             PRIMARY KEY (trace_seq, position)
         );
         CREATE INDEX IF NOT EXISTS trace_tags_tag_idx ON trace_tags(tag, trace_seq);

         CREATE TABLE IF NOT EXISTS trace_archived_sessions (
             name TEXT PRIMARY KEY,
             archived_at INTEGER NOT NULL CHECK (archived_at >= 0)
         );

         CREATE TABLE IF NOT EXISTS app_settings (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )?;
    connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_current_schema() {
        let root = std::env::temp_dir().join(format!("fcs_state_db_test_{}", std::process::id()));
        let path = root.join("state.sqlite3");
        let _ = fs::remove_dir_all(&root);

        let connection = open_at(&path).unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let journal_mode: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('history', 'trace_entries')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(journal_mode, "wal");
        assert_eq!(table_count, 2);
        drop(connection);
        let _ = fs::remove_dir_all(&root);
    }
}
