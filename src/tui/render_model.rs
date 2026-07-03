use ratatui::layout::{Constraint, Direction, Rect};

use super::{query_with_cursor, SourceMode, TuiLayoutPreset};

const SEARCH_STACKED_WIDTH: u16 = 96;
const QUERY_MINIMAL_WIDTH: u16 = 76;
const QUERY_COMPACT_WIDTH: u16 = 112;
const COMMAND_PALETTE_DESCRIPTION_WIDTH: u16 = 84;
const SEARCH_SHORTCUT_HINTS: [(&str, &str); 7] = [
    ("?", "help"),
    ("/", "query"),
    (":", "cmd"),
    ("Tab", "source"),
    ("Enter", "open"),
    ("p", "pin"),
    ("P", "preview"),
];
const SEARCH_COMPACT_SHORTCUT_HINTS: [(&str, &str); 4] =
    [("?", "help"), ("/", "query"), ("Tab", "source"), ("Enter", "open")];
const BALANCED_SHORTCUT_HINTS: [(&str, &str); 7] = [
    ("?", "help"),
    ("/", "query"),
    (":", "cmd"),
    ("Tab", "source"),
    ("p", "pin"),
    ("gd/gr", "nav"),
    ("P", "preview"),
];
const BALANCED_COMPACT_SHORTCUT_HINTS: [(&str, &str); 4] = [("?", "help"), ("/", "query"), (":", "cmd"), ("p", "pin")];
const DEBUG_SHORTCUT_HINTS: [(&str, &str); 7] = [
    ("?", "help"),
    (":dap", "cmd"),
    ("b", "break"),
    ("F5", "cont"),
    ("F10", "next"),
    ("F11", "step"),
    ("Ctrl-F5", "stop"),
];
const DEBUG_COMPACT_SHORTCUT_HINTS: [(&str, &str); 4] =
    [("?", "help"), (":dap", "cmd"), ("b", "break"), ("F5", "cont")];
const TRACE_SHORTCUT_HINTS: [(&str, &str); 7] = [
    ("?", "help"),
    ("a", "bookmark"),
    ("B", "bps"),
    ("n/N", "ops"),
    ("[ ]", "history"),
    (":trace", "cmd"),
    ("P", "preview"),
];
const TRACE_COMPACT_SHORTCUT_HINTS: [(&str, &str); 4] =
    [("?", "help"), ("a", "bookmark"), ("B", "bps"), (":trace", "cmd")];
const SEMANTIC_SHORTCUT_HINTS: [(&str, &str); 7] = [
    ("?", "help"),
    ("gd", "def"),
    ("gr", "refs"),
    ("gt", "type"),
    ("gi", "impl"),
    ("e", "diag"),
    (":trace", "record"),
];
const SEMANTIC_COMPACT_SHORTCUT_HINTS: [(&str, &str); 4] =
    [("?", "help"), ("gd", "def"), ("gr", "refs"), ("e", "diag")];
const MINIMAL_SHORTCUT_HINTS: [(&str, &str); 1] = [("?", "help")];

pub(super) fn header_title(
    workspace: &str,
    mode: SourceMode,
    layout: TuiLayoutPreset,
    trace_session: &str,
    trace_view: &str,
    width: u16,
) -> String {
    let budget = if width < 80 { 36 } else { (width as usize / 2).max(42) };
    compact_middle(
        &header_title_for(workspace, mode, layout, trace_session, trace_view),
        budget,
    )
}

pub(super) fn header_title_for(
    workspace: &str,
    mode: SourceMode,
    layout: TuiLayoutPreset,
    trace_session: &str,
    trace_view: &str,
) -> String {
    format!(
        " fcs | {} | {} | {} | trace {}:{} ",
        workspace,
        mode.short_label(),
        layout.label(),
        trace_session,
        trace_view
    )
}

pub(super) fn header_status_for(semantic_status: &str, status: &str, pending: &[&str], width: u16) -> String {
    let pending = if pending.is_empty() {
        "idle".to_string()
    } else {
        pending.join(",")
    };
    let raw = format!("semantic={semantic_status} pending={pending} status={status}");
    let budget = if width < 80 {
        width.saturating_sub(38) as usize
    } else {
        (width as usize / 2).saturating_sub(4)
    };
    compact_middle(&raw, budget.max(12))
}

pub(super) fn task_sidebar_title(preset: TuiLayoutPreset) -> &'static str {
    match preset {
        TuiLayoutPreset::Debug => "Debug Task",
        TuiLayoutPreset::Trace => "Trace Task",
        TuiLayoutPreset::Semantic => "Semantic Task",
        _ => "Task",
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TaskSidebarContext<'a> {
    pub(super) layout: TuiLayoutPreset,
    pub(super) mode: SourceMode,
    pub(super) trace_view: &'a str,
    pub(super) trace_count: usize,
    pub(super) breakpoints: usize,
    pub(super) pins: usize,
    pub(super) dap_state: &'a str,
    pub(super) dap_profile: &'a str,
    pub(super) selected_label: Option<&'a str>,
    pub(super) pending: bool,
}

pub(super) fn task_sidebar_lines_for(ctx: TaskSidebarContext<'_>) -> Vec<String> {
    match ctx.layout {
        TuiLayoutPreset::Trace => vec![
            format!("session: {} entries", ctx.trace_count),
            format!("view: {}", ctx.trace_view),
            format!("source: {}", ctx.mode.short_label()),
            "bookmark: a".to_string(),
            "semantic: :trace semantic refs".to_string(),
            "breakpoints: B".to_string(),
        ],
        TuiLayoutPreset::Debug => vec![
            format!("state: {}", ctx.dap_state),
            format!(
                "profile: {}",
                if ctx.dap_profile.trim().is_empty() {
                    "none"
                } else {
                    ctx.dap_profile
                }
            ),
            format!("breakpoints: {}", ctx.breakpoints),
            "start: :dap start".to_string(),
            "sync: :dap sync".to_string(),
            "step: F5/F10/F11".to_string(),
        ],
        TuiLayoutPreset::Semantic => {
            let selected = ctx.selected_label.unwrap_or("none");
            vec![
                format!("target: {}", compact_middle(selected, 34)),
                format!("source: {}", ctx.mode.short_label()),
                format!("pending: {}", if ctx.pending { "yes" } else { "no" }),
                "navigate: gd/gr/gt/gi".to_string(),
                "diagnostics: :diag".to_string(),
                "record: :trace semantic refs".to_string(),
            ]
        }
        _ => vec![
            format!("source: {}", ctx.mode.short_label()),
            format!("pins: {}", ctx.pins),
            format!("breakpoints: {}", ctx.breakpoints),
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResultTitleContext {
    pub(super) mode: SourceMode,
    pub(super) selected: usize,
    pub(super) total: usize,
    pub(super) visible_start: usize,
    pub(super) visible_end: usize,
    pub(super) pending: bool,
    pub(super) filter: String,
    pub(super) group: String,
    pub(super) trace_session: String,
    pub(super) trace_view: String,
}

pub(super) fn result_title_for(ctx: ResultTitleContext) -> String {
    let mut parts = vec![format!("Results {}", ctx.mode.short_label())];
    if ctx.pending {
        parts.push("loading".to_string());
    }
    parts.push(format!("{}/{}", ctx.selected, ctx.total));
    if ctx.total > 0 {
        parts.push(format!(
            "showing {}-{}",
            ctx.visible_start.saturating_add(1),
            ctx.visible_end
        ));
    }
    if ctx.mode == SourceMode::Trace {
        parts.push(format!("trace:{}:{}", ctx.trace_session, ctx.trace_view));
    }
    if ctx.filter != "none" {
        parts.push(format!("filter={}", ctx.filter));
    }
    if ctx.group != "none" {
        parts.push(format!("group={}", ctx.group));
    }
    parts.join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TuiFilterSummary {
    label: String,
}

impl From<&super::TuiResultFilter> for TuiFilterSummary {
    fn from(filter: &super::TuiResultFilter) -> Self {
        Self { label: filter.label() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResultEmptyContext<'a> {
    pub(super) mode: SourceMode,
    pub(super) query: &'a str,
    pub(super) pending: bool,
    pub(super) filter: Option<TuiFilterSummary>,
    pub(super) group: &'a str,
    pub(super) trace_count: usize,
    pub(super) pinned_count: usize,
    pub(super) breakpoint_count: usize,
}

pub(super) fn result_empty_messages_for(ctx: ResultEmptyContext<'_>) -> Vec<String> {
    if ctx.pending {
        return vec![
            format!("Loading {} results...", ctx.mode.short_label()),
            "Keep typing to refine, or wait for the current source to finish.".to_string(),
        ];
    }

    let query = ctx.query.trim();
    let mut messages = match ctx.mode {
        SourceMode::Files if query.is_empty() => vec![
            "No files are visible yet.".to_string(),
            "Type / to filter files, or run :refresh to rescan the workspace.".to_string(),
        ],
        SourceMode::Files => vec![
            format!("No files match '{query}'."),
            "Try a shorter path fragment, or clear the query with Ctrl-U.".to_string(),
        ],
        SourceMode::Search if query.is_empty() => vec![
            "Text search is waiting for a query.".to_string(),
            "Type / and enter a term, or press Tab to browse files.".to_string(),
        ],
        SourceMode::Search => vec![
            format!("No text matches for '{query}'."),
            "Try fewer terms, check ignore rules, or switch to source files.".to_string(),
        ],
        SourceMode::Symbols if query.is_empty() => vec![
            "Symbol source is waiting for a query.".to_string(),
            "Type / to search symbols, or press gd/gr from a selected code location.".to_string(),
        ],
        SourceMode::Symbols => vec![
            format!("No symbols match '{query}'."),
            "Try a shorter symbol fragment, or run :source search for text search.".to_string(),
        ],
        SourceMode::References | SourceMode::Diagnostics => vec![
            format!("No {} results are loaded.", ctx.mode.short_label()),
            "Select a code location, then use gd/gr/gt/gi or :diag.".to_string(),
        ],
        SourceMode::Trace if ctx.trace_count == 0 => vec![
            "This trace session has no bookmarks yet.".to_string(),
            "Use a to bookmark the selected location, or :trace session <name>.".to_string(),
        ],
        SourceMode::Trace => vec![
            "Trace results are hidden by the current projection.".to_string(),
            "Try :trace view session, :trace view timeline, or clear filters.".to_string(),
        ],
        SourceMode::Pinned if ctx.pinned_count == 0 => vec![
            "No pinned locations yet.".to_string(),
            "Use p on a result to pin it for later.".to_string(),
        ],
        SourceMode::Pinned => vec![
            "Pinned results are hidden by the current filter.".to_string(),
            "Use :filter clear or switch back to source files.".to_string(),
        ],
        SourceMode::Debug if ctx.breakpoint_count == 0 => vec![
            "No debug items are available yet.".to_string(),
            "Use b to add a breakpoint, or :dap start to begin a session.".to_string(),
        ],
        SourceMode::Debug => vec![
            "Debug source has no current stopped location.".to_string(),
            "Use :dap sync, :dap continue, or :dap start.".to_string(),
        ],
    };

    if let Some(filter) = ctx.filter {
        messages.push(format!("Active filter: {}. Use :filter clear to reset.", filter.label));
    }
    if ctx.group != "none" {
        messages.push(format!(
            "Grouping is set to {}. Use :group none to flatten results.",
            ctx.group
        ));
    }
    messages
}

pub(super) fn preview_empty_messages_for(mode: SourceMode, query: &str, pending: bool) -> Vec<String> {
    if pending {
        return vec![
            "Preview will update when the current source finishes.".to_string(),
            "Keep typing to refine the query, or wait for results.".to_string(),
        ];
    }

    match mode {
        SourceMode::Files if query.trim().is_empty() => vec![
            "Select a file to preview it here.".to_string(),
            "Type / to filter files, then use j/k and Enter to open.".to_string(),
        ],
        SourceMode::Files => vec![
            "No file is selected for preview.".to_string(),
            "Shorten the query or clear it with Ctrl-U to recover results.".to_string(),
        ],
        SourceMode::Search if query.trim().is_empty() => vec![
            "Enter a text query to populate search results.".to_string(),
            "Use source files for browsing when you do not know the text yet.".to_string(),
        ],
        SourceMode::Search => vec![
            "No text match is selected for preview.".to_string(),
            "Try fewer terms or switch to source files.".to_string(),
        ],
        SourceMode::Symbols => vec![
            "Select a symbol to preview its file context.".to_string(),
            "Use a shorter symbol query if the list is empty.".to_string(),
        ],
        SourceMode::References | SourceMode::Diagnostics => vec![
            "Semantic results need a selected code location.".to_string(),
            "Choose a file or symbol, then use gd/gr/gt/gi or :diag.".to_string(),
        ],
        SourceMode::Trace => vec![
            "Trace preview appears after selecting a bookmark.".to_string(),
            "Use a to bookmark locations, or :trace session <name>.".to_string(),
        ],
        SourceMode::Pinned => vec![
            "Pinned preview appears after selecting a pinned location.".to_string(),
            "Use p on any result to add it here.".to_string(),
        ],
        SourceMode::Debug => vec![
            "Debug preview appears at the stopped location.".to_string(),
            "Use :dap start, :dap sync, or b to add breakpoints.".to_string(),
        ],
    }
}

pub(super) fn trace_empty_messages() -> Vec<String> {
    vec![
        "No trace entries yet.".to_string(),
        "Use a to bookmark the selected result.".to_string(),
        "Use :trace semantic refs to record semantic edges.".to_string(),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DebugEmptyPanel {
    Stack,
    Variables,
    Events,
}

pub(super) fn debug_empty_messages(panel: DebugEmptyPanel) -> Vec<String> {
    match panel {
        DebugEmptyPanel::Stack => vec![
            "No stack frames yet.".to_string(),
            "Use :dap start, then pause or stop at a breakpoint.".to_string(),
        ],
        DebugEmptyPanel::Variables => vec![
            "No variables or watches yet.".to_string(),
            "Stop in a frame, then use :watch add <expr> or :eval <expr>.".to_string(),
        ],
        DebugEmptyPanel::Events => vec![
            "No DAP events yet.".to_string(),
            "Use :dap start, :dap sync, or F5/F10/F11.".to_string(),
        ],
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ActivityContext<'a> {
    pub(super) layout: TuiLayoutPreset,
    pub(super) mode: SourceMode,
    pub(super) pending_source: Option<&'a str>,
    pub(super) pending_lsp: Option<&'a str>,
    pub(super) pending_dap: Option<&'a str>,
    pub(super) pending_editor: bool,
    pub(super) preview: &'a str,
    pub(super) status: &'a str,
    pub(super) health: &'a str,
    pub(super) pins: usize,
    pub(super) navigation: usize,
    pub(super) trace_session: &'a str,
    pub(super) trace_view: &'a str,
    pub(super) breakpoints: usize,
}

pub(super) fn activity_source_pending_label(mode: SourceMode, query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        format!("{} refresh", mode.short_label())
    } else {
        format!("{} '{}'", mode.short_label(), compact_middle(trimmed, 28))
    }
}

pub(super) fn activity_lines_for(ctx: ActivityContext<'_>) -> Vec<String> {
    vec![
        format!("work: {}", activity_work_summary(ctx)),
        format!("next: {}", activity_next_step(ctx)),
        format!("preview: {}", compact_middle(ctx.preview, 42)),
        format!("status: {}", compact_middle(ctx.status, 54)),
        format!(
            "saved: pins {} | jumps {} | trace {}:{} | bps {}",
            ctx.pins, ctx.navigation, ctx.trace_session, ctx.trace_view, ctx.breakpoints
        ),
        format!("health: {}", compact_middle(ctx.health, 54)),
    ]
}

fn activity_work_summary(ctx: ActivityContext<'_>) -> String {
    let mut pending = Vec::new();
    if let Some(source) = ctx.pending_source {
        pending.push(source.to_string());
    }
    if let Some(label) = ctx.pending_lsp {
        pending.push(format!("lsp {label}"));
    }
    if let Some(label) = ctx.pending_dap {
        pending.push(format!("dap {label}"));
    }
    if ctx.pending_editor {
        pending.push("editor open".to_string());
    }

    if pending.is_empty() {
        return "ready".to_string();
    }

    compact_middle(&pending.join(" | "), 54)
}

fn activity_next_step(ctx: ActivityContext<'_>) -> &'static str {
    if ctx.pending_source.is_some() || ctx.pending_lsp.is_some() || ctx.pending_dap.is_some() || ctx.pending_editor {
        return "wait for results, or keep typing to replace the request";
    }

    match (ctx.layout, ctx.mode) {
        (TuiLayoutPreset::Debug, _) | (_, SourceMode::Debug) => "use :dap start/sync, b adds breakpoints, F5 continues",
        (TuiLayoutPreset::Trace, _) | (_, SourceMode::Trace) => {
            "use a to bookmark, B imports breakpoints, n/N cycles operations"
        }
        (TuiLayoutPreset::Semantic, _)
        | (_, SourceMode::References | SourceMode::Diagnostics | SourceMode::Symbols) => {
            "select a location, then use gd/gr/gt/gi or :diag"
        }
        (_, SourceMode::Pinned) => "open pinned items, x deletes selected, u unpins",
        (TuiLayoutPreset::Search, SourceMode::Files) => "type to filter files, Enter opens, p pins",
        (TuiLayoutPreset::Search, SourceMode::Search) => "type text to search, Enter opens, Tab switches source",
        _ => "query with /, switch source with Tab, open with Enter",
    }
}

pub(super) fn bottom_panel_height(preset: TuiLayoutPreset) -> u16 {
    match preset {
        TuiLayoutPreset::Search => 0,
        TuiLayoutPreset::Balanced | TuiLayoutPreset::Semantic => 8,
        TuiLayoutPreset::Debug => 12,
        TuiLayoutPreset::Trace => 10,
    }
}

pub(super) fn search_main_direction(area: Rect) -> Direction {
    if area.width < SEARCH_STACKED_WIDTH {
        Direction::Vertical
    } else {
        Direction::Horizontal
    }
}

pub(super) fn search_main_constraints(direction: Direction) -> [Constraint; 2] {
    match direction {
        Direction::Vertical => [Constraint::Percentage(44), Constraint::Percentage(56)],
        _ => [Constraint::Percentage(48), Constraint::Percentage(52)],
    }
}

pub(super) fn main_constraints(preset: TuiLayoutPreset) -> [Constraint; 3] {
    match preset {
        TuiLayoutPreset::Balanced => [
            Constraint::Length(28),
            Constraint::Percentage(42),
            Constraint::Percentage(58),
        ],
        TuiLayoutPreset::Search => [
            Constraint::Length(24),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ],
        TuiLayoutPreset::Debug => [
            Constraint::Length(22),
            Constraint::Percentage(34),
            Constraint::Percentage(66),
        ],
        TuiLayoutPreset::Trace => [
            Constraint::Length(26),
            Constraint::Percentage(38),
            Constraint::Percentage(62),
        ],
        TuiLayoutPreset::Semantic => [
            Constraint::Length(24),
            Constraint::Percentage(46),
            Constraint::Percentage(54),
        ],
    }
}

pub(super) fn source_tab_modes(width: u16, current: SourceMode) -> Vec<SourceMode> {
    if width < QUERY_COMPACT_WIDTH {
        vec![current]
    } else {
        SourceMode::all().to_vec()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortcutHintProfile {
    Search,
    Balanced,
    Debug,
    Trace,
    Semantic,
}

fn shortcut_hint_profile(layout: TuiLayoutPreset, mode: SourceMode) -> ShortcutHintProfile {
    match layout {
        TuiLayoutPreset::Debug => ShortcutHintProfile::Debug,
        TuiLayoutPreset::Trace => ShortcutHintProfile::Trace,
        TuiLayoutPreset::Semantic => ShortcutHintProfile::Semantic,
        TuiLayoutPreset::Search => match mode {
            SourceMode::Debug => ShortcutHintProfile::Debug,
            SourceMode::Trace => ShortcutHintProfile::Trace,
            SourceMode::References | SourceMode::Diagnostics | SourceMode::Symbols => ShortcutHintProfile::Semantic,
            _ => ShortcutHintProfile::Search,
        },
        TuiLayoutPreset::Balanced => match mode {
            SourceMode::Debug => ShortcutHintProfile::Debug,
            SourceMode::Trace => ShortcutHintProfile::Trace,
            SourceMode::References | SourceMode::Diagnostics => ShortcutHintProfile::Semantic,
            _ => ShortcutHintProfile::Balanced,
        },
    }
}

pub(super) fn shortcut_hints_for_context(
    width: u16,
    layout: TuiLayoutPreset,
    mode: SourceMode,
) -> &'static [(&'static str, &'static str)] {
    if width < QUERY_MINIMAL_WIDTH {
        return &MINIMAL_SHORTCUT_HINTS;
    }

    let compact = width < QUERY_COMPACT_WIDTH;
    match (shortcut_hint_profile(layout, mode), compact) {
        (ShortcutHintProfile::Search, false) => &SEARCH_SHORTCUT_HINTS,
        (ShortcutHintProfile::Search, true) => &SEARCH_COMPACT_SHORTCUT_HINTS,
        (ShortcutHintProfile::Balanced, false) => &BALANCED_SHORTCUT_HINTS,
        (ShortcutHintProfile::Balanced, true) => &BALANCED_COMPACT_SHORTCUT_HINTS,
        (ShortcutHintProfile::Debug, false) => &DEBUG_SHORTCUT_HINTS,
        (ShortcutHintProfile::Debug, true) => &DEBUG_COMPACT_SHORTCUT_HINTS,
        (ShortcutHintProfile::Trace, false) => &TRACE_SHORTCUT_HINTS,
        (ShortcutHintProfile::Trace, true) => &TRACE_COMPACT_SHORTCUT_HINTS,
        (ShortcutHintProfile::Semantic, false) => &SEMANTIC_SHORTCUT_HINTS,
        (ShortcutHintProfile::Semantic, true) => &SEMANTIC_COMPACT_SHORTCUT_HINTS,
    }
}

pub(super) fn query_input_width(width: u16, prompt: &str, layout: TuiLayoutPreset, current: SourceMode) -> usize {
    let inner_width = width.saturating_sub(2) as usize;
    let prompt_width = prompt.chars().count() + ": ".chars().count();
    let fixed_width = prompt_width
        + "    ".chars().count()
        + source_tab_width(width, current)
        + "    ".chars().count()
        + shortcut_hints_width(shortcut_hints_for_context(width, layout, current));
    inner_width.saturating_sub(fixed_width)
}

fn source_tab_width(width: u16, current: SourceMode) -> usize {
    source_tab_modes(width, current)
        .iter()
        .enumerate()
        .map(|(index, mode)| {
            let separator_width = usize::from(index > 0);
            let label_width = mode.short_label().chars().count();
            let selection_width = if *mode == current { 2 } else { 0 };
            separator_width + label_width + selection_width
        })
        .sum()
}

fn shortcut_hints_width(hints: &[(&str, &str)]) -> usize {
    hints
        .iter()
        .enumerate()
        .map(|(index, (key, label))| {
            let separator_width = if index > 0 { 2 } else { 0 };
            separator_width + key.chars().count() + 1 + label.chars().count()
        })
        .sum()
}

pub(super) fn query_with_cursor_for_width(query: &str, cursor: usize, active: bool, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let value = query_with_cursor(query, cursor, active);
    if active {
        compact_around_focus(&value, cursor.min(query.chars().count()), max_width)
    } else {
        compact_middle(&value, max_width)
    }
}

pub(super) fn compact_middle(value: &str, max_width: usize) -> String {
    let chars = value.chars().collect::<Vec<char>>();
    if chars.len() <= max_width {
        return value.to_string();
    }
    if max_width <= 6 {
        return chars.into_iter().take(max_width).collect();
    }

    let marker_width = "...".chars().count();
    let body_width = max_width.saturating_sub(marker_width);
    let prefix_width = body_width.saturating_sub(body_width / 2);
    let suffix_width = body_width / 2;
    let prefix = chars.iter().take(prefix_width).collect::<String>();
    let suffix = chars
        .iter()
        .skip(chars.len().saturating_sub(suffix_width))
        .collect::<String>();
    format!("{prefix}...{suffix}")
}

fn compact_around_focus(value: &str, focus: usize, max_width: usize) -> String {
    let chars = value.chars().collect::<Vec<char>>();
    if chars.len() <= max_width {
        return value.to_string();
    }
    if max_width <= 6 {
        let start = focus
            .saturating_sub(max_width / 2)
            .min(chars.len().saturating_sub(max_width));
        return chars.into_iter().skip(start).take(max_width).collect();
    }

    let focus = focus.min(chars.len().saturating_sub(1));
    let marker_width = "...".chars().count();
    let initial_start = focus
        .saturating_sub(max_width / 2)
        .min(chars.len().saturating_sub(max_width));
    let initial_end = (initial_start + max_width).min(chars.len());
    let has_left_marker = initial_start > 0;
    let has_right_marker = initial_end < chars.len();
    let marker_budget = marker_width * usize::from(has_left_marker) + marker_width * usize::from(has_right_marker);
    let body_width = max_width.saturating_sub(marker_budget).max(1);

    let start = if has_left_marker && has_right_marker {
        focus
            .saturating_sub(body_width / 2)
            .min(chars.len().saturating_sub(body_width))
    } else if has_left_marker {
        chars.len().saturating_sub(body_width)
    } else {
        0
    };
    let end = (start + body_width).min(chars.len());

    let mut output = String::new();
    if start > 0 {
        output.push_str("...");
    }
    output.extend(chars[start..end].iter());
    if end < chars.len() {
        output.push_str("...");
    }
    output
}

pub(super) fn bottom_constraints(preset: TuiLayoutPreset) -> [Constraint; 3] {
    match preset {
        TuiLayoutPreset::Balanced | TuiLayoutPreset::Search | TuiLayoutPreset::Semantic => [
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ],
        TuiLayoutPreset::Debug => [
            Constraint::Percentage(24),
            Constraint::Percentage(52),
            Constraint::Percentage(24),
        ],
        TuiLayoutPreset::Trace => [
            Constraint::Percentage(48),
            Constraint::Percentage(28),
            Constraint::Percentage(24),
        ],
    }
}

pub(super) fn command_palette_show_descriptions(width: u16) -> bool {
    width >= COMMAND_PALETTE_DESCRIPTION_WIDTH
}

pub(super) fn command_palette_title(command: &str, width: u16) -> String {
    let fixed_width = "Command Palette ''".chars().count();
    let command_width = (width as usize).saturating_sub(fixed_width);
    format!("Command Palette '{}'", compact_middle(command, command_width))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_layout_stacks_when_terminal_is_narrow() {
        assert_eq!(
            search_main_direction(Rect::new(0, 0, SEARCH_STACKED_WIDTH - 1, 24)),
            Direction::Vertical
        );
        assert_eq!(
            search_main_direction(Rect::new(0, 0, SEARCH_STACKED_WIDTH, 24)),
            Direction::Horizontal
        );
    }

    #[test]
    fn search_layout_uses_stable_result_and_preview_ratios() {
        assert_eq!(
            search_main_constraints(Direction::Horizontal),
            [Constraint::Percentage(48), Constraint::Percentage(52)]
        );
        assert_eq!(
            search_main_constraints(Direction::Vertical),
            [Constraint::Percentage(44), Constraint::Percentage(56)]
        );
    }

    #[test]
    fn header_title_uses_compact_source_labels() {
        let title = header_title_for(
            "code_search_tool",
            SourceMode::Files,
            TuiLayoutPreset::Search,
            "investigation",
            "timeline",
        );

        assert_eq!(
            title,
            " fcs | code_search_tool | files | search | trace investigation:timeline "
        );
    }

    #[test]
    fn header_status_shows_pending_workers() {
        let status = header_status_for("rust ready", "Searching files", &["source", "lsp"], 180);

        assert!(status.contains("semantic=rust ready"));
        assert!(status.contains("pending=source,lsp"));
        assert!(status.contains("status=Searching files"));
    }

    #[test]
    fn header_status_compacts_on_narrow_terminals() {
        let status = header_status_for(
            "rust-analyzer ready",
            "A very long status message that should not consume the whole header",
            &[],
            60,
        );

        assert!(status.contains("..."));
        assert!(status.chars().count() <= 22);
    }

    #[test]
    fn query_chrome_compacts_when_terminal_is_narrow() {
        assert_eq!(
            source_tab_modes(QUERY_COMPACT_WIDTH - 1, SourceMode::Files),
            vec![SourceMode::Files]
        );
        assert_eq!(
            source_tab_modes(QUERY_COMPACT_WIDTH, SourceMode::Files),
            SourceMode::all().to_vec()
        );
        assert_eq!(
            shortcut_hints_for_context(QUERY_COMPACT_WIDTH - 1, TuiLayoutPreset::Search, SourceMode::Files),
            SEARCH_COMPACT_SHORTCUT_HINTS.as_slice()
        );
        assert_eq!(
            shortcut_hints_for_context(QUERY_MINIMAL_WIDTH - 1, TuiLayoutPreset::Search, SourceMode::Files),
            MINIMAL_SHORTCUT_HINTS.as_slice()
        );
        assert_eq!(
            shortcut_hints_for_context(QUERY_COMPACT_WIDTH, TuiLayoutPreset::Search, SourceMode::Files),
            SEARCH_SHORTCUT_HINTS.as_slice()
        );
    }

    #[test]
    fn query_chrome_uses_contextual_shortcuts() {
        assert_eq!(
            shortcut_hints_for_context(QUERY_COMPACT_WIDTH, TuiLayoutPreset::Debug, SourceMode::Files),
            DEBUG_SHORTCUT_HINTS.as_slice()
        );
        assert_eq!(
            shortcut_hints_for_context(QUERY_COMPACT_WIDTH, TuiLayoutPreset::Balanced, SourceMode::Trace),
            TRACE_SHORTCUT_HINTS.as_slice()
        );
        assert_eq!(
            shortcut_hints_for_context(QUERY_COMPACT_WIDTH, TuiLayoutPreset::Search, SourceMode::References),
            SEMANTIC_SHORTCUT_HINTS.as_slice()
        );
    }

    #[test]
    fn result_title_shows_source_loading_and_visible_range() {
        let title = result_title_for(ResultTitleContext {
            mode: SourceMode::Files,
            selected: 4,
            total: 20,
            visible_start: 0,
            visible_end: 9,
            pending: true,
            filter: "none".to_string(),
            group: "none".to_string(),
            trace_session: "tui".to_string(),
            trace_view: "session".to_string(),
        });

        assert_eq!(title, "Results files loading 4/20 showing 1-9");
    }

    #[test]
    fn result_title_includes_trace_projection_and_grouping() {
        let title = result_title_for(ResultTitleContext {
            mode: SourceMode::Trace,
            selected: 2,
            total: 8,
            visible_start: 1,
            visible_end: 6,
            pending: false,
            filter: "kind=file".to_string(),
            group: "path".to_string(),
            trace_session: "investigation".to_string(),
            trace_view: "timeline".to_string(),
        });

        assert!(title.contains("trace:investigation:timeline"));
        assert!(title.contains("filter=kind=file"));
        assert!(title.contains("group=path"));
    }

    #[test]
    fn result_title_omits_visible_range_when_empty() {
        let title = result_title_for(ResultTitleContext {
            mode: SourceMode::Search,
            selected: 0,
            total: 0,
            visible_start: 0,
            visible_end: 0,
            pending: false,
            filter: "none".to_string(),
            group: "none".to_string(),
            trace_session: "tui".to_string(),
            trace_view: "session".to_string(),
        });

        assert_eq!(title, "Results search 0/0");
    }

    #[test]
    fn long_inactive_query_compacts_in_the_middle() {
        assert_eq!(
            query_with_cursor_for_width("abcdefghijklmnop", 0, false, 10),
            "abcd...nop"
        );
    }

    #[test]
    fn long_active_query_keeps_the_cursor_visible() {
        let rendered = query_with_cursor_for_width("src/very/deep/module/main.rs", 16, true, 14);

        assert!(rendered.contains('|'));
        assert!(rendered.starts_with("..."));
        assert!(rendered.ends_with("..."));
        assert!(rendered.chars().count() <= 14);
    }

    #[test]
    fn query_width_reserves_room_for_current_source_and_hints() {
        assert!(query_input_width(80, "QUERY", TuiLayoutPreset::Search, SourceMode::Files) > 0);
        assert!(query_input_width(80, "QUERY", TuiLayoutPreset::Search, SourceMode::Files) < 80);
    }

    #[test]
    fn command_palette_hides_descriptions_when_narrow() {
        assert!(!command_palette_show_descriptions(
            COMMAND_PALETTE_DESCRIPTION_WIDTH - 1
        ));
        assert!(command_palette_show_descriptions(COMMAND_PALETTE_DESCRIPTION_WIDTH));
    }

    #[test]
    fn command_palette_title_compacts_long_input() {
        let title = command_palette_title("trace semantic references for a deeply nested symbol", 32);

        assert!(title.contains("..."));
        assert!(title.chars().count() <= 32);
    }

    fn empty_context(mode: SourceMode, query: &str) -> ResultEmptyContext<'_> {
        ResultEmptyContext {
            mode,
            query,
            pending: false,
            filter: None,
            group: "none",
            trace_count: 0,
            pinned_count: 0,
            breakpoint_count: 0,
        }
    }

    #[test]
    fn empty_results_explain_pending_source() {
        let messages = result_empty_messages_for(ResultEmptyContext {
            pending: true,
            ..empty_context(SourceMode::Files, "main")
        });

        assert_eq!(messages[0], "Loading files results...");
        assert!(messages[1].contains("Keep typing"));
    }

    #[test]
    fn empty_results_explain_search_without_query() {
        let messages = result_empty_messages_for(empty_context(SourceMode::Search, ""));

        assert!(messages[0].contains("waiting for a query"));
        assert!(messages[1].contains("browse files"));
    }

    #[test]
    fn empty_results_explain_file_query_miss() {
        let messages = result_empty_messages_for(empty_context(SourceMode::Files, "missing.rs"));

        assert_eq!(messages[0], "No files match 'missing.rs'.");
        assert!(messages[1].contains("shorter path fragment"));
    }

    #[test]
    fn empty_results_include_projection_hints() {
        let messages = result_empty_messages_for(ResultEmptyContext {
            trace_count: 3,
            filter: Some(TuiFilterSummary {
                label: "kind=file".to_string(),
            }),
            group: "path",
            ..empty_context(SourceMode::Trace, "")
        });

        assert!(messages.iter().any(|message| message.contains("filter clear")));
        assert!(messages.iter().any(|message| message.contains("group none")));
    }

    #[test]
    fn empty_results_explain_pins_and_debug_start_states() {
        let pins = result_empty_messages_for(empty_context(SourceMode::Pinned, ""));
        let debug = result_empty_messages_for(empty_context(SourceMode::Debug, ""));

        assert!(pins[0].contains("No pinned"));
        assert!(pins[1].contains("Use p"));
        assert!(debug[0].contains("No debug"));
        assert!(debug[1].contains(":dap start"));
    }

    #[test]
    fn empty_preview_explains_pending_source() {
        let messages = preview_empty_messages_for(SourceMode::Search, "main", true);

        assert!(messages[0].contains("will update"));
        assert!(messages[1].contains("Keep typing"));
    }

    #[test]
    fn empty_preview_guides_file_browsing() {
        let messages = preview_empty_messages_for(SourceMode::Files, "", false);

        assert!(messages[0].contains("Select a file"));
        assert!(messages[1].contains("filter files"));
    }

    #[test]
    fn empty_preview_guides_semantic_and_debug_sources() {
        let semantic = preview_empty_messages_for(SourceMode::References, "", false);
        let debug = preview_empty_messages_for(SourceMode::Debug, "", false);

        assert!(semantic[0].contains("Semantic results"));
        assert!(semantic[1].contains("gd/gr"));
        assert!(debug[0].contains("stopped location"));
        assert!(debug[1].contains(":dap start"));
    }

    #[test]
    fn task_sidebar_describes_trace_workflow() {
        let lines = task_sidebar_lines_for(TaskSidebarContext {
            layout: TuiLayoutPreset::Trace,
            mode: SourceMode::Trace,
            trace_view: "timeline",
            trace_count: 4,
            breakpoints: 0,
            pins: 0,
            dap_state: "idle",
            dap_profile: "",
            selected_label: None,
            pending: false,
        });

        assert!(lines.contains(&"session: 4 entries".to_string()));
        assert!(lines.contains(&"view: timeline".to_string()));
        assert!(lines.contains(&"semantic: :trace semantic refs".to_string()));
    }

    #[test]
    fn task_sidebar_describes_debug_workflow() {
        let lines = task_sidebar_lines_for(TaskSidebarContext {
            layout: TuiLayoutPreset::Debug,
            mode: SourceMode::Debug,
            trace_view: "session",
            trace_count: 0,
            breakpoints: 2,
            pins: 0,
            dap_state: "stopped",
            dap_profile: "",
            selected_label: None,
            pending: false,
        });

        assert!(lines.contains(&"state: stopped".to_string()));
        assert!(lines.contains(&"profile: none".to_string()));
        assert!(lines.contains(&"breakpoints: 2".to_string()));
        assert!(lines.contains(&"step: F5/F10/F11".to_string()));
    }

    #[test]
    fn task_sidebar_describes_semantic_workflow() {
        let lines = task_sidebar_lines_for(TaskSidebarContext {
            layout: TuiLayoutPreset::Semantic,
            mode: SourceMode::Symbols,
            trace_view: "session",
            trace_count: 0,
            breakpoints: 0,
            pins: 0,
            dap_state: "idle",
            dap_profile: "",
            selected_label: Some("src/some/really/deep/module/with_a_long_symbol_name.rs:42"),
            pending: true,
        });

        assert!(lines[0].starts_with("target: "));
        assert!(lines[0].contains("..."));
        assert!(lines.contains(&"pending: yes".to_string()));
        assert!(lines.contains(&"navigate: gd/gr/gt/gi".to_string()));
    }

    #[test]
    fn activity_summary_surfaces_pending_work_and_next_step() {
        let lines = activity_lines_for(ActivityContext {
            layout: TuiLayoutPreset::Search,
            mode: SourceMode::Files,
            pending_source: Some("files 'src/main.rs'"),
            pending_lsp: None,
            pending_dap: Some("Start"),
            pending_editor: false,
            preview: "Preview src/main.rs",
            status: "Files: searching...",
            health: "semantic ready",
            pins: 2,
            navigation: 1,
            trace_session: "tui",
            trace_view: "session",
            breakpoints: 3,
        });

        assert_eq!(lines[0], "work: files 'src/main.rs' | dap Start");
        assert!(lines[1].contains("wait for results"));
        assert!(lines[4].contains("pins 2"));
        assert!(lines[4].contains("bps 3"));
    }

    #[test]
    fn activity_summary_uses_workflow_specific_next_steps() {
        let debug = activity_lines_for(ActivityContext {
            layout: TuiLayoutPreset::Debug,
            mode: SourceMode::Files,
            pending_source: None,
            pending_lsp: None,
            pending_dap: None,
            pending_editor: false,
            preview: "Debug",
            status: "ready",
            health: "healthy",
            pins: 0,
            navigation: 0,
            trace_session: "tui",
            trace_view: "session",
            breakpoints: 0,
        });
        let semantic = activity_lines_for(ActivityContext {
            layout: TuiLayoutPreset::Search,
            mode: SourceMode::References,
            pending_source: None,
            pending_lsp: None,
            pending_dap: None,
            pending_editor: false,
            preview: "References",
            status: "ready",
            health: "healthy",
            pins: 0,
            navigation: 0,
            trace_session: "tui",
            trace_view: "session",
            breakpoints: 0,
        });

        assert!(debug[1].contains(":dap start"));
        assert!(semantic[1].contains("gd/gr/gt/gi"));
    }

    #[test]
    fn trace_empty_panel_guides_bookmarking_and_semantic_recording() {
        let messages = trace_empty_messages();

        assert!(messages[0].contains("No trace"));
        assert!(messages.iter().any(|message| message.contains("Use a")));
        assert!(messages.iter().any(|message| message.contains(":trace semantic refs")));
    }

    #[test]
    fn debug_empty_panels_explain_next_actions() {
        let stack = debug_empty_messages(DebugEmptyPanel::Stack);
        let variables = debug_empty_messages(DebugEmptyPanel::Variables);
        let events = debug_empty_messages(DebugEmptyPanel::Events);

        assert!(stack.iter().any(|message| message.contains(":dap start")));
        assert!(variables.iter().any(|message| message.contains(":watch add")));
        assert!(events.iter().any(|message| message.contains("F5/F10/F11")));
    }
}
