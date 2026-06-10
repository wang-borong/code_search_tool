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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TraceStore {
    #[serde(default)]
    entries: Vec<TraceEntry>,
    #[serde(default)]
    archived_sessions: Vec<ArchivedTraceSession>,
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
    record_location_with_workspace_and_metadata(None, location, label, kind, metadata)
}

pub fn record_code_item_for_workspace(root: &Path, item: &CodeItem, kind: &str) -> Result<()> {
    record_location_with_workspace(Some(root), &item.location, item.display_text(), kind)
}

pub fn record_location_for_workspace(root: &Path, location: &Location, label: &str, kind: &str) -> Result<()> {
    record_location_with_workspace(Some(root), location, label, kind)
}

fn record_location_with_workspace(root: Option<&Path>, location: &Location, label: &str, kind: &str) -> Result<()> {
    record_location_with_workspace_and_metadata(root, location, label, kind, TraceMetadata::default())
}

fn record_location_with_workspace_and_metadata(
    root: Option<&Path>,
    location: &Location,
    label: &str,
    kind: &str,
    metadata: TraceMetadata,
) -> Result<()> {
    let mut store = load_store()?;
    let timestamp = now_secs();
    let id = format!("{}-{}", timestamp, store.entries.len() + 1);
    store.entries.push(TraceEntry {
        id,
        timestamp,
        workspace: root.map(Path::to_path_buf),
        kind: kind.to_string(),
        label: label.to_string(),
        path: location.path.clone(),
        line: location.line,
        column: location.column,
        note: None,
        status: None,
        priority: None,
        session: metadata.session,
        parent: metadata.parent,
        branch: metadata.branch,
        tags: metadata.tags,
    });
    save_store(&store)
}

pub fn list() -> Result<Vec<TraceEntry>> {
    sorted_entries(load_store()?.entries)
}

pub fn list_for_workspace(root: &Path) -> Result<Vec<TraceEntry>> {
    entries_for_workspace(load_store()?.entries, root)
}

pub fn list_sessions(include_archived: bool) -> Result<Vec<TraceSessionSummary>> {
    let store = load_store()?;
    Ok(summarize_sessions(&store.entries, &store.archived_sessions)
        .into_iter()
        .filter(|summary| include_archived || !summary.is_archived())
        .collect())
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
    let entries = if let Some(root) = root {
        list_for_workspace(root)?
    } else {
        list()?
    };
    Ok(entries_to_graph(entries))
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

fn entries_to_graph(mut entries: Vec<TraceEntry>) -> String {
    entries.sort_by_key(|entry| entry.timestamp);
    let mut output = String::from("# fcs Trace Graph\n\n");
    for entry in entries {
        let node = if entry.id.is_empty() {
            entry.timestamp.to_string()
        } else {
            entry.id.clone()
        };
        let parent = entry.parent.as_deref().unwrap_or("<root>");
        let line = entry.line.unwrap_or(1);
        let column = entry.column.map(|value| format!(":{value}")).unwrap_or_default();
        let metadata = metadata_summary(&entry)
            .map(|summary| format!(" {{{summary}}}"))
            .unwrap_or_default();
        output.push_str(&format!(
            "- {parent} -> {node} [{}] {}:{}{} - {}{}\n",
            entry.kind,
            entry.path.display(),
            line,
            column,
            entry.label,
            metadata
        ));
    }
    output
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

fn entry_label(entry: &TraceEntry) -> String {
    if entry.id.is_empty() {
        entry.timestamp.to_string()
    } else {
        format!("{} {}", entry.id, entry.timestamp)
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
}
