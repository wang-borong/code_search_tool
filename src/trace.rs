use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::core::{CodeItem, Location};
use crate::errors::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEntry {
    #[serde(default)]
    pub id: String,
    pub timestamp: u64,
    #[serde(default)]
    pub workspace: Option<PathBuf>,
    pub kind: String,
    pub label: String,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TraceMetadata {
    pub session: Option<String>,
    pub parent: Option<String>,
    pub branch: Option<String>,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedTraceSession {
    pub name: String,
    pub archived_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSessionSummary {
    pub name: String,
    pub entries: usize,
    pub first_timestamp: u64,
    pub last_timestamp: u64,
    pub archived_at: Option<u64>,
    pub branches: Vec<String>,
    pub tags: Vec<String>,
}

impl TraceSessionSummary {
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSessionReport {
    pub summary: TraceSessionSummary,
    pub entries: Vec<TraceEntry>,
    pub timeline: Vec<TraceTimelineItem>,
    pub replay: Vec<TraceReplayStep>,
    pub structured: TraceStructuredReport,
    pub status_counts: BTreeMap<String, usize>,
    pub priority_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceReplayStep {
    pub step: usize,
    pub entry_id: String,
    pub timestamp: u64,
    pub elapsed_secs: u64,
    pub action: String,
    pub target: String,
    pub state: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceReplayPlan {
    pub session: String,
    pub entries: usize,
    pub commands: Vec<TraceReplayCommand>,
    pub debug_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceReplayCommand {
    pub step: usize,
    pub entry_id: String,
    pub kind: String,
    pub target: String,
    pub command: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceStructuredReport {
    pub hypotheses: Vec<TraceStructuredItem>,
    pub evidence: Vec<TraceStructuredItem>,
    pub conclusions: Vec<TraceStructuredItem>,
    pub open_questions: Vec<TraceStructuredItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceStructuredItem {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub note: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceTimelineItem {
    pub id: String,
    pub timestamp: u64,
    pub kind: String,
    pub label: String,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub note: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub parent: Option<String>,
    pub branch: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSessionDiff {
    pub left_session: String,
    pub right_session: String,
    pub unchanged: usize,
    pub only_left: Vec<TraceDiffEntry>,
    pub only_right: Vec<TraceDiffEntry>,
    pub changed: Vec<TraceDiffEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceDiffEntry {
    pub key: String,
    pub left: Option<TraceEntry>,
    pub right: Option<TraceEntry>,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceInsightsReport {
    pub session: String,
    pub entries: usize,
    pub status_counts: BTreeMap<String, usize>,
    pub priority_counts: BTreeMap<String, usize>,
    pub kind_counts: BTreeMap<String, usize>,
    pub hot_files: Vec<TraceInsightCount>,
    pub debug_events: Vec<TraceInsightEvent>,
    pub unresolved_entries: Vec<TraceInsightEvent>,
    pub nearest_symbols: Vec<TraceInsightSymbol>,
    pub suggested_next_steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceInsightCount {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceInsightEvent {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub status: Option<String>,
    pub priority: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceInsightSymbol {
    pub entry_id: String,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub symbol: String,
    pub kind: String,
    pub symbol_line: usize,
    pub distance: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceSessionChange {
    Changed,
    Unchanged,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceEntryChange {
    Changed,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceGraphFormat {
    Text,
    Json,
    Mermaid,
    Dot,
}

impl TraceGraphFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "text" | "graph" | "markdown" | "md" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "mermaid" | "mmd" => Ok(Self::Mermaid),
            "dot" | "graphviz" => Ok(Self::Dot),
            other => Err(AppError::General(format!(
                "Unsupported trace graph format: {other}. Use text, json, mermaid, or dot"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceRecordResult {
    pub id: String,
    pub inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceGraphReport {
    pub entries: usize,
    pub nodes: Vec<TraceGraphNode>,
    pub edges: Vec<TraceGraphEdge>,
    #[serde(default)]
    pub collapsed: Vec<TraceGraphCollapse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceGraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub session: String,
    pub note: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub branch: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceGraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceGraphCollapse {
    pub id: String,
    pub entries: usize,
    pub session: String,
    pub kind: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct TraceEntryFilter {
    pub session: Option<String>,
    pub tag: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub relation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TraceGraphOptions {
    pub filter: TraceEntryFilter,
    pub collapse_threshold: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceDiffFilter {
    All,
    Semantic,
    Bookmark,
    Debug,
}

impl TraceDiffFilter {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "all" | "" => Ok(Self::All),
            "semantic" | "sem" => Ok(Self::Semantic),
            "bookmark" | "bookmarks" => Ok(Self::Bookmark),
            "debug" | "dap" => Ok(Self::Debug),
            other => Err(AppError::General(format!(
                "Unsupported trace diff filter: {other}. Use all, semantic, bookmark, or debug"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceStoreCheck {
    pub ok: bool,
    pub entries: usize,
    pub duplicate_ids: Vec<String>,
    pub missing_ids: usize,
    pub dangling_parents: Vec<String>,
    pub missing_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceStoreRepairReport {
    pub before_entries: usize,
    pub after_entries: usize,
    pub assigned_ids: usize,
    pub removed_duplicate_ids: usize,
    pub removed_dangling_parents: usize,
    pub removed_archived_sessions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSessionEditReport {
    pub changed_entries: usize,
    pub removed_entries: usize,
    pub created_session: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TraceStore {
    #[serde(default)]
    entries: Vec<TraceEntry>,
    #[serde(default)]
    archived_sessions: Vec<ArchivedTraceSession>,
    #[serde(default)]
    active_session: Option<String>,
}

pub fn record_code_item(item: &CodeItem, kind: &str) -> Result<()> {
    record_location(&item.location, item.display_text(), kind)
}

pub fn record_location(location: &Location, label: &str, kind: &str) -> Result<()> {
    record_location_with_workspace(None, location, label, kind)
}

pub fn record_location_with_metadata(
    location: &Location,
    label: &str,
    kind: &str,
    metadata: TraceMetadata,
) -> Result<()> {
    record_location_with_workspace_and_metadata(None, location, label, kind, metadata).map(|_| ())
}

pub fn record_code_item_for_workspace(root: &Path, item: &CodeItem, kind: &str) -> Result<()> {
    record_location_with_workspace(Some(root), &item.location, item.display_text(), kind)
}

pub fn record_location_for_workspace(root: &Path, location: &Location, label: &str, kind: &str) -> Result<()> {
    record_location_with_workspace(Some(root), location, label, kind)
}

pub fn record_location_for_workspace_with_metadata(
    root: &Path,
    location: &Location,
    label: &str,
    kind: &str,
    metadata: TraceMetadata,
) -> Result<()> {
    record_location_with_workspace_and_metadata(Some(root), location, label, kind, metadata).map(|_| ())
}

pub fn record_location_for_workspace_with_metadata_and_id(
    root: &Path,
    location: &Location,
    label: &str,
    kind: &str,
    metadata: TraceMetadata,
) -> Result<String> {
    record_location_with_workspace_and_metadata(Some(root), location, label, kind, metadata)
}

pub fn record_location_for_workspace_with_metadata_dedup(
    root: &Path,
    location: &Location,
    label: &str,
    kind: &str,
    metadata: TraceMetadata,
) -> Result<TraceRecordResult> {
    record_location_with_workspace_and_metadata_dedup(Some(root), location, label, kind, metadata)
}

fn record_location_with_workspace(root: Option<&Path>, location: &Location, label: &str, kind: &str) -> Result<()> {
    record_location_with_workspace_and_metadata(root, location, label, kind, TraceMetadata::default()).map(|_| ())
}

fn record_location_with_workspace_and_metadata(
    root: Option<&Path>,
    location: &Location,
    label: &str,
    kind: &str,
    metadata: TraceMetadata,
) -> Result<String> {
    let mut store = load_store()?;
    let timestamp = now_secs();
    let id = format!("{}-{}", timestamp, store.entries.len() + 1);
    store.entries.push(TraceEntry {
        id: id.clone(),
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
    });
    save_store(&store)?;
    Ok(id)
}

fn record_location_with_workspace_and_metadata_dedup(
    root: Option<&Path>,
    location: &Location,
    label: &str,
    kind: &str,
    metadata: TraceMetadata,
) -> Result<TraceRecordResult> {
    let mut store = load_store()?;
    if let Some(existing) = duplicate_entry_id(&store.entries, root, location, label, kind, &metadata) {
        return Ok(TraceRecordResult {
            id: existing,
            inserted: false,
        });
    }

    let timestamp = now_secs();
    let id = format!("{}-{}", timestamp, store.entries.len() + 1);
    store.entries.push(TraceEntry {
        id: id.clone(),
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
    });
    save_store(&store)?;
    Ok(TraceRecordResult { id, inserted: true })
}

pub fn list() -> Result<Vec<TraceEntry>> {
    sorted_entries(load_store()?.entries)
}

pub fn list_for_workspace(root: &Path) -> Result<Vec<TraceEntry>> {
    entries_for_workspace(load_store()?.entries, root)
}

pub fn list_filtered(root: Option<&Path>, filter: &TraceEntryFilter) -> Result<Vec<TraceEntry>> {
    let entries = match root {
        Some(root) => list_for_workspace(root)?,
        None => list()?,
    };
    Ok(entries
        .into_iter()
        .filter(|entry| trace_entry_matches_filter(entry, filter))
        .collect())
}

pub fn list_sessions(include_archived: bool) -> Result<Vec<TraceSessionSummary>> {
    let store = load_store()?;
    Ok(summarize_sessions(&store.entries, &store.archived_sessions)
        .into_iter()
        .filter(|summary| include_archived || !summary.is_archived())
        .collect())
}

pub fn active_session() -> Result<Option<String>> {
    Ok(load_store()?.active_session)
}

pub fn set_active_session(name: &str) -> Result<()> {
    let name = normalize_session_value(name)
        .ok_or_else(|| AppError::General("Trace session name cannot be empty".to_string()))?;
    let mut store = load_store()?;
    store.active_session = Some(name);
    save_store(&store)
}

pub fn resolve_session(session: Option<String>) -> Result<Option<String>> {
    match session.and_then(|value| normalize_session_value(&value)) {
        Some(session) => Ok(Some(session)),
        None => active_session(),
    }
}

pub fn archive_session(name: &str) -> Result<TraceSessionChange> {
    let mut store = load_store()?;
    let change = archive_session_in_store(&mut store, name, now_secs());
    if change == TraceSessionChange::Changed {
        save_store(&store)?;
    }
    Ok(change)
}

pub fn unarchive_session(name: &str) -> Result<TraceSessionChange> {
    let mut store = load_store()?;
    let change = unarchive_session_in_store(&mut store, name);
    if change == TraceSessionChange::Changed {
        save_store(&store)?;
    }
    Ok(change)
}

pub fn rename_session(from: &str, to: &str) -> Result<TraceSessionEditReport> {
    let to = normalize_session_value(to)
        .ok_or_else(|| AppError::General("Trace session name cannot be empty".to_string()))?;
    let mut store = load_store()?;
    let mut changed_entries = 0;
    for entry in &mut store.entries {
        if session_name(entry) == from {
            entry.session = Some(to.clone());
            changed_entries += 1;
        }
    }
    if changed_entries == 0 {
        return Err(AppError::General(format!("Trace session not found: {from}")));
    }
    for archived in &mut store.archived_sessions {
        if archived.name == from {
            archived.name = to.clone();
        }
    }
    dedupe_archived_sessions(&mut store.archived_sessions);
    if store.active_session.as_deref() == Some(from) {
        store.active_session = Some(to.clone());
    }
    save_store(&store)?;
    Ok(TraceSessionEditReport {
        changed_entries,
        removed_entries: 0,
        created_session: Some(to),
    })
}

pub fn merge_sessions(from: &str, to: &str) -> Result<TraceSessionEditReport> {
    rename_session(from, to)
}

pub fn split_session_by_tag(from: &str, tag: &str, to: &str) -> Result<TraceSessionEditReport> {
    let to = normalize_session_value(to)
        .ok_or_else(|| AppError::General("Trace session name cannot be empty".to_string()))?;
    let tag = tag.trim();
    if tag.is_empty() {
        return Err(AppError::General("Trace split tag cannot be empty".to_string()));
    }

    let mut store = load_store()?;
    let mut changed_entries = 0;
    for entry in &mut store.entries {
        if session_name(entry) == from && entry.tags.iter().any(|entry_tag| entry_tag == tag) {
            entry.session = Some(to.clone());
            changed_entries += 1;
        }
    }
    if changed_entries == 0 {
        return Err(AppError::General(format!(
            "No entries in session {from} matched tag {tag}"
        )));
    }
    save_store(&store)?;
    Ok(TraceSessionEditReport {
        changed_entries,
        removed_entries: 0,
        created_session: Some(to),
    })
}

pub fn verify_store(root: Option<&Path>) -> Result<TraceStoreCheck> {
    let store = load_store()?;
    Ok(check_store(&store, root))
}

pub fn repair_store(root: Option<&Path>) -> Result<TraceStoreRepairReport> {
    let mut store = load_store()?;
    let before_entries = store.entries.len();
    let mut assigned_ids = 0;
    let mut removed_duplicate_ids = 0;
    let mut removed_dangling_parents = 0;

    let mut seen_ids = BTreeSet::new();
    for index in 0..store.entries.len() {
        if store.entries[index].id.trim().is_empty() || seen_ids.contains(&store.entries[index].id) {
            if !store.entries[index].id.trim().is_empty() {
                removed_duplicate_ids += 1;
            }
            store.entries[index].id = format!("{}-repair-{}", now_secs(), index + 1);
            assigned_ids += 1;
        }
        seen_ids.insert(store.entries[index].id.clone());
    }

    let ids = store
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<BTreeSet<_>>();
    for entry in &mut store.entries {
        if entry
            .parent
            .as_deref()
            .map(|parent| !ids.contains(parent))
            .unwrap_or(false)
        {
            entry.parent = None;
            removed_dangling_parents += 1;
        }
    }

    let before_archived = store.archived_sessions.len();
    let session_names = store
        .entries
        .iter()
        .map(|entry| session_name(entry).to_string())
        .collect::<BTreeSet<_>>();
    store
        .archived_sessions
        .retain(|session| session_names.contains(&session.name));
    dedupe_archived_sessions(&mut store.archived_sessions);
    let removed_archived_sessions = before_archived.saturating_sub(store.archived_sessions.len());

    if let Some(root) = root {
        for entry in &mut store.entries {
            if entry.workspace.as_deref() == Some(root) && entry.path.is_relative() {
                entry.path = root.join(&entry.path);
            }
        }
    }

    let after_entries = store.entries.len();
    save_store(&store)?;
    Ok(TraceStoreRepairReport {
        before_entries,
        after_entries,
        assigned_ids,
        removed_duplicate_ids,
        removed_dangling_parents,
        removed_archived_sessions,
    })
}

pub fn compact_store() -> Result<TraceStoreRepairReport> {
    let mut store = load_store()?;
    let before_entries = store.entries.len();
    let mut seen = BTreeSet::new();
    store.entries.retain(|entry| {
        let key = compact_key(entry);
        if seen.contains(&key) {
            false
        } else {
            seen.insert(key);
            true
        }
    });
    let after_entries = store.entries.len();
    save_store(&store)?;
    Ok(TraceStoreRepairReport {
        before_entries,
        after_entries,
        assigned_ids: 0,
        removed_duplicate_ids: before_entries.saturating_sub(after_entries),
        removed_dangling_parents: 0,
        removed_archived_sessions: 0,
    })
}

pub fn update_entry_note(selector: &str, note: &str) -> Result<TraceEntryChange> {
    update_entry_field(selector, |entry| {
        entry.note = normalized_optional_value(note);
    })
}

pub fn update_entry_status(selector: &str, status: &str) -> Result<TraceEntryChange> {
    update_entry_field(selector, |entry| {
        entry.status = normalized_optional_value(status);
    })
}

pub fn update_entry_priority(selector: &str, priority: &str) -> Result<TraceEntryChange> {
    update_entry_field(selector, |entry| {
        entry.priority = normalized_optional_value(priority);
    })
}

pub fn clear() -> Result<()> {
    save_store(&TraceStore::default())
}

pub fn export_markdown(root: Option<&Path>) -> Result<String> {
    let entries = if let Some(root) = root {
        list_for_workspace(root)?
    } else {
        list()?
    };
    Ok(entries_to_markdown(entries))
}

pub fn export_json(root: Option<&Path>) -> Result<String> {
    let entries = if let Some(root) = root {
        list_for_workspace(root)?
    } else {
        list()?
    };
    serde_json::to_string_pretty(&entries)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

pub fn export_graph(root: Option<&Path>) -> Result<String> {
    export_graph_format(root, TraceGraphFormat::Text)
}

pub fn export_graph_format(root: Option<&Path>, format: TraceGraphFormat) -> Result<String> {
    export_graph_with_options(root, format, &TraceGraphOptions::default())
}

pub fn graph_report_with_options(root: Option<&Path>, options: &TraceGraphOptions) -> Result<TraceGraphReport> {
    let entries = if let Some(root) = root {
        list_for_workspace(root)?
    } else {
        list()?
    }
    .into_iter()
    .filter(|entry| trace_entry_matches_filter(entry, &options.filter))
    .collect::<Vec<_>>();
    Ok(entries_to_graph_report_with_options(entries, options))
}

pub fn export_graph_with_options(
    root: Option<&Path>,
    format: TraceGraphFormat,
    options: &TraceGraphOptions,
) -> Result<String> {
    match format {
        TraceGraphFormat::Text => {
            let report = graph_report_with_options(root, options)?;
            Ok(graph_report_to_text(&report))
        }
        TraceGraphFormat::Json => {
            let report = graph_report_with_options(root, options)?;
            serde_json::to_string_pretty(&report)
                .map(|mut json| {
                    json.push('\n');
                    json
                })
                .map_err(|err| AppError::General(err.to_string()))
        }
        TraceGraphFormat::Mermaid => {
            let report = graph_report_with_options(root, options)?;
            Ok(graph_report_to_mermaid(&report))
        }
        TraceGraphFormat::Dot => {
            let report = graph_report_with_options(root, options)?;
            Ok(graph_report_to_dot(&report))
        }
    }
}

pub fn export_session_markdown(name: &str, root: Option<&Path>) -> Result<String> {
    let report = session_report(name, root)?;
    Ok(session_report_to_markdown(&report))
}

pub fn export_session_json(name: &str, root: Option<&Path>) -> Result<String> {
    let report = session_report(name, root)?;
    serde_json::to_string_pretty(&report)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

pub fn session_entries(name: &str, root: Option<&Path>) -> Result<Vec<TraceEntry>> {
    Ok(session_report(name, root)?.entries)
}

pub fn session_timeline(name: &str, root: Option<&Path>) -> Result<Vec<TraceTimelineItem>> {
    Ok(session_report(name, root)?.timeline)
}

pub fn session_replay(name: &str, root: Option<&Path>) -> Result<Vec<TraceReplayStep>> {
    Ok(session_report(name, root)?.replay)
}

pub fn session_diff(left_session: &str, right_session: &str, root: Option<&Path>) -> Result<TraceSessionDiff> {
    let left_report = session_report(left_session, root)?;
    let right_report = session_report(right_session, root)?;
    Ok(diff_reports(&left_report, &right_report))
}

pub fn session_diff_filtered(
    left_session: &str,
    right_session: &str,
    root: Option<&Path>,
    filter: TraceDiffFilter,
) -> Result<TraceSessionDiff> {
    let mut diff = session_diff(left_session, right_session, root)?;
    if filter == TraceDiffFilter::All {
        return Ok(diff);
    }
    diff.only_left.retain(|entry| diff_entry_matches(entry, filter));
    diff.only_right.retain(|entry| diff_entry_matches(entry, filter));
    diff.changed.retain(|entry| diff_entry_matches(entry, filter));
    Ok(diff)
}

pub fn export_session_timeline_markdown(name: &str, root: Option<&Path>) -> Result<String> {
    let report = session_report(name, root)?;
    Ok(session_timeline_to_markdown(&report))
}

pub fn export_session_timeline_json(name: &str, root: Option<&Path>) -> Result<String> {
    let timeline = session_timeline(name, root)?;
    serde_json::to_string_pretty(&timeline)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

pub fn export_session_replay_markdown(name: &str, root: Option<&Path>) -> Result<String> {
    let report = session_report(name, root)?;
    Ok(session_replay_to_markdown(&report))
}

pub fn export_session_replay_json(name: &str, root: Option<&Path>) -> Result<String> {
    let replay = session_replay(name, root)?;
    serde_json::to_string_pretty(&replay)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

pub fn session_replay_plan(
    name: &str,
    root: Option<&Path>,
    program: Option<&str>,
    profile_name: Option<&str>,
) -> Result<TraceReplayPlan> {
    let report = session_report(name, root)?;
    Ok(build_replay_plan(&report, program, profile_name))
}

pub fn export_session_replay_plan_markdown(
    name: &str,
    root: Option<&Path>,
    program: Option<&str>,
    profile_name: Option<&str>,
) -> Result<String> {
    let plan = session_replay_plan(name, root, program, profile_name)?;
    Ok(replay_plan_to_markdown(&plan))
}

pub fn export_session_replay_plan_json(
    name: &str,
    root: Option<&Path>,
    program: Option<&str>,
    profile_name: Option<&str>,
) -> Result<String> {
    let plan = session_replay_plan(name, root, program, profile_name)?;
    serde_json::to_string_pretty(&plan)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

pub fn export_session_structured_markdown(name: &str, root: Option<&Path>) -> Result<String> {
    let report = session_report(name, root)?;
    let mut output = String::from("# fcs Trace Structured Report\n\n");
    output.push_str(&format!("- session: {}\n", report.summary.name));
    output.push_str(&format!("- entries: {}\n", report.summary.entries));
    append_structured_markdown(&mut output, &report.structured);
    Ok(output)
}

pub fn export_session_structured_json(name: &str, root: Option<&Path>) -> Result<String> {
    let structured = session_report(name, root)?.structured;
    serde_json::to_string_pretty(&structured)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

pub fn export_session_diff_markdown(left_session: &str, right_session: &str, root: Option<&Path>) -> Result<String> {
    let diff = session_diff(left_session, right_session, root)?;
    Ok(session_diff_to_markdown(&diff))
}

pub fn export_session_diff_json(left_session: &str, right_session: &str, root: Option<&Path>) -> Result<String> {
    let diff = session_diff(left_session, right_session, root)?;
    serde_json::to_string_pretty(&diff)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

pub fn export_session_diff_markdown_filtered(
    left_session: &str,
    right_session: &str,
    root: Option<&Path>,
    filter: TraceDiffFilter,
) -> Result<String> {
    let diff = session_diff_filtered(left_session, right_session, root, filter)?;
    Ok(session_diff_to_markdown(&diff))
}

pub fn export_session_diff_json_filtered(
    left_session: &str,
    right_session: &str,
    root: Option<&Path>,
    filter: TraceDiffFilter,
) -> Result<String> {
    let diff = session_diff_filtered(left_session, right_session, root, filter)?;
    serde_json::to_string_pretty(&diff)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

pub fn session_insights(name: &str, root: Option<&Path>) -> Result<TraceInsightsReport> {
    let report = session_report(name, root)?;
    let index = match root {
        Some(root) => crate::index::load(root)?,
        None => None,
    };
    let kind_counts = count_values(report.entries.iter().map(|entry| entry.kind.as_str()));
    let hot_files = count_values_sorted(
        report
            .entries
            .iter()
            .map(|entry| entry.path.to_string_lossy().into_owned()),
    )
    .into_iter()
    .take(10)
    .map(|(name, count)| TraceInsightCount { name, count })
    .collect::<Vec<_>>();
    let debug_events = report
        .entries
        .iter()
        .filter(|entry| is_debug_trace_entry(entry))
        .map(trace_insight_event)
        .collect::<Vec<_>>();
    let unresolved_entries = report
        .entries
        .iter()
        .filter(|entry| is_unresolved_trace_entry(entry))
        .map(trace_insight_event)
        .collect::<Vec<_>>();
    let nearest_symbols = match (root, index.as_ref()) {
        (Some(root), Some(index)) => nearest_symbols_for_entries(&report.entries, root, index),
        _ => Vec::new(),
    };
    let suggested_next_steps = insight_next_steps(&report, root, index.as_ref(), &debug_events, &unresolved_entries);

    Ok(TraceInsightsReport {
        session: report.summary.name,
        entries: report.summary.entries,
        status_counts: report.status_counts,
        priority_counts: report.priority_counts,
        kind_counts,
        hot_files,
        debug_events,
        unresolved_entries,
        nearest_symbols,
        suggested_next_steps,
    })
}

pub fn export_session_insights_markdown(name: &str, root: Option<&Path>) -> Result<String> {
    let insights = session_insights(name, root)?;
    Ok(session_insights_to_markdown(&insights))
}

pub fn export_session_insights_json(name: &str, root: Option<&Path>) -> Result<String> {
    let insights = session_insights(name, root)?;
    serde_json::to_string_pretty(&insights)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

fn session_insights_to_markdown(insights: &TraceInsightsReport) -> String {
    let mut output = String::from("# fcs Trace Insights\n\n");
    output.push_str(&format!("- session: {}\n", insights.session));
    output.push_str(&format!("- entries: {}\n", insights.entries));
    output.push_str(&format!("- statuses: {}\n", format_count_map(&insights.status_counts)));
    output.push_str(&format!(
        "- priorities: {}\n",
        format_count_map(&insights.priority_counts)
    ));
    output.push_str(&format!("- kinds: {}\n\n", format_count_map(&insights.kind_counts)));

    output.push_str("## Hot Files\n");
    if insights.hot_files.is_empty() {
        output.push_str("- none\n");
    } else {
        for item in &insights.hot_files {
            output.push_str(&format!("- {} ({})\n", item.name, item.count));
        }
    }

    output.push_str("\n## Debug Events\n");
    append_insight_events(&mut output, &insights.debug_events);
    output.push_str("\n## Unresolved Entries\n");
    append_insight_events(&mut output, &insights.unresolved_entries);

    output.push_str("\n## Nearest Indexed Symbols\n");
    if insights.nearest_symbols.is_empty() {
        output.push_str("- none\n");
    } else {
        for symbol in &insights.nearest_symbols {
            let line = symbol.line.unwrap_or(1);
            output.push_str(&format!(
                "- `{}` {}:{} -> {} [{}] at line {} (distance {})\n",
                symbol.entry_id,
                symbol.path.display(),
                line,
                symbol.symbol,
                symbol.kind,
                symbol.symbol_line,
                symbol.distance
            ));
        }
    }

    output.push_str("\n## Suggested Next Steps\n");
    if insights.suggested_next_steps.is_empty() {
        output.push_str("- none\n");
    } else {
        for step in &insights.suggested_next_steps {
            output.push_str(&format!("- {step}\n"));
        }
    }
    output
}

fn append_insight_events(output: &mut String, events: &[TraceInsightEvent]) {
    if events.is_empty() {
        output.push_str("- none\n");
        return;
    }

    for event in events {
        let line = event.line.unwrap_or(1);
        let status = event.status.as_deref().unwrap_or("-");
        let priority = event.priority.as_deref().unwrap_or("-");
        output.push_str(&format!(
            "- `{}` [{}] {}:{} - {} (status={}, priority={})\n",
            event.id,
            event.kind,
            event.path.display(),
            line,
            event.label,
            status,
            priority
        ));
    }
}

fn trace_insight_event(entry: &TraceEntry) -> TraceInsightEvent {
    TraceInsightEvent {
        id: entry.id.clone(),
        kind: entry.kind.clone(),
        label: entry.label.clone(),
        path: entry.path.clone(),
        line: entry.line,
        status: entry.status.clone(),
        priority: entry.priority.clone(),
    }
}

fn is_debug_trace_entry(entry: &TraceEntry) -> bool {
    entry.kind.contains("debug")
        || entry.kind.contains("dap")
        || entry
            .tags
            .iter()
            .any(|tag| tag.contains("debug") || tag.contains("dap"))
}

fn is_unresolved_trace_entry(entry: &TraceEntry) -> bool {
    let Some(status) = entry.status.as_deref() else {
        return true;
    };
    !matches!(
        status,
        "done" | "resolved" | "closed" | "fixed" | "verified" | "complete"
    )
}

fn nearest_symbols_for_entries(
    entries: &[TraceEntry],
    root: &Path,
    index: &crate::index::CodeIndex,
) -> Vec<TraceInsightSymbol> {
    entries
        .iter()
        .filter_map(|entry| nearest_symbol_for_entry(entry, root, index))
        .collect()
}

fn nearest_symbol_for_entry(
    entry: &TraceEntry,
    root: &Path,
    index: &crate::index::CodeIndex,
) -> Option<TraceInsightSymbol> {
    let line = entry.line?;
    let path_key = trace_entry_index_path(root, &entry.path);
    let symbol = index
        .symbols
        .iter()
        .filter(|symbol| symbol.path == path_key)
        .min_by_key(|symbol| (symbol.line.abs_diff(line), Reverse(symbol.line)))?;
    Some(TraceInsightSymbol {
        entry_id: entry.id.clone(),
        path: entry.path.clone(),
        line: entry.line,
        symbol: if symbol.name.is_empty() {
            symbol.label.clone()
        } else {
            symbol.name.clone()
        },
        kind: symbol.kind.clone(),
        symbol_line: symbol.line,
        distance: symbol.line.abs_diff(line),
    })
}

fn trace_entry_index_path(root: &Path, path: &Path) -> String {
    let relative = if path.is_absolute() {
        path.strip_prefix(root).unwrap_or(path)
    } else {
        path
    };
    relative.to_string_lossy().replace('\\', "/")
}

fn insight_next_steps(
    report: &TraceSessionReport,
    root: Option<&Path>,
    index: Option<&crate::index::CodeIndex>,
    debug_events: &[TraceInsightEvent],
    unresolved_entries: &[TraceInsightEvent],
) -> Vec<String> {
    let mut steps = Vec::new();
    if root.is_some() && index.is_none() {
        steps.push(
            "Build the workspace index with `fcs project index build` to connect trace entries to nearby symbols"
                .to_string(),
        );
    }
    if debug_events.is_empty() {
        steps.push(format!(
            "Create a debug profile from this session with `fcs debug dap from-trace {} <program>`",
            report.summary.name
        ));
    }
    if !unresolved_entries.is_empty() {
        steps.push(format!(
            "Review {} unresolved trace entry status value(s)",
            unresolved_entries.len()
        ));
    }
    if !report.structured.open_questions.is_empty() {
        steps.push(format!(
            "Resolve {} open question(s) captured in structured trace notes",
            report.structured.open_questions.len()
        ));
    }
    if report.structured.conclusions.is_empty() {
        steps.push("Add a conclusion trace entry or tag once the investigation has an outcome".to_string());
    }
    steps
}

fn count_values<'a>(values: impl Iterator<Item = &'a str>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        let normalized = if value.trim().is_empty() { "unset" } else { value };
        *counts.entry(normalized.to_string()).or_insert(0) += 1;
    }
    counts
}

fn count_values_sorted(values: impl Iterator<Item = String>) -> Vec<(String, usize)> {
    let mut counts = BTreeMap::<String, usize>::new();
    for value in values {
        let normalized = if value.trim().is_empty() {
            "unset".to_string()
        } else {
            value
        };
        *counts.entry(normalized).or_insert(0) += 1;
    }
    let mut items = counts.into_iter().collect::<Vec<(String, usize)>>();
    items.sort_by_key(|(name, count)| (Reverse(*count), name.clone()));
    items
}

fn format_count_map(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(name, count)| format!("{name}={count}"))
        .collect::<Vec<String>>()
        .join(", ")
}

fn entries_to_markdown(entries: Vec<TraceEntry>) -> String {
    let mut output = String::from("# fcs Trace Report\n\n");
    for entry in entries {
        let line = entry.line.unwrap_or(1);
        let column = entry.column.map(|value| format!(":{value}")).unwrap_or_default();
        output.push_str(&format!(
            "- `{}` `{}` [{}] {}:{}{} - {}\n",
            entry.id,
            entry.timestamp,
            entry.kind,
            entry.path.display(),
            line,
            column,
            entry.label
        ));
        if let Some(note) = entry.note.as_deref() {
            output.push_str(&format!("  - note: {note}\n"));
        }
        for line in metadata_lines(&entry) {
            output.push_str(&format!("  - {line}\n"));
        }
    }
    output
}

fn graph_report_to_text(report: &TraceGraphReport) -> String {
    let mut output = String::from("# fcs Trace Graph\n\n");
    for collapsed in &report.collapsed {
        output.push_str(&format!(
            "- <root> -> {} [summary] {} entries session={} kind={} path={}\n",
            collapsed.id,
            collapsed.entries,
            collapsed.session,
            collapsed.kind,
            collapsed.path.display()
        ));
    }
    for edge in &report.edges {
        let Some(node) = report.nodes.iter().find(|node| node.id == edge.to) else {
            continue;
        };
        let parent = if edge.from.is_empty() { "<root>" } else { &edge.from };
        let line = node.line.unwrap_or(1);
        let column = node.column.map(|value| format!(":{value}")).unwrap_or_default();
        output.push_str(&format!(
            "- {parent} -> {} [{}] {}:{}{} - {}{}\n",
            node.id,
            node.kind,
            node.path.display(),
            line,
            column,
            node.label,
            graph_node_metadata_suffix(node)
        ));
    }
    output
}

#[cfg(test)]
fn entries_to_graph(entries: Vec<TraceEntry>) -> String {
    let report = entries_to_graph_report_with_options(entries, &TraceGraphOptions::default());
    graph_report_to_text(&report)
}

fn entries_to_graph_report_with_options(mut entries: Vec<TraceEntry>, options: &TraceGraphOptions) -> TraceGraphReport {
    entries.sort_by_key(|entry| entry.timestamp);
    let (entries, collapsed) = collapse_graph_entries(entries, options.collapse_threshold);
    let nodes = entries
        .iter()
        .map(|entry| {
            let id = trace_node_id(entry);
            TraceGraphNode {
                id,
                label: entry.label.clone(),
                kind: entry.kind.clone(),
                path: entry.path.clone(),
                line: entry.line,
                column: entry.column,
                session: session_name(entry).to_string(),
                note: entry.note.clone(),
                status: entry.status.clone(),
                priority: entry.priority.clone(),
                branch: entry.branch.clone(),
                tags: entry.tags.clone(),
            }
        })
        .collect::<Vec<_>>();
    let edges = entries
        .iter()
        .map(|entry| TraceGraphEdge {
            from: entry.parent.clone().unwrap_or_else(|| "<root>".to_string()),
            to: trace_node_id(entry),
            kind: entry.kind.clone(),
            label: entry.label.clone(),
        })
        .collect::<Vec<_>>();
    TraceGraphReport {
        entries: entries.len(),
        nodes,
        edges,
        collapsed,
    }
}

fn graph_report_to_mermaid(report: &TraceGraphReport) -> String {
    let mut output = String::from("flowchart TD\n");
    output.push_str("  root[\"<root>\"]\n");
    for collapsed in &report.collapsed {
        output.push_str(&format!(
            "  {}[\"{}\"]\n",
            graph_node_ref(&collapsed.id),
            escape_mermaid_label(&format!(
                "summary\\n{} {} ({})",
                collapsed.session, collapsed.kind, collapsed.entries
            ))
        ));
        output.push_str(&format!("  root --> {}\n", graph_node_ref(&collapsed.id)));
    }
    for node in &report.nodes {
        output.push_str(&format!(
            "  {}[\"{}\"]\n",
            graph_node_ref(&node.id),
            escape_mermaid_label(&format!("{}\\n{}", node.kind, node.label))
        ));
    }
    for edge in &report.edges {
        output.push_str(&format!(
            "  {} -->|{}| {}\n",
            graph_node_ref(&edge.from),
            escape_mermaid_label(&edge.kind),
            graph_node_ref(&edge.to)
        ));
    }
    output
}

fn graph_report_to_dot(report: &TraceGraphReport) -> String {
    let mut output = String::from("digraph fcs_trace {\n");
    output.push_str("  rankdir=LR;\n");
    output.push_str("  \"<root>\" [shape=box,label=\"<root>\"];\n");
    for collapsed in &report.collapsed {
        output.push_str(&format!(
            "  \"{}\" [shape=folder,label=\"summary\\n{} {} ({})\"];\n",
            escape_dot_label(&collapsed.id),
            escape_dot_label(&collapsed.session),
            escape_dot_label(&collapsed.kind),
            collapsed.entries
        ));
        output.push_str(&format!(
            "  \"<root>\" -> \"{}\" [label=\"summary\"];\n",
            escape_dot_label(&collapsed.id)
        ));
    }
    for node in &report.nodes {
        output.push_str(&format!(
            "  \"{}\" [label=\"{}\\n{}\"];\n",
            escape_dot_label(&node.id),
            escape_dot_label(&node.kind),
            escape_dot_label(&node.label)
        ));
    }
    for edge in &report.edges {
        output.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
            escape_dot_label(&edge.from),
            escape_dot_label(&edge.to),
            escape_dot_label(&edge.kind)
        ));
    }
    output.push_str("}\n");
    output
}

fn collapse_graph_entries(entries: Vec<TraceEntry>, threshold: usize) -> (Vec<TraceEntry>, Vec<TraceGraphCollapse>) {
    if threshold == 0 {
        return (entries, Vec::new());
    }

    let mut counts = BTreeMap::<String, usize>::new();
    for entry in &entries {
        *counts.entry(collapse_key(entry)).or_insert(0) += 1;
    }

    let collapsed_keys = counts
        .iter()
        .filter(|(_, count)| **count > threshold)
        .map(|(key, _)| key.clone())
        .collect::<BTreeSet<_>>();
    if collapsed_keys.is_empty() {
        return (entries, Vec::new());
    }

    let mut collapsed = Vec::new();
    let mut collapsed_seen = BTreeSet::new();
    let kept = entries
        .into_iter()
        .filter(|entry| {
            let key = collapse_key(entry);
            if !collapsed_keys.contains(&key) {
                return true;
            }
            if collapsed_seen.insert(key.clone()) {
                let parts = key.split('|').collect::<Vec<_>>();
                collapsed.push(TraceGraphCollapse {
                    id: format!("summary:{}", collapsed_seen.len()),
                    entries: counts.get(&key).copied().unwrap_or(0),
                    session: parts.first().copied().unwrap_or("default").to_string(),
                    kind: parts.get(1).copied().unwrap_or("unknown").to_string(),
                    path: PathBuf::from(parts.get(2).copied().unwrap_or("-")),
                });
            }
            false
        })
        .collect::<Vec<_>>();

    (kept, collapsed)
}

fn collapse_key(entry: &TraceEntry) -> String {
    format!("{}|{}|{}", session_name(entry), entry.kind, entry.path.display())
}

pub fn summarize_sessions(
    entries: &[TraceEntry],
    archived_sessions: &[ArchivedTraceSession],
) -> Vec<TraceSessionSummary> {
    let archived_by_name: BTreeMap<String, u64> = archived_sessions
        .iter()
        .map(|session| (session.name.clone(), session.archived_at))
        .collect();
    let mut groups: BTreeMap<String, SessionAccumulator> = BTreeMap::new();

    for entry in entries {
        let accumulator = groups.entry(session_name(entry).to_string()).or_default();
        accumulator.entries += 1;
        accumulator.first_timestamp = match accumulator.first_timestamp {
            0 => entry.timestamp,
            existing => existing.min(entry.timestamp),
        };
        accumulator.last_timestamp = accumulator.last_timestamp.max(entry.timestamp);
        if let Some(branch) = entry.branch.as_deref().filter(|branch| !branch.is_empty()) {
            accumulator.branches.insert(branch.to_string());
        }
        for tag in entry.tags.iter().filter(|tag| !tag.is_empty()) {
            accumulator.tags.insert(tag.clone());
        }
    }

    for archived in archived_sessions {
        groups.entry(archived.name.clone()).or_default();
    }

    let mut summaries = groups
        .into_iter()
        .map(|(name, accumulator)| TraceSessionSummary {
            archived_at: archived_by_name.get(&name).copied(),
            name,
            entries: accumulator.entries,
            first_timestamp: accumulator.first_timestamp,
            last_timestamp: accumulator.last_timestamp,
            branches: accumulator.branches.into_iter().collect(),
            tags: accumulator.tags.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    summaries.sort_by_key(|summary| {
        (
            summary.is_archived(),
            Reverse(summary.last_timestamp),
            summary.name.clone(),
        )
    });
    summaries
}

pub fn entries_to_items(entries: &[TraceEntry]) -> Vec<CodeItem> {
    entries
        .iter()
        .map(|entry| {
            let line = entry.line.unwrap_or(1);
            let display_path = entry.path.to_string_lossy().replace('\\', "/");
            let label = match metadata_summary(entry) {
                Some(metadata) => format!("{} {{{metadata}}}", entry.label),
                None => entry.label.clone(),
            };
            CodeItem::symbol(
                entry.path.clone(),
                display_path,
                line,
                entry.column,
                label,
                entry.kind.clone(),
            )
        })
        .collect()
}

pub fn format_session(summary: &TraceSessionSummary) -> String {
    let state = summary
        .archived_at
        .map(|timestamp| format!("archived@{timestamp}"))
        .unwrap_or_else(|| "active".to_string());
    let branches = if summary.branches.is_empty() {
        "-".to_string()
    } else {
        summary.branches.join(",")
    };
    let tags = if summary.tags.is_empty() {
        "-".to_string()
    } else {
        summary.tags.join(",")
    };

    format!(
        "{} [{}] entries={} first={} last={} branches={} tags={}",
        summary.name, state, summary.entries, summary.first_timestamp, summary.last_timestamp, branches, tags
    )
}

fn load_store() -> Result<TraceStore> {
    let path = trace_path()?;
    load_store_from_path(&path)
}

fn load_store_from_path(path: &Path) -> Result<TraceStore> {
    if !path.exists() {
        return Ok(TraceStore::default());
    }

    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents).map_err(|e| AppError::General(e.to_string()))
}

fn save_store(store: &TraceStore) -> Result<()> {
    let path = trace_path()?;
    save_store_to_path(&path, store)
}

fn save_store_to_path(path: &Path, store: &TraceStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(store).map_err(|e| AppError::General(e.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn duplicate_entry_id(
    entries: &[TraceEntry],
    root: Option<&Path>,
    location: &Location,
    label: &str,
    kind: &str,
    metadata: &TraceMetadata,
) -> Option<String> {
    let workspace = root.map(Path::to_path_buf);
    entries
        .iter()
        .find(|entry| {
            entry.workspace == workspace
                && entry.kind == kind
                && entry.label == label
                && entry.path == location.path
                && entry.line == location.line
                && entry.column == location.column
                && entry.session == metadata.session
                && entry.parent == metadata.parent
                && entry.branch == metadata.branch
                && entry.tags == metadata.tags
        })
        .map(|entry| entry.id.clone())
}

fn compact_key(entry: &TraceEntry) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        entry
            .workspace
            .as_deref()
            .map(Path::display)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        session_name(entry),
        entry.kind,
        entry.label,
        entry.path.display(),
        entry.line.unwrap_or(0),
        entry.column.unwrap_or(0),
        entry.parent.as_deref().unwrap_or_default(),
        entry.tags.join(",")
    )
}

fn check_store(store: &TraceStore, root: Option<&Path>) -> TraceStoreCheck {
    let mut ids = BTreeSet::new();
    let mut duplicate_ids = BTreeSet::new();
    let mut missing_ids = 0;
    for entry in &store.entries {
        if entry.id.trim().is_empty() {
            missing_ids += 1;
            continue;
        }
        if !ids.insert(entry.id.clone()) {
            duplicate_ids.insert(entry.id.clone());
        }
    }

    let dangling_parents = store
        .entries
        .iter()
        .filter_map(|entry| {
            let parent = entry.parent.as_deref()?;
            (!ids.contains(parent)).then(|| parent.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let missing_paths = store
        .entries
        .iter()
        .filter(|entry| {
            root.map(|root| entry.workspace.as_deref() == Some(root))
                .unwrap_or(true)
        })
        .filter_map(|entry| {
            let path = if entry.path.is_absolute() {
                entry.path.clone()
            } else {
                root.map(|root| root.join(&entry.path))
                    .unwrap_or_else(|| entry.path.clone())
            };
            (!path.exists()).then_some(path)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(50)
        .collect::<Vec<_>>();

    TraceStoreCheck {
        ok: duplicate_ids.is_empty() && missing_ids == 0 && dangling_parents.is_empty() && missing_paths.is_empty(),
        entries: store.entries.len(),
        duplicate_ids: duplicate_ids.into_iter().collect(),
        missing_ids,
        dangling_parents,
        missing_paths,
    }
}

fn dedupe_archived_sessions(sessions: &mut Vec<ArchivedTraceSession>) {
    sessions.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.archived_at.cmp(&right.archived_at))
    });
    sessions.dedup_by(|left, right| left.name == right.name);
}

fn update_entry_field<F>(selector: &str, update: F) -> Result<TraceEntryChange>
where
    F: FnOnce(&mut TraceEntry),
{
    let mut store = load_store()?;
    let change = update_entry_field_in_store(&mut store, selector, update);
    if change == TraceEntryChange::Changed {
        save_store(&store)?;
    }
    Ok(change)
}

fn update_entry_field_in_store<F>(store: &mut TraceStore, selector: &str, update: F) -> TraceEntryChange
where
    F: FnOnce(&mut TraceEntry),
{
    let Some(entry) = find_entry_mut(store, selector) else {
        return TraceEntryChange::NotFound;
    };

    update(entry);
    TraceEntryChange::Changed
}

fn find_entry_mut<'a>(store: &'a mut TraceStore, selector: &str) -> Option<&'a mut TraceEntry> {
    if selector == "latest" {
        return store.entries.iter_mut().max_by_key(|entry| entry.timestamp);
    }

    store.entries.iter_mut().find(|entry| entry.id == selector)
}

fn normalized_optional_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_session_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn trace_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("trace.toml"))
}

fn cache_dir() -> Result<PathBuf> {
    crate::cache::user_cache_dir()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn sorted_entries(mut entries: Vec<TraceEntry>) -> Result<Vec<TraceEntry>> {
    entries.sort_by_key(|entry| Reverse(entry.timestamp));
    Ok(entries)
}

fn entries_for_workspace(entries: Vec<TraceEntry>, root: &Path) -> Result<Vec<TraceEntry>> {
    sorted_entries(
        entries
            .into_iter()
            .filter(|entry| entry.workspace.as_deref() == Some(root) || entry.workspace.is_none())
            .collect(),
    )
}

fn session_report(name: &str, root: Option<&Path>) -> Result<TraceSessionReport> {
    let store = load_store()?;
    let filtered_entries = match root {
        Some(root) => entries_for_workspace(store.entries.clone(), root)?,
        None => sorted_entries(store.entries.clone())?,
    };
    let mut session_entries = filtered_entries
        .iter()
        .filter(|entry| session_name(entry) == name)
        .cloned()
        .collect::<Vec<_>>();
    session_entries.sort_by_key(|entry| entry.timestamp);

    let summary = summarize_sessions(&filtered_entries, &store.archived_sessions)
        .into_iter()
        .find(|summary| summary.name == name)
        .ok_or_else(|| AppError::General(format!("Trace session not found: {name}")))?;

    if session_entries.is_empty() && summary.entries == 0 {
        return Err(AppError::General(format!("Trace session has no entries: {name}")));
    }

    let timeline = entries_to_timeline(&session_entries);
    let replay = entries_to_replay(&session_entries);
    let structured = entries_to_structured_report(&session_entries);
    let status_counts = count_optional_values(session_entries.iter().map(|entry| entry.status.as_deref()));
    let priority_counts = count_optional_values(session_entries.iter().map(|entry| entry.priority.as_deref()));

    Ok(TraceSessionReport {
        summary,
        entries: session_entries,
        timeline,
        replay,
        structured,
        status_counts,
        priority_counts,
    })
}

fn entries_to_timeline(entries: &[TraceEntry]) -> Vec<TraceTimelineItem> {
    entries
        .iter()
        .map(|entry| TraceTimelineItem {
            id: entry.id.clone(),
            timestamp: entry.timestamp,
            kind: entry.kind.clone(),
            label: entry.label.clone(),
            path: entry.path.clone(),
            line: entry.line,
            column: entry.column,
            note: entry.note.clone(),
            status: entry.status.clone(),
            priority: entry.priority.clone(),
            parent: entry.parent.clone(),
            branch: entry.branch.clone(),
            tags: entry.tags.clone(),
        })
        .collect()
}

fn entries_to_replay(entries: &[TraceEntry]) -> Vec<TraceReplayStep> {
    let mut previous_timestamp = None;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let elapsed_secs = previous_timestamp
                .map(|previous| entry.timestamp.saturating_sub(previous))
                .unwrap_or(0);
            previous_timestamp = Some(entry.timestamp);

            TraceReplayStep {
                step: index + 1,
                entry_id: entry.id.clone(),
                timestamp: entry.timestamp,
                elapsed_secs,
                action: replay_action(entry),
                target: trace_target(entry),
                state: replay_state(entry),
                note: entry.note.clone(),
            }
        })
        .collect()
}

fn entries_to_structured_report(entries: &[TraceEntry]) -> TraceStructuredReport {
    let mut report = TraceStructuredReport::default();
    for entry in entries {
        let item = TraceStructuredItem {
            id: entry.id.clone(),
            label: entry.label.clone(),
            path: entry.path.clone(),
            line: entry.line,
            note: entry.note.clone(),
            status: entry.status.clone(),
            priority: entry.priority.clone(),
            tags: entry.tags.clone(),
        };

        if entry_has_marker(entry, "hypothesis") {
            report.hypotheses.push(item.clone());
        }
        if entry_has_marker(entry, "evidence") {
            report.evidence.push(item.clone());
        }
        if entry_has_marker(entry, "conclusion") || entry_status_is(entry, &["done", "fixed", "resolved"]) {
            report.conclusions.push(item.clone());
        }
        if entry_has_marker(entry, "question") || entry_status_is(entry, &["open", "blocked", "unknown"]) {
            report.open_questions.push(item);
        }
    }
    report
}

fn replay_action(entry: &TraceEntry) -> String {
    match entry.kind.as_str() {
        "definition" => "inspect definition".to_string(),
        "reference" => "inspect reference".to_string(),
        "search" => "inspect search hit".to_string(),
        "open" => "open location".to_string(),
        "bookmark" => "record bookmark".to_string(),
        other => format!("record {other}"),
    }
}

fn replay_state(entry: &TraceEntry) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(status) = entry.status.as_deref().filter(|value| !value.is_empty()) {
        parts.push(format!("status={status}"));
    }
    if let Some(priority) = entry.priority.as_deref().filter(|value| !value.is_empty()) {
        parts.push(format!("priority={priority}"));
    }
    if let Some(branch) = entry.branch.as_deref().filter(|value| !value.is_empty()) {
        parts.push(format!("branch={branch}"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn trace_target(entry: &TraceEntry) -> String {
    let line = entry.line.unwrap_or(1);
    let column = entry.column.map(|value| format!(":{value}")).unwrap_or_default();
    format!("{}:{}{}", entry.path.display(), line, column)
}

fn entry_has_marker(entry: &TraceEntry, marker: &str) -> bool {
    entry.kind.eq_ignore_ascii_case(marker) || entry.tags.iter().any(|tag| tag.eq_ignore_ascii_case(marker))
}

fn entry_status_is(entry: &TraceEntry, expected: &[&str]) -> bool {
    entry
        .status
        .as_deref()
        .map(|status| expected.iter().any(|value| status.eq_ignore_ascii_case(value)))
        .unwrap_or(false)
}

fn count_optional_values<'a>(values: impl Iterator<Item = Option<&'a str>>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values.flatten().filter(|value| !value.is_empty()) {
        *counts.entry(value.to_string()).or_insert(0) += 1;
    }
    counts
}

fn diff_reports(left_report: &TraceSessionReport, right_report: &TraceSessionReport) -> TraceSessionDiff {
    let left_by_key = entries_by_diff_key(&left_report.entries);
    let right_by_key = entries_by_diff_key(&right_report.entries);
    let keys = left_by_key
        .keys()
        .chain(right_by_key.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut diff = TraceSessionDiff {
        left_session: left_report.summary.name.clone(),
        right_session: right_report.summary.name.clone(),
        unchanged: 0,
        only_left: Vec::new(),
        only_right: Vec::new(),
        changed: Vec::new(),
    };

    for key in keys {
        match (left_by_key.get(&key), right_by_key.get(&key)) {
            (Some(left), Some(right)) => {
                let changes = entry_changes(left, right);
                if changes.is_empty() {
                    diff.unchanged += 1;
                } else {
                    diff.changed.push(TraceDiffEntry {
                        key,
                        left: Some((*left).clone()),
                        right: Some((*right).clone()),
                        changes,
                    });
                }
            }
            (Some(left), None) => diff.only_left.push(TraceDiffEntry {
                key,
                left: Some((*left).clone()),
                right: None,
                changes: vec!["missing in right session".to_string()],
            }),
            (None, Some(right)) => diff.only_right.push(TraceDiffEntry {
                key,
                left: None,
                right: Some((*right).clone()),
                changes: vec!["missing in left session".to_string()],
            }),
            (None, None) => {}
        }
    }

    diff
}

fn entries_by_diff_key(entries: &[TraceEntry]) -> BTreeMap<String, TraceEntry> {
    let mut by_key = BTreeMap::new();
    for entry in entries {
        by_key.insert(diff_key(entry), entry.clone());
    }
    by_key
}

fn diff_key(entry: &TraceEntry) -> String {
    format!(
        "{}:{}:{}:{}",
        entry.path.display(),
        entry.line.unwrap_or(1),
        entry.column.unwrap_or(0),
        entry.label
    )
}

fn entry_changes(left: &TraceEntry, right: &TraceEntry) -> Vec<String> {
    let mut changes = Vec::new();
    push_field_change(
        &mut changes,
        "kind",
        Some(left.kind.as_str()),
        Some(right.kind.as_str()),
    );
    push_field_change(&mut changes, "note", left.note.as_deref(), right.note.as_deref());
    push_field_change(&mut changes, "status", left.status.as_deref(), right.status.as_deref());
    push_field_change(
        &mut changes,
        "priority",
        left.priority.as_deref(),
        right.priority.as_deref(),
    );
    push_field_change(&mut changes, "branch", left.branch.as_deref(), right.branch.as_deref());
    if left.tags != right.tags {
        changes.push(format!("tags: {} -> {}", left.tags.join(","), right.tags.join(",")));
    }
    changes
}

fn push_field_change(changes: &mut Vec<String>, name: &str, left: Option<&str>, right: Option<&str>) {
    if left != right {
        changes.push(format!(
            "{name}: {} -> {}",
            left.filter(|value| !value.is_empty()).unwrap_or("-"),
            right.filter(|value| !value.is_empty()).unwrap_or("-")
        ));
    }
}

pub fn format_entry(entry: &TraceEntry) -> String {
    let line = entry.line.unwrap_or(1);
    let column = entry.column.map(|value| format!(":{value}")).unwrap_or_default();
    let mut text = format!(
        "{} [{}] {}:{}{} {}",
        entry_label(entry),
        entry.kind,
        entry.path.display(),
        line,
        column,
        entry.label
    );
    if let Some(metadata) = metadata_summary(entry) {
        text.push_str(&format!(" {{{metadata}}}"));
    }
    text
}

pub fn location_from_path(path: impl AsRef<Path>, line: Option<usize>, column: Option<usize>) -> Location {
    Location::new(path.as_ref().to_path_buf(), line, column)
}

fn archive_session_in_store(store: &mut TraceStore, name: &str, archived_at: u64) -> TraceSessionChange {
    if !store.entries.iter().any(|entry| session_name(entry) == name) {
        return TraceSessionChange::NotFound;
    }
    if store.archived_sessions.iter().any(|session| session.name == name) {
        return TraceSessionChange::Unchanged;
    }

    store.archived_sessions.push(ArchivedTraceSession {
        name: name.to_string(),
        archived_at,
    });
    store
        .archived_sessions
        .sort_by(|left, right| left.name.cmp(&right.name));
    TraceSessionChange::Changed
}

fn unarchive_session_in_store(store: &mut TraceStore, name: &str) -> TraceSessionChange {
    if !store.entries.iter().any(|entry| session_name(entry) == name) {
        return TraceSessionChange::NotFound;
    }

    let before = store.archived_sessions.len();
    store.archived_sessions.retain(|session| session.name != name);
    if store.archived_sessions.len() == before {
        TraceSessionChange::Unchanged
    } else {
        TraceSessionChange::Changed
    }
}

fn trace_entry_matches_filter(entry: &TraceEntry, filter: &TraceEntryFilter) -> bool {
    if let Some(session) = filter.session.as_deref() {
        if session_name(entry) != session {
            return false;
        }
    }
    if let Some(tag) = filter.tag.as_deref() {
        if !entry.tags.iter().any(|entry_tag| entry_tag == tag) {
            return false;
        }
    }
    if let Some(kind) = filter.kind.as_deref() {
        if entry.kind != kind {
            return false;
        }
    }
    if let Some(status) = filter.status.as_deref() {
        if entry.status.as_deref() != Some(status) {
            return false;
        }
    }
    if let Some(priority) = filter.priority.as_deref() {
        if entry.priority.as_deref() != Some(priority) {
            return false;
        }
    }
    if let Some(relation) = filter.relation.as_deref() {
        if semantic_relation_from_entry(entry) != Some(relation) && !entry.kind.ends_with(relation) {
            return false;
        }
    }
    true
}

fn diff_entry_matches(entry: &TraceDiffEntry, filter: TraceDiffFilter) -> bool {
    let matches_entry = |entry: &TraceEntry| match filter {
        TraceDiffFilter::All => true,
        TraceDiffFilter::Semantic => entry.kind == "semantic-root" || entry.kind.starts_with("semantic:"),
        TraceDiffFilter::Bookmark => entry.kind == "bookmark",
        TraceDiffFilter::Debug => is_debug_trace_entry(entry),
    };
    entry.left.as_ref().map(&matches_entry).unwrap_or(false) || entry.right.as_ref().map(matches_entry).unwrap_or(false)
}

fn entry_label(entry: &TraceEntry) -> String {
    if entry.id.is_empty() {
        entry.timestamp.to_string()
    } else {
        format!("{} {}", entry.id, entry.timestamp)
    }
}

fn trace_node_id(entry: &TraceEntry) -> String {
    if entry.id.is_empty() {
        entry.timestamp.to_string()
    } else {
        entry.id.clone()
    }
}

fn graph_node_metadata_suffix(node: &TraceGraphNode) -> String {
    let mut parts = Vec::new();
    parts.push(format!("session={}", node.session));
    if let Some(status) = node.status.as_deref() {
        parts.push(format!("status={status}"));
    }
    if let Some(priority) = node.priority.as_deref() {
        parts.push(format!("priority={priority}"));
    }
    if let Some(branch) = node.branch.as_deref() {
        parts.push(format!("branch={branch}"));
    }
    if !node.tags.is_empty() {
        parts.push(format!("tags={}", node.tags.join(",")));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {{{}}}", parts.join(" "))
    }
}

fn session_name(entry: &TraceEntry) -> &str {
    entry
        .session
        .as_deref()
        .filter(|session| !session.is_empty())
        .unwrap_or("default")
}

fn metadata_summary(entry: &TraceEntry) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(session) = entry.session.as_deref() {
        parts.push(format!("session={session}"));
    }
    if let Some(status) = entry.status.as_deref() {
        parts.push(format!("status={status}"));
    }
    if let Some(priority) = entry.priority.as_deref() {
        parts.push(format!("priority={priority}"));
    }
    if let Some(parent) = entry.parent.as_deref() {
        parts.push(format!("parent={parent}"));
    }
    if let Some(branch) = entry.branch.as_deref() {
        parts.push(format!("branch={branch}"));
    }
    if !entry.tags.is_empty() {
        parts.push(format!("tags={}", entry.tags.join(",")));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn metadata_lines(entry: &TraceEntry) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(session) = entry.session.as_deref() {
        lines.push(format!("session: {session}"));
    }
    if let Some(status) = entry.status.as_deref() {
        lines.push(format!("status: {status}"));
    }
    if let Some(priority) = entry.priority.as_deref() {
        lines.push(format!("priority: {priority}"));
    }
    if let Some(parent) = entry.parent.as_deref() {
        lines.push(format!("parent: {parent}"));
    }
    if let Some(branch) = entry.branch.as_deref() {
        lines.push(format!("branch: {branch}"));
    }
    if !entry.tags.is_empty() {
        lines.push(format!("tags: {}", entry.tags.join(", ")));
    }
    lines
}

fn graph_node_ref(id: &str) -> String {
    if id == "<root>" {
        return "root".to_string();
    }
    let mut value = String::from("n");
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() {
            value.push(ch);
        } else {
            value.push('_');
        }
    }
    value
}

fn escape_mermaid_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('\n', "\\n")
}

fn escape_dot_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn semantic_relation_from_entry(entry: &TraceEntry) -> Option<&str> {
    if let Some(relation) = entry
        .note
        .as_deref()
        .and_then(|note| note.split_whitespace().find_map(|part| part.strip_prefix("relation=")))
    {
        return Some(relation);
    }

    entry.tags.iter().map(String::as_str).find(|tag| {
        matches!(
            *tag,
            "references" | "definition" | "type" | "implementation" | "incoming" | "outgoing"
        )
    })
}

fn session_report_to_markdown(report: &TraceSessionReport) -> String {
    let mut output = String::from("# fcs Trace Session Report\n\n");
    output.push_str(&format!("- session: {}\n", report.summary.name));
    output.push_str(&format!("- state: {}\n", session_state(&report.summary)));
    output.push_str(&format!("- entries: {}\n", report.summary.entries));
    output.push_str(&format!("- first: {}\n", report.summary.first_timestamp));
    output.push_str(&format!("- last: {}\n", report.summary.last_timestamp));
    if !report.summary.branches.is_empty() {
        output.push_str(&format!("- branches: {}\n", report.summary.branches.join(", ")));
    }
    if !report.summary.tags.is_empty() {
        output.push_str(&format!("- tags: {}\n", report.summary.tags.join(", ")));
    }
    if !report.status_counts.is_empty() {
        output.push_str(&format!("- status: {}\n", format_counts(&report.status_counts)));
    }
    if !report.priority_counts.is_empty() {
        output.push_str(&format!("- priority: {}\n", format_counts(&report.priority_counts)));
    }
    output.push_str("\n## Entries\n\n");

    for entry in &report.entries {
        output.push_str(&format!(
            "- `{}` [{}] {}:{}{} - {}\n",
            entry.id,
            entry.kind,
            entry.path.display(),
            entry.line.unwrap_or(1),
            entry.column.map(|value| format!(":{value}")).unwrap_or_default(),
            entry.label
        ));
        if let Some(note) = entry.note.as_deref() {
            output.push_str(&format!("  - note: {note}\n"));
        }
        for line in metadata_lines(entry) {
            if !line.starts_with("session:") {
                output.push_str(&format!("  - {line}\n"));
            }
        }
    }

    append_replay_markdown(&mut output, &report.replay);
    append_structured_markdown(&mut output, &report.structured);
    output
}

fn session_timeline_to_markdown(report: &TraceSessionReport) -> String {
    let mut output = String::from("# fcs Trace Session Timeline\n\n");
    output.push_str(&format!("- session: {}\n\n", report.summary.name));
    for item in &report.timeline {
        let line = item.line.unwrap_or(1);
        let column = item.column.map(|value| format!(":{value}")).unwrap_or_default();
        let status = item.status.as_deref().unwrap_or("-");
        let priority = item.priority.as_deref().unwrap_or("-");
        output.push_str(&format!(
            "- `{}` `{}` [{}] {}:{}{} - {} status={} priority={}\n",
            item.id,
            item.timestamp,
            item.kind,
            item.path.display(),
            line,
            column,
            item.label,
            status,
            priority
        ));
        if let Some(note) = item.note.as_deref() {
            output.push_str(&format!("  - note: {note}\n"));
        }
    }
    output
}

fn session_replay_to_markdown(report: &TraceSessionReport) -> String {
    let mut output = String::from("# fcs Trace Session Replay\n\n");
    output.push_str(&format!("- session: {}\n\n", report.summary.name));
    append_replay_markdown(&mut output, &report.replay);
    output
}

fn build_replay_plan(
    report: &TraceSessionReport,
    program: Option<&str>,
    profile_name: Option<&str>,
) -> TraceReplayPlan {
    let commands = report
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let target = trace_target(entry);
            TraceReplayCommand {
                step: index + 1,
                entry_id: entry.id.clone(),
                kind: entry.kind.clone(),
                target: target.clone(),
                command: replay_command_for_entry(report, entry, &target),
            }
        })
        .collect::<Vec<TraceReplayCommand>>();
    let debug_command = program.map(|program| {
        let mut command = format!(
            "fcs debug dap from-trace {} {}",
            shell_quote(&report.summary.name),
            shell_quote(program)
        );
        if let Some(profile_name) = profile_name {
            command.push_str(&format!(" --name {}", shell_quote(profile_name)));
        }
        command
    });

    TraceReplayPlan {
        session: report.summary.name.clone(),
        entries: report.entries.len(),
        commands,
        debug_command,
    }
}

fn replay_command_for_entry(report: &TraceSessionReport, entry: &TraceEntry, target: &str) -> String {
    if entry.kind == "semantic-root" {
        let relation = semantic_relation_from_entry(entry).unwrap_or("outgoing");
        return format!(
            "fcs trace semantic {} --relation {} --session {}",
            shell_quote(target),
            shell_quote(relation),
            shell_quote(&report.summary.name)
        );
    }
    if entry.kind.starts_with("semantic:") {
        return format!(
            "fcs trace add {} --kind {} --session {} --parent {}",
            shell_quote(target),
            shell_quote(&entry.kind),
            shell_quote(&report.summary.name),
            shell_quote(entry.parent.as_deref().unwrap_or("semantic-root"))
        );
    }
    if entry.kind == "search" {
        return format!("fcs find text {}", shell_quote(&entry.label));
    }
    if is_debug_trace_entry(entry) {
        return format!(
            "fcs trace add {} --kind {} --session {}",
            shell_quote(target),
            shell_quote(&entry.kind),
            shell_quote(&report.summary.name)
        );
    }
    format!("fcs find preview {}", shell_quote(&format!("{target}:20")))
}

fn replay_plan_to_markdown(plan: &TraceReplayPlan) -> String {
    let mut output = String::from("# fcs Trace Replay Plan\n\n");
    output.push_str(&format!("- session: {}\n", plan.session));
    output.push_str(&format!("- entries: {}\n\n", plan.entries));
    output.push_str("## Commands\n\n");
    if plan.commands.is_empty() {
        output.push_str("- empty\n");
    } else {
        for command in &plan.commands {
            output.push_str(&format!(
                "{}. `{}` [{}] {}\n",
                command.step, command.entry_id, command.kind, command.command
            ));
        }
    }
    if let Some(debug_command) = &plan.debug_command {
        output.push_str("\n## Debug Profile\n\n");
        output.push_str(&format!("```bash\n{debug_command}\n```\n"));
    }
    output
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_' | ':'))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

fn append_replay_markdown(output: &mut String, replay: &[TraceReplayStep]) {
    output.push_str("\n## Replay\n\n");
    if replay.is_empty() {
        output.push_str("- empty\n");
        return;
    }

    for step in replay {
        let state = step.state.as_deref().unwrap_or("-");
        output.push_str(&format!(
            "{}. `{}` +{}s {} at {} state={}\n",
            step.step, step.entry_id, step.elapsed_secs, step.action, step.target, state
        ));
        if let Some(note) = step.note.as_deref() {
            output.push_str(&format!("   - note: {note}\n"));
        }
    }
}

fn append_structured_markdown(output: &mut String, structured: &TraceStructuredReport) {
    output.push_str("\n## Structured Report\n\n");
    append_structured_section(output, "Hypotheses", &structured.hypotheses);
    append_structured_section(output, "Evidence", &structured.evidence);
    append_structured_section(output, "Conclusions", &structured.conclusions);
    append_structured_section(output, "Open Questions", &structured.open_questions);
}

fn append_structured_section(output: &mut String, title: &str, items: &[TraceStructuredItem]) {
    output.push_str(&format!("### {title}\n\n"));
    if items.is_empty() {
        output.push_str("- none\n\n");
        return;
    }

    for item in items {
        let line = item.line.unwrap_or(1);
        let note = item
            .note
            .as_deref()
            .map(|note| format!(" - {note}"))
            .unwrap_or_default();
        output.push_str(&format!(
            "- `{}` {}:{} - {}{}\n",
            item.id,
            item.path.display(),
            line,
            item.label,
            note
        ));
    }
    output.push('\n');
}

fn session_diff_to_markdown(diff: &TraceSessionDiff) -> String {
    let mut output = String::from("# fcs Trace Session Diff\n\n");
    output.push_str(&format!("- left: {}\n", diff.left_session));
    output.push_str(&format!("- right: {}\n", diff.right_session));
    output.push_str(&format!("- unchanged: {}\n", diff.unchanged));
    output.push_str(&format!("- changed: {}\n", diff.changed.len()));
    output.push_str(&format!("- only_left: {}\n", diff.only_left.len()));
    output.push_str(&format!("- only_right: {}\n\n", diff.only_right.len()));
    append_diff_section(&mut output, "Changed", &diff.changed);
    append_diff_section(&mut output, "Only Left", &diff.only_left);
    append_diff_section(&mut output, "Only Right", &diff.only_right);
    output
}

fn append_diff_section(output: &mut String, title: &str, entries: &[TraceDiffEntry]) {
    if entries.is_empty() {
        return;
    }

    output.push_str(&format!("## {title}\n\n"));
    for entry in entries {
        output.push_str(&format!("- `{}`\n", entry.key));
        for change in &entry.changes {
            output.push_str(&format!("  - {change}\n"));
        }
    }
    output.push('\n');
}

fn format_counts(counts: &BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .map(|(name, count)| format!("{name}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn session_state(summary: &TraceSessionSummary) -> String {
    summary
        .archived_at
        .map(|timestamp| format!("archived at {timestamp}"))
        .unwrap_or_else(|| "active".to_string())
}

#[derive(Debug, Default)]
struct SessionAccumulator {
    entries: usize,
    first_timestamp: u64,
    last_timestamp: u64,
    branches: BTreeSet<String>,
    tags: BTreeSet<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_trace_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("fcs_trace_{name}_{}", std::process::id()))
            .join("trace.toml")
    }

    fn test_entry(id: &str, session: &str, label: &str, line: usize) -> TraceEntry {
        TraceEntry {
            id: id.to_string(),
            timestamp: line as u64,
            workspace: None,
            kind: "bookmark".to_string(),
            label: label.to_string(),
            path: PathBuf::from("src/main.rs"),
            line: Some(line),
            column: None,
            note: None,
            status: None,
            priority: None,
            session: Some(session.to_string()),
            parent: None,
            branch: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn converts_entries_to_code_items() {
        let entry = TraceEntry {
            id: "trace-1".to_string(),
            timestamp: 1,
            workspace: None,
            kind: "bookmark".to_string(),
            label: "main".to_string(),
            path: PathBuf::from("src/main.rs"),
            line: Some(10),
            column: Some(3),
            note: None,
            status: None,
            priority: None,
            session: Some("bug-42".to_string()),
            parent: Some("root".to_string()),
            branch: Some("main".to_string()),
            tags: vec!["hot".to_string()],
        };

        let items = entries_to_items(std::slice::from_ref(&entry));
        assert_eq!(items[0].location.line, Some(10));
        assert!(items[0].display_text().contains("main"));
        assert!(items[0].display_text().contains("[bookmark]"));
        assert!(items[0].display_text().contains("session=bug-42"));
        assert!(format_entry(&entry).contains("tags=hot"));
    }

    #[test]
    fn persists_trace_store_and_filters_workspace_entries() {
        let path = temp_trace_path("workspace_filter");
        let _ = fs::remove_file(&path);
        let root_a = PathBuf::from("/tmp/fcs-a");
        let root_b = PathBuf::from("/tmp/fcs-b");
        let store = TraceStore {
            entries: vec![
                TraceEntry {
                    id: "global".to_string(),
                    timestamp: 1,
                    workspace: None,
                    kind: "global".to_string(),
                    label: "global entry".to_string(),
                    path: PathBuf::from("global.rs"),
                    line: Some(1),
                    column: None,
                    note: None,
                    status: None,
                    priority: None,
                    session: None,
                    parent: None,
                    branch: None,
                    tags: Vec::new(),
                },
                TraceEntry {
                    id: "root-a".to_string(),
                    timestamp: 3,
                    workspace: Some(root_a.clone()),
                    kind: "bookmark".to_string(),
                    label: "workspace a".to_string(),
                    path: PathBuf::from("a.rs"),
                    line: Some(2),
                    column: Some(4),
                    note: Some("checked during test".to_string()),
                    status: None,
                    priority: None,
                    session: Some("session-a".to_string()),
                    parent: Some("global".to_string()),
                    branch: Some("fix".to_string()),
                    tags: vec!["hot".to_string()],
                },
                TraceEntry {
                    id: "root-b".to_string(),
                    timestamp: 2,
                    workspace: Some(root_b),
                    kind: "bookmark".to_string(),
                    label: "workspace b".to_string(),
                    path: PathBuf::from("b.rs"),
                    line: Some(3),
                    column: None,
                    note: None,
                    status: None,
                    priority: None,
                    session: None,
                    parent: None,
                    branch: None,
                    tags: Vec::new(),
                },
            ],
            archived_sessions: Vec::new(),
            active_session: None,
        };

        save_store_to_path(&path, &store).unwrap();
        let loaded = load_store_from_path(&path).unwrap();
        let entries = entries_for_workspace(loaded.entries, &root_a).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label, "workspace a");
        assert_eq!(entries[1].label, "global entry");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn exports_markdown_with_columns_notes_and_metadata() {
        let markdown = entries_to_markdown(vec![TraceEntry {
            id: "trace-7".to_string(),
            timestamp: 7,
            workspace: None,
            kind: "bookmark".to_string(),
            label: "handle_request".to_string(),
            path: PathBuf::from("src/lib.rs"),
            line: Some(12),
            column: Some(5),
            note: Some("entry note".to_string()),
            status: None,
            priority: None,
            session: Some("bug-42".to_string()),
            parent: Some("root".to_string()),
            branch: Some("main".to_string()),
            tags: vec!["regression".to_string(), "hot".to_string()],
        }]);

        assert!(markdown.contains("# fcs Trace Report"));
        assert!(markdown.contains("src/lib.rs:12:5 - handle_request"));
        assert!(markdown.contains("note: entry note"));
        assert!(markdown.contains("session: bug-42"));
        assert!(markdown.contains("tags: regression, hot"));
    }

    #[test]
    fn exports_json_and_graph_with_parent_edges() {
        let entries = vec![
            TraceEntry {
                id: "root".to_string(),
                timestamp: 1,
                workspace: None,
                kind: "bookmark".to_string(),
                label: "root node".to_string(),
                path: PathBuf::from("src/root.rs"),
                line: Some(1),
                column: None,
                note: None,
                status: None,
                priority: None,
                session: Some("bug-42".to_string()),
                parent: None,
                branch: None,
                tags: Vec::new(),
            },
            TraceEntry {
                id: "child".to_string(),
                timestamp: 2,
                workspace: None,
                kind: "reference".to_string(),
                label: "child node".to_string(),
                path: PathBuf::from("src/child.rs"),
                line: Some(7),
                column: Some(2),
                note: None,
                status: None,
                priority: None,
                session: Some("bug-42".to_string()),
                parent: Some("root".to_string()),
                branch: Some("fix".to_string()),
                tags: vec!["hot".to_string()],
            },
        ];

        let json = serde_json::to_string_pretty(&entries).unwrap();
        let graph = entries_to_graph(entries);

        assert!(json.contains("\"id\": \"child\""));
        assert!(graph.contains("<root> -> root"));
        assert!(graph.contains("root -> child"));
        assert!(graph.contains("src/child.rs:7:2 - child node"));
        assert!(graph.contains("branch=fix"));
    }

    #[test]
    fn old_trace_entries_can_omit_new_metadata() {
        let contents = r#"
[[entries]]
timestamp = 7
kind = "bookmark"
label = "legacy"
path = "src/main.rs"
line = 3
"#;

        let store: TraceStore = toml::from_str(contents).unwrap();

        assert_eq!(store.entries.len(), 1);
        assert!(store.entries[0].id.is_empty());
        assert!(store.entries[0].status.is_none());
        assert!(store.entries[0].priority.is_none());
        assert!(store.entries[0].session.is_none());
        assert!(store.entries[0].tags.is_empty());
        assert!(store.archived_sessions.is_empty());
    }

    #[test]
    fn updates_entry_note_status_and_priority_by_id_or_latest() {
        let mut store = TraceStore {
            entries: vec![
                test_entry("old", "bug-1", "old", 1),
                test_entry("new", "bug-1", "new", 2),
            ],
            archived_sessions: Vec::new(),
            active_session: None,
        };

        assert_eq!(
            update_entry_field_in_store(&mut store, "old", |entry| {
                entry.note = normalized_optional_value("checked");
                entry.status = normalized_optional_value("open");
            }),
            TraceEntryChange::Changed
        );
        assert_eq!(
            update_entry_field_in_store(&mut store, "latest", |entry| {
                entry.priority = normalized_optional_value("high");
            }),
            TraceEntryChange::Changed
        );
        assert_eq!(
            update_entry_field_in_store(&mut store, "missing", |_| {}),
            TraceEntryChange::NotFound
        );

        assert_eq!(store.entries[0].note.as_deref(), Some("checked"));
        assert_eq!(store.entries[0].status.as_deref(), Some("open"));
        assert_eq!(store.entries[1].priority.as_deref(), Some("high"));
        assert_eq!(normalized_optional_value("-"), None);
    }

    #[test]
    fn summarizes_sessions_and_marks_archived_sessions() {
        let entries = vec![
            TraceEntry {
                id: "root".to_string(),
                timestamp: 10,
                workspace: None,
                kind: "bookmark".to_string(),
                label: "root".to_string(),
                path: PathBuf::from("src/main.rs"),
                line: Some(1),
                column: None,
                note: None,
                status: None,
                priority: None,
                session: Some("bug-42".to_string()),
                parent: None,
                branch: Some("main".to_string()),
                tags: vec!["hot".to_string()],
            },
            TraceEntry {
                id: "child".to_string(),
                timestamp: 20,
                workspace: None,
                kind: "reference".to_string(),
                label: "child".to_string(),
                path: PathBuf::from("src/lib.rs"),
                line: Some(9),
                column: Some(2),
                note: None,
                status: None,
                priority: None,
                session: Some("bug-42".to_string()),
                parent: Some("root".to_string()),
                branch: Some("fix".to_string()),
                tags: vec!["hot".to_string(), "regression".to_string()],
            },
            TraceEntry {
                id: "default".to_string(),
                timestamp: 5,
                workspace: None,
                kind: "bookmark".to_string(),
                label: "default".to_string(),
                path: PathBuf::from("src/other.rs"),
                line: Some(3),
                column: None,
                note: None,
                status: None,
                priority: None,
                session: None,
                parent: None,
                branch: None,
                tags: Vec::new(),
            },
        ];
        let archived = vec![ArchivedTraceSession {
            name: "bug-42".to_string(),
            archived_at: 30,
        }];

        let summaries = summarize_sessions(&entries, &archived);
        let bug = summaries.iter().find(|summary| summary.name == "bug-42").unwrap();
        let default = summaries.iter().find(|summary| summary.name == "default").unwrap();

        assert_eq!(bug.entries, 2);
        assert_eq!(bug.first_timestamp, 10);
        assert_eq!(bug.last_timestamp, 20);
        assert_eq!(bug.archived_at, Some(30));
        assert_eq!(bug.branches, vec!["fix".to_string(), "main".to_string()]);
        assert_eq!(bug.tags, vec!["hot".to_string(), "regression".to_string()]);
        assert_eq!(default.entries, 1);
        assert!(!format_session(default).contains("archived"));
    }

    #[test]
    fn archives_and_unarchives_sessions_without_duplicates() {
        let mut store = TraceStore {
            entries: vec![TraceEntry {
                id: "root".to_string(),
                timestamp: 1,
                workspace: None,
                kind: "bookmark".to_string(),
                label: "root".to_string(),
                path: PathBuf::from("src/main.rs"),
                line: Some(1),
                column: None,
                note: None,
                status: None,
                priority: None,
                session: Some("bug-42".to_string()),
                parent: None,
                branch: None,
                tags: Vec::new(),
            }],
            archived_sessions: Vec::new(),
            active_session: None,
        };

        assert_eq!(
            archive_session_in_store(&mut store, "missing", 2),
            TraceSessionChange::NotFound
        );
        assert_eq!(
            archive_session_in_store(&mut store, "bug-42", 2),
            TraceSessionChange::Changed
        );
        assert_eq!(
            archive_session_in_store(&mut store, "bug-42", 3),
            TraceSessionChange::Unchanged
        );
        assert_eq!(store.archived_sessions.len(), 1);
        assert_eq!(
            unarchive_session_in_store(&mut store, "bug-42"),
            TraceSessionChange::Changed
        );
        assert!(store.archived_sessions.is_empty());
    }

    #[test]
    fn renders_session_report_markdown() {
        let mut entry = TraceEntry {
            id: "root".to_string(),
            timestamp: 1,
            workspace: None,
            kind: "bookmark".to_string(),
            label: "root".to_string(),
            path: PathBuf::from("src/main.rs"),
            line: Some(10),
            column: Some(4),
            note: Some("checked".to_string()),
            status: Some("open".to_string()),
            priority: None,
            session: Some("bug-42".to_string()),
            parent: None,
            branch: Some("fix".to_string()),
            tags: vec!["hypothesis".to_string(), "evidence".to_string()],
        };
        let replay = entries_to_replay(&[entry.clone()]);
        let structured = entries_to_structured_report(&[entry.clone()]);
        entry.status = None;
        let report = TraceSessionReport {
            summary: TraceSessionSummary {
                name: "bug-42".to_string(),
                entries: 1,
                first_timestamp: 1,
                last_timestamp: 1,
                archived_at: Some(5),
                branches: vec!["fix".to_string()],
                tags: vec!["hot".to_string()],
            },
            entries: vec![entry],
            timeline: Vec::new(),
            replay,
            structured,
            status_counts: BTreeMap::new(),
            priority_counts: BTreeMap::new(),
        };

        let markdown = session_report_to_markdown(&report);

        assert!(markdown.contains("# fcs Trace Session Report"));
        assert!(markdown.contains("state: archived at 5"));
        assert!(markdown.contains("src/main.rs:10:4 - root"));
        assert!(markdown.contains("note: checked"));
        assert!(markdown.contains("## Replay"));
        assert!(markdown.contains("## Structured Report"));
        assert!(markdown.contains("Hypotheses"));
    }

    #[test]
    fn renders_session_timeline_and_diff() {
        let mut left_entry = test_entry("left-1", "left", "shared", 10);
        left_entry.status = Some("open".to_string());
        let mut right_entry = test_entry("right-1", "right", "shared", 10);
        right_entry.status = Some("done".to_string());
        right_entry.priority = Some("high".to_string());
        let only_left = test_entry("left-2", "left", "left-only", 11);
        let only_right = test_entry("right-2", "right", "right-only", 12);
        let left_report = TraceSessionReport {
            summary: TraceSessionSummary {
                name: "left".to_string(),
                entries: 2,
                first_timestamp: 10,
                last_timestamp: 11,
                archived_at: None,
                branches: Vec::new(),
                tags: Vec::new(),
            },
            entries: vec![left_entry.clone(), only_left],
            timeline: entries_to_timeline(&[left_entry.clone()]),
            replay: entries_to_replay(&[left_entry.clone()]),
            structured: entries_to_structured_report(&[left_entry.clone()]),
            status_counts: count_optional_values([left_entry.status.as_deref()].into_iter()),
            priority_counts: BTreeMap::new(),
        };
        let right_report = TraceSessionReport {
            summary: TraceSessionSummary {
                name: "right".to_string(),
                entries: 2,
                first_timestamp: 10,
                last_timestamp: 12,
                archived_at: None,
                branches: Vec::new(),
                tags: Vec::new(),
            },
            entries: vec![right_entry.clone(), only_right],
            timeline: entries_to_timeline(&[right_entry.clone()]),
            replay: entries_to_replay(&[right_entry.clone()]),
            structured: entries_to_structured_report(&[right_entry.clone()]),
            status_counts: count_optional_values([right_entry.status.as_deref()].into_iter()),
            priority_counts: count_optional_values([right_entry.priority.as_deref()].into_iter()),
        };

        let timeline = session_timeline_to_markdown(&right_report);
        let diff = diff_reports(&left_report, &right_report);
        let markdown = session_diff_to_markdown(&diff);

        assert!(timeline.contains("status=done priority=high"));
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.only_left.len(), 1);
        assert_eq!(diff.only_right.len(), 1);
        assert!(markdown.contains("status: open -> done"));
        assert!(markdown.contains("priority: - -> high"));
    }

    #[test]
    fn builds_replay_and_structured_report() {
        let mut first = test_entry("one", "bug", "candidate cause", 10);
        first.tags = vec!["hypothesis".to_string()];
        first.status = Some("open".to_string());
        let mut second = test_entry("two", "bug", "call site", 13);
        second.tags = vec!["evidence".to_string()];
        second.priority = Some("high".to_string());
        let mut third = test_entry("three", "bug", "fixed by guard", 21);
        third.tags = vec!["conclusion".to_string()];
        third.status = Some("resolved".to_string());

        let replay = entries_to_replay(&[first.clone(), second.clone(), third.clone()]);
        let structured = entries_to_structured_report(&[first, second, third]);

        assert_eq!(replay.len(), 3);
        assert_eq!(replay[0].elapsed_secs, 0);
        assert_eq!(replay[1].elapsed_secs, 3);
        assert_eq!(replay[2].elapsed_secs, 8);
        assert_eq!(structured.hypotheses.len(), 1);
        assert_eq!(structured.evidence.len(), 1);
        assert_eq!(structured.conclusions.len(), 1);
        assert_eq!(structured.open_questions.len(), 1);
    }

    #[test]
    fn insights_correlate_trace_entries_with_index_symbols() {
        let mut entry = test_entry("debug-1", "bug", "stopped in run", 12);
        entry.kind = "dap-stop".to_string();
        entry.status = Some("open".to_string());
        let index = crate::index::CodeIndex {
            version: 2,
            root: "/tmp/project".to_string(),
            built_at_unix: 1,
            options: crate::index::IndexOptionsSnapshot::default(),
            files: Vec::new(),
            symbols: vec![crate::index::IndexedSymbol {
                path: "src/main.rs".to_string(),
                line: 10,
                column: Some(1),
                label: "run [function]".to_string(),
                detail: "run [function]".to_string(),
                name: "run".to_string(),
                kind: "function".to_string(),
                language: "rust".to_string(),
                range: crate::index::IndexedSymbolRange {
                    start_line: 10,
                    start_column: 1,
                    end_line: 10,
                    end_column: 4,
                },
                parent: None,
            }],
        };
        let symbols = nearest_symbols_for_entries(&[entry.clone()], Path::new("/tmp/project"), &index);
        let insights = TraceInsightsReport {
            session: "bug".to_string(),
            entries: 1,
            status_counts: count_optional_values([entry.status.as_deref()].into_iter()),
            priority_counts: BTreeMap::new(),
            kind_counts: count_values([entry.kind.as_str()].into_iter()),
            hot_files: vec![TraceInsightCount {
                name: "src/main.rs".to_string(),
                count: 1,
            }],
            debug_events: vec![trace_insight_event(&entry)],
            unresolved_entries: vec![trace_insight_event(&entry)],
            nearest_symbols: symbols,
            suggested_next_steps: vec!["review".to_string()],
        };
        let markdown = session_insights_to_markdown(&insights);

        assert_eq!(insights.nearest_symbols[0].symbol, "run");
        assert!(is_debug_trace_entry(&entry));
        assert!(markdown.contains("# fcs Trace Insights"));
        assert!(markdown.contains("run [function]"));
        assert!(markdown.contains("review"));
    }
}
