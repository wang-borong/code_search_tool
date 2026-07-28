use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::types::{Type, Value};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};

use super::{
    ArchivedTraceSession, TraceEntry, TraceEntryChange, TraceEntryFilter, TraceMetadata, TraceRecordResult,
    TraceSessionChange, TraceSessionEditReport, TraceSessionSummary, TraceStore,
};
use crate::core::Location;
use crate::errors::{AppError, Result};

const ACTIVE_SESSION_KEY: &str = "active_trace_session";
const ENTRY_COLUMNS: &str = "id, timestamp, workspace, kind, label, path, line, column, note, status, priority, session, parent, branch, tags_json";

#[derive(Clone, Copy)]
enum EntryOrder {
    NewestFirst,
    Sequence,
}

pub(super) fn record(
    root: Option<&Path>,
    location: &Location,
    label: &str,
    kind: &str,
    metadata: TraceMetadata,
    timestamp: u64,
    deduplicate: bool,
) -> Result<TraceRecordResult> {
    let entry = new_entry(root, location, label, kind, metadata, timestamp);
    let mut connection = crate::state_db::open()?;
    record_with_connection(&mut connection, &entry, deduplicate)
}

#[cfg(test)]
fn record_at(path: &Path, entry: &TraceEntry, deduplicate: bool) -> Result<TraceRecordResult> {
    let mut connection = crate::state_db::open_at(path)?;
    record_with_connection(&mut connection, entry, deduplicate)
}

fn new_entry(
    root: Option<&Path>,
    location: &Location,
    label: &str,
    kind: &str,
    metadata: TraceMetadata,
    timestamp: u64,
) -> TraceEntry {
    TraceEntry {
        id: String::new(),
        timestamp,
        workspace: root.map(Path::to_path_buf),
        kind: kind.to_string(),
        label: label.to_string(),
        path: location.path.clone(),
        line: location.line,
        column: location.column,
        note: metadata.note,
        status: metadata.status,
        priority: metadata.priority,
        session: metadata.session,
        parent: metadata.parent,
        branch: metadata.branch,
        tags: metadata.tags,
    }
}

fn record_with_connection(
    connection: &mut Connection,
    entry: &TraceEntry,
    deduplicate: bool,
) -> Result<TraceRecordResult> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

    if deduplicate {
        if let Some(id) = duplicate_entry_id(&transaction, entry)? {
            transaction.commit()?;
            return Ok(TraceRecordResult { id, inserted: false });
        }
    }

    let id = insert_entry(&transaction, entry, None)?;
    transaction.commit()?;
    Ok(TraceRecordResult { id, inserted: true })
}

pub(super) fn list(root: Option<&Path>, filter: &TraceEntryFilter) -> Result<Vec<TraceEntry>> {
    let connection = crate::state_db::open()?;
    query_entries(&connection, root, filter, EntryOrder::NewestFirst)
}

#[cfg(test)]
pub(super) fn list_at(path: &Path, root: Option<&Path>, filter: &TraceEntryFilter) -> Result<Vec<TraceEntry>> {
    let connection = crate::state_db::open_at(path)?;
    query_entries(&connection, root, filter, EntryOrder::NewestFirst)
}

pub(super) fn list_sessions(include_archived: bool) -> Result<Vec<TraceSessionSummary>> {
    let connection = crate::state_db::open()?;
    let mut sessions = BTreeMap::<String, TraceSessionSummary>::new();

    let mut statement = connection.prepare(
        "SELECT COALESCE(NULLIF(session, ''), 'default'), COUNT(*), MIN(timestamp), MAX(timestamp)
         FROM trace_entries
         GROUP BY COALESCE(NULLIF(session, ''), 'default')",
    )?;
    for row in statement.query_map([], |row| {
        Ok(TraceSessionSummary {
            name: row.get(0)?,
            entries: row.get::<_, i64>(1)? as usize,
            first_timestamp: row.get::<_, i64>(2)? as u64,
            last_timestamp: row.get::<_, i64>(3)? as u64,
            archived_at: None,
            branches: Vec::new(),
            tags: Vec::new(),
        })
    })? {
        let summary = row?;
        sessions.insert(summary.name.clone(), summary);
    }

    let mut branch_statement = connection.prepare(
        "SELECT DISTINCT COALESCE(NULLIF(session, ''), 'default'), branch
         FROM trace_entries
         WHERE branch IS NOT NULL AND branch != ''
         ORDER BY 1, 2",
    )?;
    for row in branch_statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
        let (name, branch) = row?;
        if let Some(summary) = sessions.get_mut(&name) {
            summary.branches.push(branch);
        }
    }

    let mut tag_statement = connection.prepare(
        "SELECT DISTINCT COALESCE(NULLIF(entries.session, ''), 'default'), tags.tag
         FROM trace_entries AS entries
         JOIN trace_tags AS tags ON tags.trace_seq = entries.seq
         WHERE tags.tag != ''
         ORDER BY 1, 2",
    )?;
    for row in tag_statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
        let (name, tag) = row?;
        if let Some(summary) = sessions.get_mut(&name) {
            summary.tags.push(tag);
        }
    }

    for archived in archived_sessions_with_connection(&connection)? {
        let summary = sessions
            .entry(archived.name.clone())
            .or_insert_with(|| TraceSessionSummary {
                name: archived.name.clone(),
                entries: 0,
                first_timestamp: 0,
                last_timestamp: 0,
                archived_at: None,
                branches: Vec::new(),
                tags: Vec::new(),
            });
        summary.archived_at = Some(archived.archived_at);
    }

    let mut summaries = sessions
        .into_values()
        .filter(|summary| include_archived || !summary.is_archived())
        .collect::<Vec<_>>();
    summaries.sort_by_key(|summary| {
        (
            summary.is_archived(),
            Reverse(summary.last_timestamp),
            summary.name.clone(),
        )
    });
    Ok(summaries)
}

pub(super) fn archived_sessions() -> Result<Vec<ArchivedTraceSession>> {
    let connection = crate::state_db::open()?;
    archived_sessions_with_connection(&connection)
}

pub(super) fn active_session() -> Result<Option<String>> {
    let connection = crate::state_db::open()?;
    connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [ACTIVE_SESSION_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
}

pub(super) fn set_active_session(name: &str) -> Result<()> {
    let connection = crate::state_db::open()?;
    connection.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![ACTIVE_SESSION_KEY, name],
    )?;
    Ok(())
}

pub(super) fn archive_session(name: &str, archived_at: u64) -> Result<TraceSessionChange> {
    let mut connection = crate::state_db::open()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !session_exists(&transaction, name)? {
        transaction.commit()?;
        return Ok(TraceSessionChange::NotFound);
    }
    let changed = transaction.execute(
        "INSERT OR IGNORE INTO trace_archived_sessions (name, archived_at) VALUES (?1, ?2)",
        params![name, sqlite_integer(archived_at, "archive timestamp")?],
    )?;
    transaction.commit()?;
    Ok(if changed == 0 {
        TraceSessionChange::Unchanged
    } else {
        TraceSessionChange::Changed
    })
}

pub(super) fn unarchive_session(name: &str) -> Result<TraceSessionChange> {
    let mut connection = crate::state_db::open()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !session_exists(&transaction, name)? {
        transaction.commit()?;
        return Ok(TraceSessionChange::NotFound);
    }
    let changed = transaction.execute("DELETE FROM trace_archived_sessions WHERE name = ?1", [name])?;
    transaction.commit()?;
    Ok(if changed == 0 {
        TraceSessionChange::Unchanged
    } else {
        TraceSessionChange::Changed
    })
}

pub(super) fn rename_session(from: &str, to: &str) -> Result<TraceSessionEditReport> {
    let mut connection = crate::state_db::open()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed_entries = if from == "default" {
        transaction.execute(
            "UPDATE trace_entries SET session = ?1 WHERE session IS NULL OR session = ''",
            [to],
        )?
    } else {
        transaction.execute(
            "UPDATE trace_entries SET session = ?1 WHERE session = ?2",
            params![to, from],
        )?
    };
    if changed_entries == 0 {
        return Err(AppError::General(format!("Trace session not found: {from}")));
    }

    let archived_at = transaction.query_row(
        "SELECT MIN(archived_at) FROM trace_archived_sessions WHERE name = ?1 OR name = ?2",
        params![from, to],
        |row| row.get::<_, Option<i64>>(0),
    )?;
    transaction.execute(
        "DELETE FROM trace_archived_sessions WHERE name = ?1 OR name = ?2",
        params![from, to],
    )?;
    if let Some(archived_at) = archived_at {
        transaction.execute(
            "INSERT INTO trace_archived_sessions (name, archived_at) VALUES (?1, ?2)",
            params![to, archived_at],
        )?;
    }
    transaction.execute(
        "UPDATE app_settings SET value = ?1 WHERE key = ?2 AND value = ?3",
        params![to, ACTIVE_SESSION_KEY, from],
    )?;
    transaction.commit()?;
    Ok(TraceSessionEditReport {
        changed_entries,
        removed_entries: 0,
        created_session: Some(to.to_string()),
    })
}

pub(super) fn split_session_by_tag(from: &str, tag: &str, to: &str) -> Result<TraceSessionEditReport> {
    let connection = crate::state_db::open()?;
    let sql = if from == "default" {
        "UPDATE trace_entries AS entries
         SET session = ?1
         WHERE (session IS NULL OR session = '')
           AND EXISTS (
               SELECT 1 FROM trace_tags AS tags
               WHERE tags.trace_seq = entries.seq AND tags.tag = ?2
           )"
    } else {
        "UPDATE trace_entries AS entries
         SET session = ?1
         WHERE session = ?2
           AND EXISTS (
               SELECT 1 FROM trace_tags AS tags
               WHERE tags.trace_seq = entries.seq AND tags.tag = ?3
           )"
    };
    let changed_entries = if from == "default" {
        connection.execute(sql, params![to, tag])?
    } else {
        connection.execute(sql, params![to, from, tag])?
    };
    if changed_entries == 0 {
        return Err(AppError::General(format!(
            "No entries in session {from} matched tag {tag}"
        )));
    }
    Ok(TraceSessionEditReport {
        changed_entries,
        removed_entries: 0,
        created_session: Some(to.to_string()),
    })
}

pub(super) fn update_entry_value(selector: &str, column: &str, value: Option<&str>) -> Result<TraceEntryChange> {
    let column = match column {
        "note" => "note",
        "status" => "status",
        "priority" => "priority",
        _ => return Err(AppError::General(format!("Unsupported trace entry column: {column}"))),
    };
    let connection = crate::state_db::open()?;
    let sql = if selector == "latest" {
        format!(
            "UPDATE trace_entries SET {column} = ?1
             WHERE seq = (SELECT seq FROM trace_entries ORDER BY timestamp DESC, seq DESC LIMIT 1)"
        )
    } else {
        format!("UPDATE trace_entries SET {column} = ?1 WHERE id = ?2")
    };
    let changed = if selector == "latest" {
        connection.execute(&sql, [value])?
    } else {
        connection.execute(&sql, params![value, selector])?
    };
    Ok(if changed == 0 {
        TraceEntryChange::NotFound
    } else {
        TraceEntryChange::Changed
    })
}

pub(super) fn clear() -> Result<()> {
    let mut connection = crate::state_db::open()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute("DELETE FROM trace_entries", [])?;
    transaction.execute("DELETE FROM trace_archived_sessions", [])?;
    transaction.execute("DELETE FROM app_settings WHERE key = ?1", [ACTIVE_SESSION_KEY])?;
    transaction.commit()?;
    Ok(())
}

pub(super) fn load_store() -> Result<TraceStore> {
    let connection = crate::state_db::open()?;
    load_store_with_connection(&connection)
}

#[cfg(test)]
pub(super) fn load_store_at(path: &Path) -> Result<TraceStore> {
    let connection = crate::state_db::open_at(path)?;
    load_store_with_connection(&connection)
}

pub(super) fn replace_store(store: &TraceStore) -> Result<()> {
    let mut connection = crate::state_db::open()?;
    replace_store_with_connection(&mut connection, store)
}

#[cfg(test)]
pub(super) fn replace_store_at(path: &Path, store: &TraceStore) -> Result<()> {
    let mut connection = crate::state_db::open_at(path)?;
    replace_store_with_connection(&mut connection, store)
}

fn query_entries(
    connection: &Connection,
    root: Option<&Path>,
    filter: &TraceEntryFilter,
    order: EntryOrder,
) -> Result<Vec<TraceEntry>> {
    let mut clauses = Vec::new();
    let mut values = Vec::<Value>::new();

    if let Some(root) = root {
        clauses.push("(workspace = ? OR workspace IS NULL)");
        values.push(Value::Text(path_text(root)));
    }
    if let Some(session) = filter.session.as_deref() {
        if session == "default" {
            clauses.push("(session IS NULL OR session = '')");
        } else {
            clauses.push("session = ?");
            values.push(Value::Text(session.to_string()));
        }
    }
    if let Some(tag) = filter.tag.as_deref() {
        clauses.push(
            "EXISTS (SELECT 1 FROM trace_tags AS tags WHERE tags.trace_seq = trace_entries.seq AND tags.tag = ?)",
        );
        values.push(Value::Text(tag.to_string()));
    }
    for (column, value) in [
        ("kind", filter.kind.as_deref()),
        ("status", filter.status.as_deref()),
        ("priority", filter.priority.as_deref()),
    ] {
        if let Some(value) = value {
            clauses.push(match column {
                "kind" => "kind = ?",
                "status" => "status = ?",
                "priority" => "priority = ?",
                _ => unreachable!(),
            });
            values.push(Value::Text(value.to_string()));
        }
    }

    let where_clause = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let order_clause = match order {
        EntryOrder::NewestFirst => "timestamp DESC, seq ASC",
        EntryOrder::Sequence => "seq ASC",
    };
    let sql = format!("SELECT {ENTRY_COLUMNS} FROM trace_entries{where_clause} ORDER BY {order_clause}");
    let mut statement = connection.prepare(&sql)?;
    let entries = statement
        .query_map(params_from_iter(values.iter()), trace_entry_from_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    if filter.relation.is_some() {
        Ok(entries
            .into_iter()
            .filter(|entry| super::trace_entry_matches_filter(entry, filter))
            .collect())
    } else {
        Ok(entries)
    }
}

fn load_store_with_connection(connection: &Connection) -> Result<TraceStore> {
    let entries = query_entries(connection, None, &TraceEntryFilter::default(), EntryOrder::Sequence)?;
    let archived_sessions = archived_sessions_with_connection(connection)?;
    let active_session = connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [ACTIVE_SESSION_KEY],
            |row| row.get(0),
        )
        .optional()?;
    Ok(TraceStore {
        entries,
        archived_sessions,
        active_session,
    })
}

fn replace_store_with_connection(connection: &mut Connection, store: &TraceStore) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute("DELETE FROM trace_entries", [])?;
    transaction.execute("DELETE FROM trace_archived_sessions", [])?;
    transaction.execute("DELETE FROM app_settings WHERE key = ?1", [ACTIVE_SESSION_KEY])?;

    for entry in &store.entries {
        let id = (!entry.id.trim().is_empty()).then_some(entry.id.as_str());
        insert_entry(&transaction, entry, id)?;
    }
    for session in &store.archived_sessions {
        transaction.execute(
            "INSERT INTO trace_archived_sessions (name, archived_at) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET archived_at = MIN(archived_at, excluded.archived_at)",
            params![session.name, sqlite_integer(session.archived_at, "archive timestamp")?],
        )?;
    }
    if let Some(active_session) = store.active_session.as_deref() {
        transaction.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
            params![ACTIVE_SESSION_KEY, active_session],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn insert_entry(transaction: &Transaction<'_>, entry: &TraceEntry, existing_id: Option<&str>) -> Result<String> {
    let tags_json = serde_json::to_string(&entry.tags).map_err(|error| AppError::General(error.to_string()))?;
    transaction.execute(
        "INSERT INTO trace_entries (
             id, timestamp, workspace, kind, label, path, line, column, note, status, priority, session, parent, branch,
             tags_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            existing_id,
            sqlite_integer(entry.timestamp, "trace timestamp")?,
            entry.workspace.as_deref().map(path_text),
            entry.kind,
            entry.label,
            path_text(&entry.path),
            optional_sqlite_integer(entry.line, "trace line")?,
            optional_sqlite_integer(entry.column, "trace column")?,
            entry.note,
            entry.status,
            entry.priority,
            entry.session,
            entry.parent,
            entry.branch,
            tags_json,
        ],
    )?;
    let seq = transaction.last_insert_rowid();
    let id = existing_id
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}-{seq}", entry.timestamp));
    if existing_id.is_none() {
        transaction.execute("UPDATE trace_entries SET id = ?1 WHERE seq = ?2", params![id, seq])?;
    }

    let mut tag_statement =
        transaction.prepare("INSERT INTO trace_tags (trace_seq, position, tag) VALUES (?1, ?2, ?3)")?;
    for (position, tag) in entry.tags.iter().enumerate() {
        tag_statement.execute(params![
            seq,
            optional_sqlite_integer(Some(position), "trace tag position")?,
            tag
        ])?;
    }
    drop(tag_statement);
    Ok(id)
}

fn duplicate_entry_id(transaction: &Transaction<'_>, entry: &TraceEntry) -> Result<Option<String>> {
    let tags_json = serde_json::to_string(&entry.tags).map_err(|error| AppError::General(error.to_string()))?;
    transaction
        .query_row(
            "SELECT id
             FROM trace_entries
             WHERE workspace IS ?1
               AND kind = ?2
               AND label = ?3
               AND path = ?4
               AND line IS ?5
               AND column IS ?6
               AND session IS ?7
               AND parent IS ?8
               AND branch IS ?9
               AND tags_json = ?10
             ORDER BY seq
             LIMIT 1",
            params![
                entry.workspace.as_deref().map(path_text),
                entry.kind,
                entry.label,
                path_text(&entry.path),
                optional_sqlite_integer(entry.line, "trace line")?,
                optional_sqlite_integer(entry.column, "trace column")?,
                entry.session,
                entry.parent,
                entry.branch,
                tags_json,
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
}

fn archived_sessions_with_connection(connection: &Connection) -> Result<Vec<ArchivedTraceSession>> {
    let mut statement = connection.prepare("SELECT name, archived_at FROM trace_archived_sessions ORDER BY name")?;
    let sessions = statement
        .query_map([], |row| {
            Ok(ArchivedTraceSession {
                name: row.get(0)?,
                archived_at: row.get::<_, i64>(1)? as u64,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(sessions)
}

fn session_exists(connection: &Connection, name: &str) -> Result<bool> {
    let count: i64 = if name == "default" {
        connection.query_row(
            "SELECT COUNT(*) FROM trace_entries WHERE session IS NULL OR session = ''",
            [],
            |row| row.get(0),
        )?
    } else {
        connection.query_row("SELECT COUNT(*) FROM trace_entries WHERE session = ?1", [name], |row| {
            row.get(0)
        })?
    };
    Ok(count > 0)
}

fn trace_entry_from_row(row: &Row<'_>) -> rusqlite::Result<TraceEntry> {
    let tags_json = row.get::<_, String>(14)?;
    let tags = serde_json::from_str(&tags_json)
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(14, Type::Text, Box::new(error)))?;
    Ok(TraceEntry {
        id: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
        timestamp: row.get::<_, i64>(1)? as u64,
        workspace: row.get::<_, Option<String>>(2)?.map(Into::into),
        kind: row.get(3)?,
        label: row.get(4)?,
        path: row.get::<_, String>(5)?.into(),
        line: row.get::<_, Option<i64>>(6)?.map(|value| value as usize),
        column: row.get::<_, Option<i64>>(7)?.map(|value| value as usize),
        note: row.get(8)?,
        status: row.get(9)?,
        priority: row.get(10)?,
        session: row.get(11)?,
        parent: row.get(12)?,
        branch: row.get(13)?,
        tags,
    })
}

fn sqlite_integer(value: u64, field: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| AppError::General(format!("{field} exceeds SQLite INTEGER range")))
}

fn optional_sqlite_integer(value: Option<usize>, field: &str) -> Result<Option<i64>> {
    value
        .map(|value| {
            i64::try_from(value).map_err(|_| AppError::General(format!("{field} exceeds SQLite INTEGER range")))
        })
        .transpose()
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn sqlite_filter_uses_normalized_tags_and_default_session() {
        let root = std::env::temp_dir().join(format!("fcs_trace_storage_{}", std::process::id()));
        let path = root.join("state.sqlite3");
        let _ = std::fs::remove_dir_all(&root);
        let mut connection = crate::state_db::open_at(&path).unwrap();
        let store = TraceStore {
            entries: vec![TraceEntry {
                id: "trace-1".to_string(),
                timestamp: 1,
                workspace: None,
                kind: "bookmark".to_string(),
                label: "main".to_string(),
                path: PathBuf::from("src/main.rs"),
                line: Some(1),
                column: None,
                note: None,
                status: Some("open".to_string()),
                priority: None,
                session: None,
                parent: None,
                branch: None,
                tags: vec!["hot".to_string()],
            }],
            archived_sessions: Vec::new(),
            active_session: None,
        };
        replace_store_with_connection(&mut connection, &store).unwrap();

        let entries = query_entries(
            &connection,
            None,
            &TraceEntryFilter {
                session: Some("default".to_string()),
                tag: Some("hot".to_string()),
                status: Some("open".to_string()),
                ..TraceEntryFilter::default()
            },
            EntryOrder::NewestFirst,
        )
        .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "trace-1");
        drop(connection);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sqlite_record_generates_unique_ids_and_deduplicates_atomically() {
        let root = std::env::temp_dir().join(format!("fcs_trace_record_{}", std::process::id()));
        let path = root.join("state.sqlite3");
        let _ = std::fs::remove_dir_all(&root);
        let location = Location::new(PathBuf::from("src/main.rs"), Some(7), Some(2));
        let metadata = TraceMetadata {
            session: Some("bug-42".to_string()),
            tags: vec!["hot".to_string()],
            ..TraceMetadata::default()
        };

        let first_entry = new_entry(None, &location, "main", "bookmark", metadata.clone(), 10);
        let duplicate_entry = new_entry(None, &location, "main", "bookmark", metadata.clone(), 11);
        let second_entry = new_entry(None, &location, "main", "bookmark", metadata, 10);
        let first = record_at(&path, &first_entry, true).unwrap();
        let duplicate = record_at(&path, &duplicate_entry, true).unwrap();
        let second = record_at(&path, &second_entry, false).unwrap();

        assert!(first.inserted);
        assert!(!duplicate.inserted);
        assert_eq!(duplicate.id, first.id);
        assert!(second.inserted);
        assert_ne!(second.id, first.id);
        assert_eq!(list_at(&path, None, &TraceEntryFilter::default()).unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }
}
