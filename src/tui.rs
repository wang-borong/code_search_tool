mod actions;
mod dap_worker;
mod highlight;
mod lsp_worker;
mod preview_cache;
mod render;
mod sources;
mod state;

use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::config::Config;
use crate::core::{CodeItem, CodeItemKind, Location};
use crate::errors::Result;
use actions::{action_for_key, goto_action_for_key, AppAction};
use dap_worker::{DapCommand, DapWorker};
use lsp_worker::{LspCommand, LspPayload, LspWorker};
use preview_cache::{PreviewCache, PreviewWindow};
use sources::{
    fuzzy_score, parse_mode, resolve_ignore_file, source_mode_after, tracking_mode_after, SourceMode, SourceWorker,
};
#[cfg(test)]
use sources::{SourceRequest, SourceResponse};
use state::{TuiPersistentState, TuiSavedBreakpoint, TuiSavedItem, TuiSavedLocation};

const HELP_TEXT: &str = "? help  / query  : command  n/N cycle  p pin  [/ ] back/fwd  gd/gr jump  Enter open";
const HELP_OVERLAY_TEXT: &str = "\
fcs workbench

Navigation
  j/k or arrows       move result selection
  enter or o          open selected location
  [ / ]               jump stack back / forward
  n / N               cycle search -> refs -> symbols -> diag -> debug

Search and semantic tracing
  /                   edit query and refresh current source
  tab / shift-tab     cycle all sources
  gd / gr / gt / gi   definition / references / type / implementation
  W / s / e           workspace symbols / document symbols / diagnostics
  c / C / h           incoming calls / outgoing calls / hover

Pins, trace, debug
  p / u               pin / unpin selected result
  a / b / B           bookmark / breakpoint / trace breakpoints
  D / X               debug source / run debug profile
  : dap smoke         run a mock DAP session and show threads/stack/variables
  : dap start         start interactive mock DAP session
  : dap start <name>  start a saved DAP profile in mock mode
  : dap real <cmd>    start the current debug profile with a real DAP adapter
  : dap sync          synchronize current TUI breakpoints to the DAP session
  : dap next          continue/pause/step-in/step-out/restart/stop/jump also supported
  : dap adapters      show discovered adapter commands in the status line
  : watch add <expr>  evaluate a watch expression on refresh
  : watch clear       remove all watch expressions
  : eval <expr>       evaluate once in the selected frame
  F5/F10/F11          continue / next / step-in (shift-F11 step-out, ctrl-F5 stop)
  x                   delete selected debug item

Command palette
  :                   open palette with fuzzy suggestions
  tab                 complete best palette command
  examples            source symbols, query parser, pin, pins, cycle, preview lock
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusLevel {
    Info,
    Warning,
    Error,
}

struct AppState {
    config: Config,
    root: PathBuf,
    ignore_path: PathBuf,
    source_worker: SourceWorker,
    pending_source: Option<(u64, SourceMode, String)>,
    lsp_worker: LspWorker,
    pending_lsp: Option<(u64, &'static str)>,
    dap_worker: DapWorker,
    pending_dap: Option<(u64, &'static str)>,
    mode: SourceMode,
    query: String,
    query_cursor: usize,
    query_history: Vec<String>,
    query_history_index: Option<usize>,
    input_active: bool,
    command_active: bool,
    help_visible: bool,
    command: String,
    command_cursor: usize,
    command_history: Vec<String>,
    command_history_index: Option<usize>,
    results: Vec<CodeItem>,
    selected: usize,
    pinned_items: Vec<CodeItem>,
    trace_items: Vec<CodeItem>,
    breakpoints: Vec<crate::dap::DapBreakpoint>,
    debug_profiles: Vec<crate::debugger::DebugProfile>,
    dap_snapshot: crate::dap::DapSessionSnapshot,
    navigation: Vec<CodeItem>,
    navigation_index: Option<usize>,
    locked_preview: Option<Location>,
    preview_scroll: isize,
    preview_cache: RefCell<PreviewCache>,
    debug_binary: PathBuf,
    semantic_status: String,
    startup_plan: Option<crate::workspace::WorkspaceStartupPlan>,
    status: String,
    status_level: StatusLevel,
    should_quit: bool,
    pending_g: bool,
    pending_debug_run: bool,
}

impl AppState {
    fn new(
        config: Config,
        directory: Option<String>,
        mode: Option<SourceMode>,
        query: Option<String>,
        debug_binary: Option<String>,
    ) -> Result<Self> {
        let root = crate::workspace::resolve_root(directory.as_ref())?;
        let persisted_state = state::load(&root).unwrap_or_default().unwrap_or_default();
        let mode = mode.or_else(|| persisted_state.mode()).unwrap_or(SourceMode::Search);
        let query = query.unwrap_or_else(|| persisted_state.query.clone());
        let pinned_items = saved_items_to_code_items(persisted_state.pinned_items.clone());
        let navigation = saved_items_to_code_items(persisted_state.navigation.clone());
        let navigation_index = persisted_state
            .navigation_index
            .filter(|index| *index < navigation.len());
        let breakpoints = if persisted_state.debug_breakpoints.is_empty() {
            persisted_state
                .breakpoints
                .clone()
                .into_iter()
                .map(|location| crate::dap::DapBreakpoint::from_location(&location.into_location()))
                .collect::<Vec<crate::dap::DapBreakpoint>>()
        } else {
            persisted_state
                .debug_breakpoints
                .clone()
                .into_iter()
                .map(TuiSavedBreakpoint::into_breakpoint)
                .collect::<Vec<crate::dap::DapBreakpoint>>()
        };
        let locked_preview = persisted_state
            .locked_preview
            .clone()
            .map(TuiSavedLocation::into_location);
        let mut config = config;
        let project_config = crate::workspace::read_project_config(&root)?;
        if let Some(project_config) = &project_config {
            config.lsp.clangd_command = project_config.clangd_command.clone();
            for pattern in &project_config.search_ignore {
                if !config.search.ignore.contains(pattern) {
                    config.search.ignore.push(pattern.clone());
                }
            }
        }
        let ignore_path = resolve_ignore_file(&root);
        let workspace_status =
            crate::workspace::status(Some(&root.to_string_lossy().to_string()), &config.lsp.clangd_command)?;
        let semantic_status = workspace_status.semantic_status_label().to_string();
        let startup_plan = crate::workspace::startup_plan(&root, &config).ok();
        let lsp_worker = LspWorker::start(root.clone(), config.clone());
        let mut state = Self {
            config,
            root,
            ignore_path,
            source_worker: SourceWorker::start(),
            pending_source: None,
            lsp_worker,
            pending_lsp: None,
            dap_worker: DapWorker::start(),
            pending_dap: None,
            mode,
            query,
            query_cursor: 0,
            query_history: crate::history::list()
                .map(|entries| entries.into_iter().map(|entry| entry.query).collect())
                .unwrap_or_default(),
            query_history_index: None,
            input_active: false,
            command_active: false,
            help_visible: false,
            command: String::new(),
            command_cursor: 0,
            command_history: persisted_state.command_history,
            command_history_index: None,
            results: Vec::new(),
            selected: 0,
            pinned_items,
            trace_items: Vec::new(),
            breakpoints,
            debug_profiles: Vec::new(),
            dap_snapshot: default_dap_snapshot(),
            navigation,
            navigation_index,
            locked_preview,
            preview_scroll: persisted_state.preview_scroll,
            preview_cache: RefCell::new(PreviewCache::new(128)),
            debug_binary: debug_binary
                .map(PathBuf::from)
                .or_else(|| {
                    project_config
                        .as_ref()
                        .map(|project_config| PathBuf::from(&project_config.default_debug_binary))
                })
                .unwrap_or_else(|| PathBuf::from("target/debug/app")),
            semantic_status,
            startup_plan,
            status: "Ready".to_string(),
            status_level: StatusLevel::Info,
            should_quit: false,
            pending_g: false,
            pending_debug_run: false,
        };
        state.query_cursor = state.query.chars().count();
        state.refresh_trace_items();
        state.refresh_debug_profiles();
        state.refresh()?;
        Ok(state)
    }

    fn refresh(&mut self) -> Result<()> {
        let selected_location = self.current_location();
        self.selected = 0;
        self.results = match self.mode {
            SourceMode::Search | SourceMode::Files | SourceMode::Symbols => {
                let id =
                    self.source_worker
                        .request(self.mode, &self.root, &self.ignore_path, &self.query, &self.config)?;
                self.pending_source = Some((id, self.mode, self.query.clone()));
                self.status = format!("{}: searching...", self.mode.label());
                return Ok(());
            }
            SourceMode::References => self.reference_results(selected_location)?,
            SourceMode::Diagnostics => self.diagnostic_results(selected_location)?,
            SourceMode::Trace => self.trace_items.clone(),
            SourceMode::Pinned => self.pinned_items.clone(),
            SourceMode::Debug => {
                self.refresh_debug_profiles();
                self.debug_results()
            }
        };
        self.status = format!("{}: {} result(s)", self.mode.label(), self.results.len());
        Ok(())
    }

    fn poll_source_worker(&mut self) {
        let Some(response) = self.source_worker.try_recv_latest() else {
            return;
        };

        self.pending_source = None;
        match response.result {
            Ok(items) => {
                self.selected = 0;
                self.results = items;
                self.status = format!(
                    "{}: {} result(s) for '{}'",
                    response.mode.label(),
                    self.results.len(),
                    response.query
                );
            }
            Err(err) => {
                self.results.clear();
                self.status = err.to_string();
            }
        }
    }

    fn current_item(&self) -> Option<&CodeItem> {
        self.results.get(self.selected)
    }

    fn current_location(&self) -> Option<Location> {
        self.current_item().map(|item| item.location.clone())
    }

    fn reference_results(&mut self, location: Option<Location>) -> Result<Vec<CodeItem>> {
        let Some(location) = location else {
            return Ok(Vec::new());
        };
        self.queue_lsp("References", LspCommand::References(location))?;
        Ok(Vec::new())
    }

    fn diagnostic_results(&mut self, location: Option<Location>) -> Result<Vec<CodeItem>> {
        let path = location
            .map(|location| location.path)
            .unwrap_or_else(|| self.root.clone());
        let path = if path.is_file() {
            path
        } else {
            self.root.join("src").join("main.rs")
        };
        if !path.exists() {
            return Ok(Vec::new());
        }

        self.queue_lsp("Diagnostics", LspCommand::Diagnostics(path))?;
        Ok(Vec::new())
    }

    fn debug_results(&self) -> Vec<CodeItem> {
        let mut items = self
            .debug_profiles
            .iter()
            .map(|profile| {
                let session = crate::debugger::DebugSession::from_profile(profile);
                CodeItem::symbol(
                    self.root.clone(),
                    self.root.to_string_lossy().replace('\\', "/"),
                    1,
                    None,
                    format!("profile {}: {}", profile.name, session.command_preview()),
                    "debug-profile",
                )
            })
            .collect::<Vec<CodeItem>>();

        items.extend(
            self.breakpoints
                .iter()
                .enumerate()
                .map(|(index, breakpoint)| {
                    let label = breakpoint_label(index, breakpoint);
                    CodeItem::symbol(
                        breakpoint.path.clone(),
                        breakpoint.path.to_string_lossy().replace('\\', "/"),
                        breakpoint.line,
                        breakpoint.column,
                        label,
                        "debug",
                    )
                })
                .collect::<Vec<CodeItem>>(),
        );
        items
    }

    fn queue_lsp(&mut self, label: &'static str, command: LspCommand) -> Result<()> {
        let id = self.lsp_worker.request(command)?;
        self.pending_lsp = Some((id, label));
        self.status = format!("{label}: pending...");
        Ok(())
    }

    fn poll_lsp_worker(&mut self) {
        let Some(response) = self.lsp_worker.try_recv_latest() else {
            return;
        };

        self.pending_lsp = None;
        match response.result {
            Ok(LspPayload::Items(items)) => {
                self.selected = 0;
                self.results = items;
                self.status = format!("{}: {} result(s)", response.label, self.results.len());
                if response.label == "Diagnostics" {
                    self.mode = SourceMode::Diagnostics;
                } else if response.label == "Workspace Symbols" || response.label == "Document Symbols" {
                    self.mode = SourceMode::Symbols;
                } else {
                    self.mode = SourceMode::References;
                }
            }
            Ok(LspPayload::Text(text)) => {
                self.status = compact_status(&text);
            }
            Err(err) => {
                self.status = err.to_string();
            }
        }
    }

    fn refresh_trace_items(&mut self) {
        self.trace_items = crate::trace::list_for_workspace(&self.root)
            .map(|entries| crate::trace::entries_to_items(&entries))
            .unwrap_or_default();
    }

    fn refresh_debug_profiles(&mut self) {
        self.debug_profiles = crate::debugger::list_profiles(&self.root).unwrap_or_default();
    }

    fn persistent_state(&self) -> TuiPersistentState {
        let mut command_history = self.command_history.clone();
        const MAX_SAVED_COMMAND_HISTORY: usize = 50;
        if command_history.len() > MAX_SAVED_COMMAND_HISTORY {
            command_history = command_history[command_history.len() - MAX_SAVED_COMMAND_HISTORY..].to_vec();
        }

        TuiPersistentState {
            mode: Some(self.mode.short_label().to_string()),
            query: self.query.clone(),
            pinned_items: self.pinned_items.iter().map(TuiSavedItem::from_code_item).collect(),
            navigation: self.navigation.iter().map(TuiSavedItem::from_code_item).collect(),
            navigation_index: self.navigation_index.filter(|index| *index < self.navigation.len()),
            breakpoints: self
                .breakpoints
                .iter()
                .map(|breakpoint| {
                    TuiSavedLocation::from_location(&Location::new(
                        breakpoint.path.clone(),
                        Some(breakpoint.line),
                        breakpoint.column,
                    ))
                })
                .collect(),
            debug_breakpoints: self
                .breakpoints
                .iter()
                .map(TuiSavedBreakpoint::from_breakpoint)
                .collect(),
            locked_preview: self.locked_preview.as_ref().map(TuiSavedLocation::from_location),
            preview_scroll: self.preview_scroll,
            command_history,
            ..TuiPersistentState::default()
        }
    }

    fn save_persistent_state(&self) -> Result<()> {
        state::save(&self.root, &self.persistent_state()).map(|_| ())
    }

    fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_level = StatusLevel::Info;
    }

    fn set_warning(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_level = StatusLevel::Warning;
    }

    fn set_error(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_level = StatusLevel::Error;
    }

    fn next_source(&mut self) -> Result<()> {
        self.mode = source_mode_after(self.mode, 1);
        self.preview_scroll = 0;
        self.locked_preview = None;
        self.refresh()
    }

    fn prev_source(&mut self) -> Result<()> {
        self.mode = source_mode_after(self.mode, -1);
        self.preview_scroll = 0;
        self.locked_preview = None;
        self.refresh()
    }

    fn set_source(&mut self, mode: SourceMode) -> Result<()> {
        self.mode = mode;
        self.preview_scroll = 0;
        self.locked_preview = None;
        self.refresh()
    }

    fn move_selection(&mut self, delta: isize) {
        self.selected = selection_after(self.selected, self.results.len(), delta);
        if self.locked_preview.is_none() {
            self.preview_scroll = 0;
        }
    }

    fn pin_selected(&mut self) {
        let Some(item) = self.current_item().cloned() else {
            self.set_warning("No selected item to pin");
            return;
        };

        if self.pinned_items.iter().any(|pinned| same_code_item(pinned, &item)) {
            self.set_warning(format!("Already pinned {}", item.display_text()));
            return;
        }

        self.pinned_items.push(item.clone());
        self.push_navigation(item.clone());
        self.set_status(format!("Pinned {}", item.display_text()));
    }

    fn unpin_selected(&mut self) {
        let Some(item) = self.current_item().cloned() else {
            self.set_warning("No selected item to unpin");
            return;
        };

        let Some(index) = self
            .pinned_items
            .iter()
            .position(|pinned| same_code_item(pinned, &item))
        else {
            self.set_warning(format!("Selected item is not pinned: {}", item.display_text()));
            return;
        };

        self.pinned_items.remove(index);
        self.set_status(format!("Unpinned {}", item.display_text()));
    }

    fn load_pinned_results(&mut self) {
        self.mode = SourceMode::Pinned;
        self.results = self.pinned_items.clone();
        self.selected = 0;
        self.preview_scroll = 0;
        self.set_status(format!("Pins: {} result(s)", self.results.len()));
    }

    fn open_selected(&mut self) -> Result<()> {
        let Some(item) = self.current_item().cloned() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        crate::trace::record_code_item_for_workspace(&self.root, &item, "tui-open")?;
        self.push_navigation(item.clone());
        crate::editor::open_location(&item.location, self.config.editor.command.as_deref())?;
        self.refresh_trace_items();
        self.status = format!("Opened {}", item.display_text());
        Ok(())
    }

    fn add_trace(&mut self) -> Result<()> {
        let Some(item) = self.current_item().cloned() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        crate::trace::record_code_item_for_workspace(&self.root, &item, "bookmark")?;
        self.push_navigation(item.clone());
        self.refresh_trace_items();
        self.status = format!("Bookmarked {}", item.display_text());
        Ok(())
    }

    fn add_breakpoint(&mut self) {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return;
        };

        self.breakpoints
            .push(crate::dap::DapBreakpoint::from_location(&location));
        self.status = format!("Breakpoint count: {}", self.breakpoints.len());
    }

    fn add_advanced_breakpoint(&mut self, kind: &str, value: &str) {
        let Some(location) = self.current_location() else {
            self.set_warning("No selected item for breakpoint");
            return;
        };

        let mut breakpoint = crate::dap::DapBreakpoint::from_location(&location);
        match kind {
            "if" | "condition" => breakpoint.condition = Some(value.trim().to_string()),
            "hit" | "hit-count" => breakpoint.hit_condition = Some(value.trim().to_string()),
            "log" | "logpoint" => breakpoint.log_message = Some(value.trim().to_string()),
            _ => {
                self.set_warning(format!("Unknown breakpoint kind: {kind}"));
                return;
            }
        }
        self.breakpoints.push(breakpoint);
        self.set_status(format!("Breakpoint count: {}", self.breakpoints.len()));
    }

    fn set_breakpoint_enabled(&mut self, index: usize, enabled: bool) {
        if index == 0 || index > self.breakpoints.len() {
            self.set_warning(format!("Breakpoint index out of range: {index}"));
            return;
        }
        self.breakpoints[index - 1].enabled = enabled;
        let state = if enabled { "enabled" } else { "disabled" };
        self.set_status(format!("Breakpoint {index} {state}"));
    }

    fn delete_breakpoint_by_index(&mut self, index: usize) {
        if index == 0 || index > self.breakpoints.len() {
            self.set_warning(format!("Breakpoint index out of range: {index}"));
            return;
        }
        self.breakpoints.remove(index - 1);
        if self.mode == SourceMode::Debug {
            self.results = self.debug_results();
            self.selected = self.selected.min(self.results.len().saturating_sub(1));
        }
        self.set_status(format!("Deleted breakpoint {index}"));
    }

    fn delete_selected(&mut self) -> Result<()> {
        match self.mode {
            SourceMode::Debug => self.delete_selected_debug(),
            SourceMode::Pinned => {
                self.unpin_selected();
                self.results = self.pinned_items.clone();
                self.selected = self.selected.min(self.results.len().saturating_sub(1));
                Ok(())
            }
            SourceMode::Trace => {
                self.set_warning(
                    "Trace deletion is available from CLI session tools; use trace source to open/bookmark",
                );
                Ok(())
            }
            _ => {
                self.set_warning("Delete only applies to Debug or Trace source");
                Ok(())
            }
        }
    }

    fn delete_selected_debug(&mut self) -> Result<()> {
        if self.selected < self.debug_profiles.len() {
            let name = self.debug_profiles[self.selected].name.clone();
            if crate::debugger::delete_profile(&self.root, &name)? {
                self.refresh_debug_profiles();
                self.results = self.debug_results();
                self.selected = self.selected.min(self.results.len().saturating_sub(1));
                self.set_status(format!("Deleted debug profile: {name}"));
            } else {
                self.set_warning(format!("Debug profile not found: {name}"));
            }
            return Ok(());
        }

        let breakpoint_index = self.selected.saturating_sub(self.debug_profiles.len());
        if breakpoint_index < self.breakpoints.len() {
            self.breakpoints.remove(breakpoint_index);
            self.results = self.debug_results();
            self.selected = self.selected.min(self.results.len().saturating_sub(1));
            self.set_status(format!("Deleted breakpoint {}", breakpoint_index + 1));
        } else {
            self.set_warning("No debug item selected");
        }
        Ok(())
    }

    fn add_trace_breakpoints(&mut self) {
        let mut added = 0;
        for item in &self.trace_items {
            self.breakpoints
                .push(crate::dap::DapBreakpoint::from_location(&item.location));
            added += 1;
        }
        self.status = format!("Added {added} trace breakpoint(s)");
    }

    fn show_definition(&mut self) -> Result<()> {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        self.push_current_navigation();
        self.queue_lsp("Definitions", LspCommand::Definition(location))
    }

    fn show_type_definition(&mut self) -> Result<()> {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        self.push_current_navigation();
        self.queue_lsp("Type Definitions", LspCommand::TypeDefinition(location))
    }

    fn show_implementation(&mut self) -> Result<()> {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        self.push_current_navigation();
        self.queue_lsp("Implementations", LspCommand::Implementation(location))
    }

    fn show_references(&mut self) -> Result<()> {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        self.push_current_navigation();
        self.queue_lsp("References", LspCommand::References(location))
    }

    fn show_diagnostics(&mut self) -> Result<()> {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        self.push_current_navigation();
        self.queue_lsp("Diagnostics", LspCommand::Diagnostics(location.path.clone()))
    }

    fn show_workspace_symbols(&mut self) -> Result<()> {
        if self.query.trim().is_empty() {
            self.status = "Workspace symbol query is empty".to_string();
            return Ok(());
        }

        self.push_current_navigation();
        self.queue_lsp("Workspace Symbols", LspCommand::WorkspaceSymbols(self.query.clone()))
    }

    fn show_document_symbols(&mut self) -> Result<()> {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        self.push_current_navigation();
        self.queue_lsp("Document Symbols", LspCommand::DocumentSymbols(location.path))
    }

    fn show_hover(&mut self) -> Result<()> {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        self.push_current_navigation();
        self.queue_lsp("Hover", LspCommand::Hover(location))
    }

    fn show_incoming_calls(&mut self) -> Result<()> {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        self.push_current_navigation();
        self.queue_lsp("Incoming Calls", LspCommand::IncomingCalls(location))
    }

    fn show_outgoing_calls(&mut self) -> Result<()> {
        let Some(location) = self.current_location() else {
            self.status = "No selected item".to_string();
            return Ok(());
        };

        self.push_current_navigation();
        self.queue_lsp("Outgoing Calls", LspCommand::OutgoingCalls(location))
    }

    fn show_debug(&mut self) {
        self.refresh_debug_profiles();
        self.mode = SourceMode::Debug;
        self.results = self.debug_results();
        self.selected = 0;
        self.status = self.debug_command_preview();
    }

    fn request_debug_run(&mut self) {
        if self.current_debug_profile().is_none() && self.breakpoints.is_empty() {
            self.status = "No debug profile or breakpoints to run".to_string();
            return;
        }
        self.pending_debug_run = true;
    }

    fn take_debug_run_request(&mut self) -> bool {
        let pending = self.pending_debug_run;
        self.pending_debug_run = false;
        pending
    }

    fn debug_session(&self) -> crate::debugger::DebugSession {
        if let Some(profile) = self.current_debug_profile() {
            return crate::debugger::DebugSession::from_profile(profile);
        }

        crate::debugger::DebugSession {
            debugger: crate::debugger::DebuggerKind::Gdb,
            binary: self.debug_binary.clone(),
            cwd: Some(self.root.clone()),
            env: Vec::new(),
            breakpoints: self
                .breakpoints
                .iter()
                .filter(|breakpoint| breakpoint.enabled)
                .map(breakpoint_location)
                .collect(),
            args: Vec::new(),
        }
    }

    fn debug_command_preview(&self) -> String {
        self.debug_session().command_preview()
    }

    fn debug_panel_text(&self) -> String {
        let mut lines = Vec::new();
        if self.current_debug_profile().is_none() && self.breakpoints.is_empty() {
            lines.push("No gdb/lldb profile or breakpoints".to_string());
        } else {
            lines.push(self.debug_command_preview());
        }

        lines.extend(dap_panel_lines(&self.dap_snapshot));
        lines.join("\n")
    }

    fn current_debug_profile(&self) -> Option<&crate::debugger::DebugProfile> {
        if self.mode != SourceMode::Debug {
            return None;
        }

        self.debug_profiles.get(self.selected)
    }

    fn dap_profile(&self) -> crate::dap::DapLaunchProfile {
        let session = self.debug_session();
        let name = self
            .current_debug_profile()
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| "tui-breakpoints".to_string());

        crate::dap::DapLaunchProfile {
            name,
            adapter: "mock".to_string(),
            request: "launch".to_string(),
            program: session.binary,
            process_id: None,
            cwd: session.cwd,
            args: session.args,
            env: session
                .env
                .into_iter()
                .map(|entry| crate::dap::DapEnvVar {
                    name: entry.name,
                    value: entry.value,
                })
                .collect(),
            breakpoints: if self.current_debug_profile().is_some() {
                session
                    .breakpoints
                    .iter()
                    .map(crate::dap::DapBreakpoint::from_location)
                    .collect()
            } else {
                self.breakpoints.clone()
            },
            stop_on_entry: false,
        }
    }

    fn queue_dap_mock_session(&mut self) -> Result<()> {
        let profile = self.dap_profile();
        let id = self.dap_worker.request(DapCommand::MockSession(profile))?;
        self.pending_dap = Some((id, "DAP mock session"));
        self.set_status("DAP mock session: pending...");
        Ok(())
    }

    fn queue_dap_start(&mut self) -> Result<()> {
        let profile = self.dap_profile();
        self.queue_dap_command("DAP start", DapCommand::StartMock(profile))
    }

    fn queue_dap_start_profile(&mut self, name: &str) -> Result<()> {
        let profile = crate::dap::load_profile(&self.root, name.trim())?;
        self.queue_dap_command("DAP start profile", DapCommand::StartMock(profile))
    }

    fn queue_dap_real(&mut self, rest: &str) -> Result<()> {
        let (profile, adapter_command) = self.dap_real_input(rest)?;
        let mut spec = dap_adapter_spec_from_command(adapter_command, &self.root)?;
        if spec.cwd.is_none() {
            spec.cwd = Some(self.root.clone());
        }
        self.queue_dap_command("DAP real start", DapCommand::StartReal { spec, profile })
    }

    fn dap_real_input(&self, rest: &str) -> Result<(crate::dap::DapLaunchProfile, String)> {
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(crate::errors::AppError::General(
                "Usage: dap real <adapter-command...> or dap real <profile> -- <adapter-command...>".to_string(),
            ));
        }

        if let Some((profile_name, adapter_command)) = rest.split_once(" -- ") {
            let mut profile = crate::dap::load_profile(&self.root, profile_name.trim())?;
            if profile.adapter == "mock" {
                profile.adapter = adapter_id_from_command(adapter_command);
            }
            return Ok((profile, adapter_command.trim().to_string()));
        }

        let mut profile = self.dap_profile();
        profile.adapter = adapter_id_from_command(rest);
        Ok((profile, rest.to_string()))
    }

    fn queue_dap_command(&mut self, label: &'static str, command: DapCommand) -> Result<()> {
        let id = self.dap_worker.request(command)?;
        self.pending_dap = Some((id, label));
        self.set_status(format!("{label}: pending..."));
        Ok(())
    }

    fn queue_dap_breakpoint_sync(&mut self) -> Result<()> {
        let profile = self.dap_profile();
        self.queue_dap_command("DAP break sync", DapCommand::SyncBreakpoints(profile))
    }

    fn show_dap_adapters(&mut self) {
        let adapters = crate::dap::discover_adapters()
            .into_iter()
            .take(6)
            .map(|adapter| {
                let state = if adapter.available { "ok" } else { "missing" };
                let capabilities = if adapter.capabilities.is_empty() {
                    "-".to_string()
                } else {
                    adapter.capabilities.join(",")
                };
                format!(
                    "{}={} ({state}; {})",
                    adapter.adapter,
                    adapter.command_line(),
                    capabilities
                )
            })
            .collect::<Vec<String>>();
        if adapters.is_empty() {
            self.set_warning("No DAP adapter candidates configured");
        } else {
            self.set_status(format!("DAP adapters: {}", adapters.join("; ")));
        }
    }

    fn show_dap_templates(&mut self) {
        let templates = crate::dap::adapter_templates()
            .into_iter()
            .map(|template| {
                let attach = if template.attach_fields.is_empty() {
                    "attach:-".to_string()
                } else {
                    format!("attach:{}", template.attach_fields.join(","))
                };
                format!(
                    "{}:{} via {} ({attach})",
                    template.adapter, template.request, template.command
                )
            })
            .collect::<Vec<String>>();
        if templates.is_empty() {
            self.set_warning("No DAP adapter templates configured");
        } else {
            self.set_status(format!("DAP templates: {}", templates.join("; ")));
        }
    }

    fn queue_dap_control(&mut self, command: &str) -> Result<()> {
        match command {
            "refresh" => self.queue_dap_command("DAP refresh", DapCommand::Refresh),
            "continue" | "cont" | "c" => self.queue_dap_command("DAP continue", DapCommand::Continue),
            "pause" => self.queue_dap_command("DAP pause", DapCommand::Pause),
            "next" | "n" => self.queue_dap_command("DAP next", DapCommand::Next),
            "step" | "step-in" | "in" => self.queue_dap_command("DAP step-in", DapCommand::StepIn),
            "step-out" | "out" => self.queue_dap_command("DAP step-out", DapCommand::StepOut),
            "restart" => self.queue_dap_command("DAP restart", DapCommand::Restart),
            "terminate" => self.queue_dap_command("DAP terminate", DapCommand::Terminate),
            "disconnect" => self.queue_dap_command("DAP disconnect", DapCommand::Disconnect),
            "stop" => self.queue_dap_command("DAP stop", DapCommand::Stop),
            "adapters" => {
                self.show_dap_adapters();
                Ok(())
            }
            "templates" => {
                self.show_dap_templates();
                Ok(())
            }
            other => {
                self.set_warning(format!("Unknown DAP command: {other}"));
                Ok(())
            }
        }
    }

    fn queue_watch_add(&mut self, expression: &str) -> Result<()> {
        let expression = expression.trim();
        if expression.is_empty() {
            self.set_warning("Watch expression is empty");
            return Ok(());
        }
        self.queue_dap_command("DAP watch add", DapCommand::AddWatch(expression.to_string()))
    }

    fn queue_watch_remove(&mut self, index: &str) -> Result<()> {
        let index = index
            .trim()
            .parse::<usize>()
            .map_err(|err| crate::errors::AppError::General(format!("Invalid watch index: {err}")))?;
        self.queue_dap_command("DAP watch remove", DapCommand::RemoveWatch(index))
    }

    fn queue_watch_clear(&mut self) -> Result<()> {
        self.queue_dap_command("DAP watch clear", DapCommand::ClearWatches)
    }

    fn queue_eval(&mut self, expression: &str) -> Result<()> {
        let expression = expression.trim();
        if expression.is_empty() {
            self.set_warning("Evaluate expression is empty");
            return Ok(());
        }
        self.queue_dap_command("DAP evaluate", DapCommand::Evaluate(expression.to_string()))
    }

    fn add_dap_stopped_breakpoint(&mut self) {
        let Some(location) = self.dap_stopped_location() else {
            self.set_warning("DAP session has no stopped location");
            return;
        };

        self.breakpoints
            .push(crate::dap::DapBreakpoint::from_location(&location));
        self.set_status(format!("Breakpoint count: {}", self.breakpoints.len()));
    }

    fn add_trace_breakpoint(&mut self) {
        let Some(location) = self.current_location().or_else(|| self.dap_stopped_location()) else {
            self.set_warning("No trace or DAP stopped location for breakpoint");
            return;
        };

        self.breakpoints
            .push(crate::dap::DapBreakpoint::from_location(&location));
        self.set_status(format!("Breakpoint count: {}", self.breakpoints.len()));
    }

    fn save_dap_profile_from_trace(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            self.set_warning("DAP profile name is empty");
            return Ok(());
        }

        let mut seen = std::collections::BTreeSet::new();
        let breakpoints = self
            .trace_items
            .iter()
            .filter_map(|item| {
                let line = item.location.line?;
                let key = (item.location.path.clone(), line, item.location.column);
                if seen.insert(key) {
                    Some(crate::dap::DapBreakpoint::from_location(&item.location))
                } else {
                    None
                }
            })
            .collect::<Vec<crate::dap::DapBreakpoint>>();
        if breakpoints.is_empty() {
            self.set_warning("Trace has no line locations for a DAP profile");
            return Ok(());
        }

        let mut profile = self.dap_profile();
        profile.name = name.to_string();
        profile.breakpoints = breakpoints;
        crate::dap::save_profile(&self.root, profile)?;
        self.set_status(format!("Saved DAP profile from trace: {name}"));
        Ok(())
    }

    fn dap_stopped_location(&self) -> Option<Location> {
        let stopped_location = self.dap_snapshot.stopped_location.as_ref()?;
        let path = if stopped_location.path.is_absolute() {
            stopped_location.path.clone()
        } else {
            self.root.join(&stopped_location.path)
        };
        Some(Location::new(
            path,
            Some(stopped_location.line),
            stopped_location.column,
        ))
    }

    fn poll_dap_worker(&mut self) {
        let Some(response) = self.dap_worker.try_recv_latest() else {
            return;
        };

        self.pending_dap = None;
        match response.result {
            Ok(snapshot) => {
                let request_count = snapshot.request_count;
                let trace_result = self.record_dap_stopped_location(&snapshot);
                self.dap_snapshot = snapshot;
                match trace_result {
                    Ok(true) => self.set_status(format!(
                        "{} completed: {request_count} request(s), debug stop traced",
                        response.label
                    )),
                    Ok(false) => {
                        self.set_status(format!("{} completed: {request_count} request(s)", response.label));
                    }
                    Err(err) => self.set_warning(format!("{} completed, trace failed: {err}", response.label)),
                }
            }
            Err(err) => {
                self.set_error(err.to_string());
            }
        }
    }

    fn record_dap_stopped_location(&self, snapshot: &crate::dap::DapSessionSnapshot) -> Result<bool> {
        let Some(stopped_location) = &snapshot.stopped_location else {
            return Ok(false);
        };

        let path = if stopped_location.path.is_absolute() {
            stopped_location.path.clone()
        } else {
            self.root.join(&stopped_location.path)
        };
        let location = Location::new(&path, Some(stopped_location.line), stopped_location.column);
        let reason = snapshot.stop_reason.as_deref().unwrap_or("stopped");
        let metadata = crate::trace::TraceMetadata {
            session: Some(format!("dap:{}", snapshot.profile)),
            parent: None,
            branch: Some(snapshot.adapter.clone()),
            tags: vec![
                "dap".to_string(),
                "debug".to_string(),
                "stop".to_string(),
                reason.to_string(),
            ],
            note: Some(dap_trace_note(snapshot)),
            status: Some("observed".to_string()),
            priority: None,
        };
        crate::trace::record_location_for_workspace_with_metadata(
            &self.root,
            &location,
            &format!("DAP stopped: {reason}"),
            "debug-stop",
            metadata,
        )?;
        Ok(true)
    }

    fn jump_to_dap_stopped_location(&mut self) {
        let snapshot = &self.dap_snapshot;
        let Some(stopped_location) = &snapshot.stopped_location else {
            self.set_warning("DAP session has no stopped location");
            return;
        };

        let path = if stopped_location.path.is_absolute() {
            stopped_location.path.clone()
        } else {
            self.root.join(&stopped_location.path)
        };
        let location = Location::new(&path, Some(stopped_location.line), stopped_location.column);
        let display = format!("{}:{}:{}", path.display(), stopped_location.line, snapshot.status);
        let item = CodeItem::from_parts(
            CodeItemKind::TextMatch,
            "DAP stopped",
            snapshot.status.clone(),
            location,
            display,
        );
        self.results = vec![item.clone()];
        self.selected = 0;
        self.preview_scroll = 0;
        self.mode = SourceMode::Debug;
        self.push_navigation(item);
        self.set_status("Jumped to DAP stopped location");
    }

    fn preview_window_for_current(&self, height: u16) -> PreviewWindow {
        self.preview_location()
            .map(|location| {
                self.preview_cache
                    .borrow_mut()
                    .window_with_scroll(&location, height, self.preview_scroll)
            })
            .unwrap_or_else(|| PreviewWindow::message("No selection"))
    }

    fn preview_location(&self) -> Option<Location> {
        self.locked_preview.clone().or_else(|| self.current_location())
    }

    fn preview_title(&self) -> String {
        let lock = if self.locked_preview.is_some() { " locked" } else { "" };
        let scroll = if self.preview_scroll == 0 {
            String::new()
        } else {
            format!(" scroll={}", self.preview_scroll)
        };
        format!("Preview{lock}{scroll}")
    }

    fn scroll_preview(&mut self, delta: isize) {
        self.preview_scroll = (self.preview_scroll + delta).max(-2000);
        self.set_status(format!("Preview scroll: {}", self.preview_scroll));
    }

    fn toggle_preview_lock(&mut self) {
        if self.locked_preview.is_some() {
            self.locked_preview = None;
            self.preview_scroll = 0;
            self.set_status("Preview unlocked");
            return;
        }

        let Some(location) = self.current_location() else {
            self.set_warning("No selected item to lock preview");
            return;
        };
        self.locked_preview = Some(location);
        self.preview_scroll = 0;
        self.set_status("Preview locked");
    }

    fn push_current_navigation(&mut self) {
        if let Some(item) = self.current_item().cloned() {
            self.push_navigation(item);
        }
    }

    fn push_navigation(&mut self, item: CodeItem) {
        if let Some(index) = self.navigation_index {
            self.navigation.truncate(index + 1);
        }

        if let Some(last) = self.navigation.last() {
            if same_code_item(last, &item) {
                self.navigation_index = Some(self.navigation.len() - 1);
                return;
            }
        }

        self.navigation.push(item);
        self.navigation_index = Some(self.navigation.len() - 1);
    }

    fn jump_navigation(&mut self, delta: isize) {
        let Some(index) = self.navigation_index else {
            self.status = "Navigation stack is empty".to_string();
            return;
        };
        let next = index as isize + delta;
        if next < 0 || next >= self.navigation.len() as isize {
            self.status = "No more navigation entries".to_string();
            return;
        }

        self.navigation_index = Some(next as usize);
        if let Some(item) = self.navigation.get(next as usize).cloned() {
            self.results = vec![item];
            self.selected = 0;
            self.preview_scroll = 0;
            self.status = format!("Navigation {}/{}", next + 1, self.navigation.len());
        }
    }

    fn cycle_operation(&mut self, delta: isize) -> Result<()> {
        let next = tracking_mode_after(self.mode, delta);
        self.run_tracking_operation(next)
    }

    fn run_tracking_operation(&mut self, mode: SourceMode) -> Result<()> {
        match mode {
            SourceMode::Search => self.set_source(SourceMode::Search),
            SourceMode::References => self.show_references(),
            SourceMode::Symbols => self.set_source(SourceMode::Symbols),
            SourceMode::Diagnostics => self.show_diagnostics(),
            SourceMode::Debug => {
                self.show_debug();
                Ok(())
            }
            SourceMode::Files | SourceMode::Trace | SourceMode::Pinned => self.set_source(mode),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.help_visible {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') | KeyCode::Char('q') => {
                    self.help_visible = false;
                    self.set_status("Help closed");
                    return Ok(());
                }
                _ => {}
            }
        }

        if matches!(action_for_key(key, &self.config.tui.keymap), Some(AppAction::Quit)) {
            return self.apply_action(AppAction::Quit);
        }

        if self.input_active {
            return self.handle_input_key(key);
        }
        if self.command_active {
            return self.handle_command_key(key);
        }

        if self.pending_g {
            self.pending_g = false;
            if let Some(action) = goto_action_for_key(key) {
                return self.apply_action(action);
            }
        }

        if let Some(action) = action_for_key(key, &self.config.tui.keymap) {
            self.apply_action(action)?;
        }

        Ok(())
    }

    fn apply_action(&mut self, action: AppAction) -> Result<()> {
        match action {
            AppAction::Quit => self.should_quit = true,
            AppAction::ActivateQuery => {
                self.help_visible = false;
                self.input_active = true;
            }
            AppAction::ActivateCommandPalette => self.activate_command_palette(),
            AppAction::BeginGoto => self.pending_g = true,
            AppAction::Refresh => self.refresh()?,
            AppAction::PinSelected => self.pin_selected(),
            AppAction::UnpinSelected => self.unpin_selected(),
            AppAction::LoadPinned => self.load_pinned_results(),
            AppAction::AddTrace => self.add_trace()?,
            AppAction::AddBreakpoint => self.add_breakpoint(),
            AppAction::AddTraceBreakpoints => self.add_trace_breakpoints(),
            AppAction::DeleteSelected => self.delete_selected()?,
            AppAction::ShowDebug => self.show_debug(),
            AppAction::RequestDebugRun => self.request_debug_run(),
            AppAction::WorkspaceSymbols => self.show_workspace_symbols()?,
            AppAction::ShowHelp => {
                self.help_visible = !self.help_visible;
                self.set_status(if self.help_visible {
                    "Help opened"
                } else {
                    "Help closed"
                });
            }
            AppAction::JumpNavigation(delta) => self.jump_navigation(delta),
            AppAction::CycleOperation(delta) => self.cycle_operation(delta)?,
            AppAction::IncomingCalls => self.show_incoming_calls()?,
            AppAction::OutgoingCalls => self.show_outgoing_calls()?,
            AppAction::Diagnostics => self.show_diagnostics()?,
            AppAction::Hover => self.show_hover()?,
            AppAction::Implementation => self.show_implementation()?,
            AppAction::DocumentSymbols => self.show_document_symbols()?,
            AppAction::TypeDefinition => self.show_type_definition()?,
            AppAction::Open => self.open_selected()?,
            AppAction::NextSource => self.next_source()?,
            AppAction::PreviousSource => self.prev_source()?,
            AppAction::MoveSelection(delta) => self.move_selection(delta),
            AppAction::ScrollPreview(delta) => self.scroll_preview(delta),
            AppAction::TogglePreviewLock => self.toggle_preview_lock(),
            AppAction::DapContinue => self.queue_dap_control("continue")?,
            AppAction::DapPause => self.queue_dap_control("pause")?,
            AppAction::DapNext => self.queue_dap_control("next")?,
            AppAction::DapStepIn => self.queue_dap_control("step-in")?,
            AppAction::DapStepOut => self.queue_dap_control("step-out")?,
            AppAction::DapStop => self.queue_dap_control("stop")?,
            AppAction::Definition => self.show_definition()?,
            AppAction::References => self.show_references()?,
        }

        Ok(())
    }

    fn activate_command_palette(&mut self) {
        self.help_visible = false;
        self.command_active = true;
        self.command.clear();
        self.command_cursor = 0;
        self.status = command_help_text();
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.command_active = false;
                self.status = "Command cancelled".to_string();
            }
            KeyCode::Enter => {
                self.command_active = false;
                let command = self.command.trim().to_string();
                if !command.is_empty() {
                    self.command_history.push(command.clone());
                    self.command_history_index = None;
                }
                self.command.clear();
                self.command_cursor = 0;
                self.execute_palette_command(&command)?;
            }
            KeyCode::Tab => {
                self.complete_command();
            }
            KeyCode::Up => {
                self.recall_command_history(1);
            }
            KeyCode::Down => {
                self.recall_command_history(-1);
            }
            KeyCode::Backspace => {
                let (command, cursor) = delete_char_before_cursor(&self.command, self.command_cursor);
                self.command = command;
                self.command_cursor = cursor;
            }
            KeyCode::Delete => {
                let (command, cursor) = delete_char_at_cursor(&self.command, self.command_cursor);
                self.command = command;
                self.command_cursor = cursor;
            }
            KeyCode::Left => {
                self.command_cursor = self.command_cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                self.command_cursor = (self.command_cursor + 1).min(self.command.chars().count());
            }
            KeyCode::Home => {
                self.command_cursor = 0;
            }
            KeyCode::End => {
                self.command_cursor = self.command.chars().count();
            }
            KeyCode::Char(ch) => {
                let (command, cursor) = insert_char_at(&self.command, self.command_cursor, ch);
                self.command = command;
                self.command_cursor = cursor;
                self.status = command_hint_text(&self.command);
                self.status_level = StatusLevel::Info;
            }
            _ => {}
        }
        Ok(())
    }

    fn execute_palette_command(&mut self, command: &str) -> Result<()> {
        if self.execute_structured_palette_command(command)? {
            return Ok(());
        }

        let Some(action) = palette_command_to_action(command) else {
            self.set_warning(format!("Unknown command: {command}"));
            return Ok(());
        };

        self.apply_action(action)
    }

    fn execute_structured_palette_command(&mut self, command: &str) -> Result<bool> {
        let command = command.trim();
        if command.is_empty() {
            self.set_warning("Command is empty");
            return Ok(true);
        }

        if let Some(rest) = command
            .strip_prefix("source ")
            .or_else(|| command.strip_prefix("mode "))
        {
            let mode = parse_mode(Some(rest.trim()))?;
            self.set_source(mode)?;
            return Ok(true);
        }

        if let Some(rest) = command.strip_prefix("query ") {
            self.query = rest.to_string();
            self.query_cursor = self.query.chars().count();
            self.refresh()?;
            return Ok(true);
        }

        if let Some(rest) = command.strip_prefix("preview ") {
            match rest.trim() {
                "lock" | "toggle" => self.toggle_preview_lock(),
                "up" => self.scroll_preview(-5),
                "down" => self.scroll_preview(5),
                "reset" => {
                    self.preview_scroll = 0;
                    self.set_status("Preview scroll reset");
                }
                other => self.set_warning(format!("Unknown preview command: {other}")),
            }
            return Ok(true);
        }

        if let Some(rest) = command.strip_prefix("cycle") {
            match rest.trim() {
                "" | "next" | "forward" => self.cycle_operation(1)?,
                "prev" | "previous" | "back" => self.cycle_operation(-1)?,
                other => self.set_warning(format!("Unknown cycle command: {other}")),
            }
            return Ok(true);
        }

        if let Some(rest) = command.strip_prefix("jump ") {
            match rest.trim() {
                "back" | "prev" | "previous" => self.jump_navigation(-1),
                "forward" | "next" => self.jump_navigation(1),
                other => self.set_warning(format!("Unknown jump command: {other}")),
            }
            return Ok(true);
        }

        if let Some(rest) = command.strip_prefix("dap ") {
            let rest = rest.trim();
            if let Some(adapter_command) = rest.strip_prefix("real ").or_else(|| rest.strip_prefix("start-real ")) {
                self.queue_dap_real(adapter_command)?;
                return Ok(true);
            }
            if let Some(profile_name) = rest.strip_prefix("start ") {
                self.queue_dap_start_profile(profile_name)?;
                return Ok(true);
            }
            if let Some(thread_id) = rest.strip_prefix("thread ") {
                let thread_id = thread_id
                    .trim()
                    .parse::<u64>()
                    .map_err(|err| crate::errors::AppError::General(format!("Invalid DAP thread id: {err}")))?;
                self.queue_dap_command("DAP thread", DapCommand::SelectThread(thread_id))?;
                return Ok(true);
            }
            if let Some(frame_index) = rest.strip_prefix("frame ") {
                self.queue_dap_command("DAP frame", DapCommand::SelectFrame(parse_index(frame_index)?))?;
                return Ok(true);
            }
            match rest {
                "smoke" | "mock" | "session" => self.queue_dap_mock_session()?,
                "start" | "launch" => self.queue_dap_start()?,
                "sync" | "break-sync" | "breakpoints" => self.queue_dap_breakpoint_sync()?,
                "break" | "breakpoint" => self.add_dap_stopped_breakpoint(),
                "jump" | "open" => self.jump_to_dap_stopped_location(),
                "adapters" | "adapter" => self.show_dap_adapters(),
                "templates" | "template" => self.show_dap_templates(),
                "refresh" | "continue" | "cont" | "c" | "pause" | "next" | "n" | "step" | "step-in" | "in"
                | "step-out" | "out" | "restart" | "terminate" | "disconnect" | "stop" => {
                    self.queue_dap_control(rest)?
                }
                other => self.set_warning(format!("Unknown DAP command: {other}")),
            }
            return Ok(true);
        }

        if let Some(rest) = command
            .strip_prefix("var ")
            .or_else(|| command.strip_prefix("vars "))
            .or_else(|| command.strip_prefix("variables "))
        {
            let rest = rest.trim();
            if let Some(reference) = rest.strip_prefix("expand ") {
                let reference = reference
                    .trim()
                    .parse::<u64>()
                    .map_err(|err| crate::errors::AppError::General(format!("Invalid variables reference: {err}")))?;
                self.queue_dap_command("DAP variable expand", DapCommand::ExpandVariables(reference))?;
            } else if let Some(page) = rest.strip_prefix("page ") {
                let parts = page.split_whitespace().collect::<Vec<&str>>();
                if parts.len() != 2 {
                    self.set_warning("Usage: var page <start> <count>");
                } else {
                    let start = parts[0].parse::<usize>().map_err(|err| {
                        crate::errors::AppError::General(format!("Invalid variable page start: {err}"))
                    })?;
                    let count = parts[1].parse::<usize>().map_err(|err| {
                        crate::errors::AppError::General(format!("Invalid variable page count: {err}"))
                    })?;
                    self.queue_dap_command("DAP variable page", DapCommand::VariablesPage { start, count })?;
                }
            } else {
                self.set_warning(format!("Unknown variable command: {rest}"));
            }
            return Ok(true);
        }

        if let Some(rest) = command.strip_prefix("watch ") {
            if let Some(expression) = rest.trim().strip_prefix("add ") {
                self.queue_watch_add(expression)?;
            } else if let Some(index) = rest
                .trim()
                .strip_prefix("del ")
                .or_else(|| rest.trim().strip_prefix("delete "))
            {
                self.queue_watch_remove(index)?;
            } else if matches!(rest.trim(), "clear" | "reset") {
                self.queue_watch_clear()?;
            } else if matches!(rest.trim(), "refresh" | "update") {
                self.queue_dap_command("DAP watch refresh", DapCommand::RefreshWatches)?;
            } else {
                self.queue_watch_add(rest.trim())?;
            }
            return Ok(true);
        }

        if let Some(expression) = command.strip_prefix("eval ") {
            self.queue_eval(expression)?;
            return Ok(true);
        }

        if let Some(rest) = command.strip_prefix("break ") {
            if let Some(value) = rest.trim().strip_prefix("if ") {
                self.add_advanced_breakpoint("if", value);
            } else if let Some(value) = rest.trim().strip_prefix("hit ") {
                self.add_advanced_breakpoint("hit", value);
            } else if let Some(value) = rest.trim().strip_prefix("log ") {
                self.add_advanced_breakpoint("log", value);
            } else if let Some(value) = rest.trim().strip_prefix("enable ") {
                self.set_breakpoint_enabled(parse_index(value)?, true);
            } else if let Some(value) = rest.trim().strip_prefix("disable ") {
                self.set_breakpoint_enabled(parse_index(value)?, false);
            } else if let Some(value) = rest
                .trim()
                .strip_prefix("delete ")
                .or_else(|| rest.trim().strip_prefix("del "))
            {
                self.delete_breakpoint_by_index(parse_index(value)?);
            } else if matches!(rest.trim(), "sync" | "dap-sync") {
                self.queue_dap_breakpoint_sync()?;
            } else {
                self.add_breakpoint();
            }
            return Ok(true);
        }

        if let Some(rest) = command.strip_prefix("trace ") {
            if matches!(rest.trim(), "break" | "breakpoint") {
                self.add_trace_breakpoint();
            } else if let Some(name) = rest.trim().strip_prefix("dap-profile ") {
                self.save_dap_profile_from_trace(name)?;
            } else {
                self.set_warning(format!("Unknown trace command: {}", rest.trim()));
            }
            return Ok(true);
        }

        match command {
            "dap-smoke" | "dap-mock" => {
                self.queue_dap_mock_session()?;
                Ok(true)
            }
            "dap-start" => {
                self.queue_dap_start()?;
                Ok(true)
            }
            "dap-sync" => {
                self.queue_dap_breakpoint_sync()?;
                Ok(true)
            }
            "dap-next" => {
                self.queue_dap_control("next")?;
                Ok(true)
            }
            "dap-continue" => {
                self.queue_dap_control("continue")?;
                Ok(true)
            }
            "dap-pause" => {
                self.queue_dap_control("pause")?;
                Ok(true)
            }
            "dap-restart" => {
                self.queue_dap_control("restart")?;
                Ok(true)
            }
            "pin" => {
                self.pin_selected();
                Ok(true)
            }
            "unpin" => {
                self.unpin_selected();
                Ok(true)
            }
            "pins" | "pinned" => {
                self.load_pinned_results();
                Ok(true)
            }
            "back" | "jump-back" => {
                self.jump_navigation(-1);
                Ok(true)
            }
            "forward" | "jump-forward" => {
                self.jump_navigation(1);
                Ok(true)
            }
            "files" | "file" | "symbols" | "symbol" | "trace" | "debug" | "refs" | "references" | "diag"
            | "diagnostics" | "search" | "text" => {
                let mode = parse_mode(Some(command))?;
                self.set_source(mode)?;
                Ok(true)
            }
            "delete" | "del" | "remove" => {
                self.delete_selected()?;
                Ok(true)
            }
            "preview-lock" => {
                self.toggle_preview_lock();
                Ok(true)
            }
            "preview-up" => {
                self.scroll_preview(-5);
                Ok(true)
            }
            "preview-down" => {
                self.scroll_preview(5);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn complete_command(&mut self) {
        let matches = palette_command_matches(&self.command);
        match matches.as_slice() {
            [] => self.set_warning(format!("No command match for '{}'", self.command)),
            [only] => {
                self.command = (*only).to_string();
                self.command_cursor = self.command.chars().count();
                self.set_status(format!("Completed command: {}", self.command));
            }
            values => {
                self.command = values[0].to_string();
                self.command_cursor = self.command.chars().count();
                self.set_status(format!("Matches: {}", values.join(", ")));
            }
        }
    }

    fn command_matches(&self) -> Vec<&'static str> {
        palette_command_matches(&self.command)
    }

    fn is_pinned(&self, item: &CodeItem) -> bool {
        self.pinned_items.iter().any(|pinned| same_code_item(pinned, item))
    }

    fn recall_command_history(&mut self, delta: isize) {
        let Some(next) = history_index_after(self.command_history_index, self.command_history.len(), delta) else {
            return;
        };
        self.command_history_index = Some(next);
        self.command = self.command_history[next].clone();
        self.command_cursor = self.command.chars().count();
        self.set_status(format!("Command history {}/{}", next + 1, self.command_history.len()));
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.input_active = false,
            KeyCode::Enter => {
                self.input_active = false;
                self.query_history_index = None;
                self.refresh()?;
            }
            KeyCode::Backspace => {
                self.delete_query_char_before_cursor();
            }
            KeyCode::Delete => {
                self.delete_query_char_at_cursor();
            }
            KeyCode::Left => {
                self.query_cursor = self.query_cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                self.query_cursor = (self.query_cursor + 1).min(self.query.chars().count());
            }
            KeyCode::Home => {
                self.query_cursor = 0;
            }
            KeyCode::End => {
                self.query_cursor = self.query.chars().count();
            }
            KeyCode::Up => {
                self.recall_query_history(1);
            }
            KeyCode::Down => {
                self.recall_query_history(-1);
            }
            KeyCode::Char(ch) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && ch == 'u' {
                    self.query.clear();
                    self.query_cursor = 0;
                } else if key.modifiers.contains(KeyModifiers::CONTROL) && ch == 'w' {
                    self.delete_query_word_before_cursor();
                } else {
                    self.insert_query_char(ch);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn insert_query_char(&mut self, ch: char) {
        let (query, cursor) = insert_char_at(&self.query, self.query_cursor, ch);
        self.query = query;
        self.query_cursor = cursor;
    }

    fn delete_query_char_before_cursor(&mut self) {
        let (query, cursor) = delete_char_before_cursor(&self.query, self.query_cursor);
        self.query = query;
        self.query_cursor = cursor;
    }

    fn delete_query_char_at_cursor(&mut self) {
        let (query, cursor) = delete_char_at_cursor(&self.query, self.query_cursor);
        self.query = query;
        self.query_cursor = cursor;
    }

    fn delete_query_word_before_cursor(&mut self) {
        let (query, cursor) = delete_word_before_cursor(&self.query, self.query_cursor);
        self.query = query;
        self.query_cursor = cursor;
    }

    fn recall_query_history(&mut self, delta: isize) {
        let Some(next) = history_index_after(self.query_history_index, self.query_history.len(), delta) else {
            return;
        };
        self.query_history_index = Some(next);
        self.query = self.query_history[next].clone();
        self.query_cursor = self.query.chars().count();
    }
}

pub fn run(
    config: Config,
    directory: Option<String>,
    mode: Option<String>,
    query: Option<String>,
    debug_binary: Option<String>,
) -> Result<()> {
    let mode = mode.as_deref().map(|value| parse_mode(Some(value))).transpose()?;
    let mut app = AppState::new(config, directory, mode, query, debug_binary)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut AppState) -> Result<()> {
    while !app.should_quit {
        app.poll_source_worker();
        app.poll_lsp_worker();
        app.poll_dap_worker();
        terminal.draw(|frame| render::render(frame, app))?;
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if let Err(err) = app.handle_key(key) {
                    app.set_error(err.to_string());
                }
            }
        }
        if app.take_debug_run_request() {
            run_debug_session_in_terminal(terminal, app)?;
        }
    }
    app.save_persistent_state()?;
    Ok(())
}

fn run_debug_session_in_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let result = app.debug_session().run();

    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    app.status = match result {
        Ok(()) => "Debugger exited successfully".to_string(),
        Err(err) => format!("Debugger failed: {err}"),
    };
    Ok(())
}

fn same_code_item(left: &CodeItem, right: &CodeItem) -> bool {
    left.display_text() == right.display_text() && left.location == right.location
}

fn saved_items_to_code_items(items: Vec<TuiSavedItem>) -> Vec<CodeItem> {
    items.into_iter().filter_map(TuiSavedItem::into_code_item).collect()
}

fn default_dap_snapshot() -> crate::dap::DapSessionSnapshot {
    crate::dap::DapSessionSnapshot {
        adapter: "mock".to_string(),
        state: crate::dap::DapSessionState::Idle,
        status: "DAP mock: idle".to_string(),
        profile: "none".to_string(),
        selected_thread_id: None,
        selected_frame_id: None,
        variables_reference: None,
        variables_start: None,
        variables_count: None,
        request_count: 0,
        response_count: 0,
        commands: Vec::new(),
        events: Vec::new(),
        threads: Vec::new(),
        stack: Vec::new(),
        scopes: Vec::new(),
        variables: Vec::new(),
        breakpoints: Vec::new(),
        capabilities: Vec::new(),
        thread_items: Vec::new(),
        frame_items: Vec::new(),
        scope_items: Vec::new(),
        variable_items: Vec::new(),
        watches: Vec::new(),
        last_evaluation: None,
        stop_reason: None,
        last_event: None,
        last_request: None,
        last_error: None,
        error: None,
        stopped_location: None,
    }
}

fn dap_panel_lines(snapshot: &crate::dap::DapSessionSnapshot) -> Vec<String> {
    let mut lines = vec![
        format!(
            "{} [{}:{}:{}]",
            snapshot.status,
            snapshot.adapter,
            snapshot.profile,
            snapshot.state.as_str()
        ),
        format!(
            "DAP requests/responses: {}/{}",
            snapshot.request_count, snapshot.response_count
        ),
    ];
    let mut selection = Vec::new();
    if let Some(thread_id) = snapshot.selected_thread_id {
        selection.push(format!("thread={thread_id}"));
    }
    if let Some(frame_id) = snapshot.selected_frame_id {
        selection.push(format!("frame={frame_id}"));
    }
    if let Some(reference) = snapshot.variables_reference {
        let page = match (snapshot.variables_start, snapshot.variables_count) {
            (Some(start), Some(count)) => format!("vars_ref={reference} page={start}..{}", start + count),
            (Some(start), None) => format!("vars_ref={reference} start={start}"),
            (None, Some(count)) => format!("vars_ref={reference} count={count}"),
            (None, None) => format!("vars_ref={reference}"),
        };
        selection.push(page);
    }
    if !selection.is_empty() {
        lines.push(format!("Selected: {}", selection.join(" ")));
    }
    if let Some(request) = &snapshot.last_request {
        lines.push(format!("Last request: {request}"));
    }
    if !snapshot.threads.is_empty() {
        lines.push(format!("Threads: {}", limited_join(&snapshot.threads, 2)));
    }
    if !snapshot.stack.is_empty() {
        lines.push(format!("Stack: {}", limited_join(&snapshot.stack, 2)));
    }
    if !snapshot.scopes.is_empty() {
        lines.push(format!("Scopes: {}", limited_join(&snapshot.scopes, 2)));
    }
    if !snapshot.variables.is_empty() {
        lines.push(format!("Variables: {}", limited_join(&snapshot.variables, 3)));
    }
    if !snapshot.watches.is_empty() {
        lines.push(format!("Watches: {}", limited_join(&snapshot.watches, 3)));
    }
    if !snapshot.breakpoints.is_empty() {
        lines.push(format!("Breakpoints: {}", limited_join(&snapshot.breakpoints, 3)));
    }
    if let Some(evaluation) = &snapshot.last_evaluation {
        lines.push(format!("Eval: {evaluation}"));
    }
    if let Some(reason) = &snapshot.stop_reason {
        lines.push(format!("Stop reason: {reason}"));
    }
    if let Some(error) = &snapshot.error {
        lines.push(format!("Error: {error}"));
    }
    if let Some(error) = &snapshot.last_error {
        lines.push(format!("Last error: {error}"));
    }
    if let Some(location) = &snapshot.stopped_location {
        let column = location.column.map(|column| format!(":{column}")).unwrap_or_default();
        lines.push(format!(
            "Stopped: {}:{}{}",
            location.path.display(),
            location.line,
            column
        ));
    }
    if !snapshot.commands.is_empty() {
        lines.push(format!("Commands: {}", limited_join(&snapshot.commands, 8)));
    }
    if !snapshot.events.is_empty() {
        lines.push(format!("Events: {}", limited_join(&snapshot.events, 4)));
    }
    lines
}

fn breakpoint_location(breakpoint: &crate::dap::DapBreakpoint) -> Location {
    Location::new(breakpoint.path.clone(), Some(breakpoint.line), breakpoint.column)
}

fn breakpoint_label(index: usize, breakpoint: &crate::dap::DapBreakpoint) -> String {
    let mut parts = vec![format!("breakpoint {}", index + 1)];
    if !breakpoint.enabled {
        parts.push("disabled".to_string());
    }
    if let Some(condition) = &breakpoint.condition {
        parts.push(format!("if {condition}"));
    }
    if let Some(hit_condition) = &breakpoint.hit_condition {
        parts.push(format!("hit {hit_condition}"));
    }
    if let Some(log_message) = &breakpoint.log_message {
        parts.push(format!("log {log_message}"));
    }
    parts.join(" ")
}

fn parse_index(value: &str) -> Result<usize> {
    value
        .trim()
        .parse::<usize>()
        .map_err(|err| crate::errors::AppError::General(format!("Invalid index: {err}")))
}

fn dap_adapter_spec_from_command(command: String, root: &std::path::Path) -> Result<crate::dap::DapAdapterProcessSpec> {
    let mut parts = command
        .split_whitespace()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<&str>>();
    if parts.is_empty() {
        return Err(crate::errors::AppError::General(
            "DAP adapter command is empty".to_string(),
        ));
    }

    let program = parts.remove(0);
    Ok(crate::dap::DapAdapterProcessSpec {
        command: PathBuf::from(program),
        args: parts.into_iter().map(ToOwned::to_owned).collect(),
        cwd: Some(root.to_path_buf()),
        env: Vec::new(),
    })
}

fn adapter_id_from_command(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .and_then(|program| {
            PathBuf::from(program)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "cppdbg".to_string())
}

fn limited_join(values: &[String], max: usize) -> String {
    let mut parts = values.iter().take(max).cloned().collect::<Vec<String>>();
    if values.len() > max {
        parts.push(format!("+{}", values.len() - max));
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

fn dap_trace_note(snapshot: &crate::dap::DapSessionSnapshot) -> String {
    let mut parts = Vec::new();
    parts.push(format!("status={}", snapshot.status));
    parts.push(format!("adapter={}", snapshot.adapter));
    parts.push(format!("profile={}", snapshot.profile));
    if let Some(reason) = &snapshot.stop_reason {
        parts.push(format!("reason={reason}"));
    }
    if let Some(frame) = snapshot.stack.first() {
        parts.push(format!("top_frame={frame}"));
    }
    if !snapshot.variables.is_empty() {
        parts.push(format!("variables={}", limited_join(&snapshot.variables, 4)));
    }
    if !snapshot.watches.is_empty() {
        parts.push(format!("watches={}", limited_join(&snapshot.watches, 4)));
    }
    if !snapshot.breakpoints.is_empty() {
        parts.push(format!("breakpoints={}", limited_join(&snapshot.breakpoints, 4)));
    }
    parts.join("; ")
}

fn query_with_cursor(query: &str, cursor: usize, active: bool) -> String {
    if !active {
        return query.to_string();
    }

    let mut chars = query.chars().collect::<Vec<char>>();
    let index = cursor.min(chars.len());
    chars.insert(index, '|');
    chars.into_iter().collect()
}

fn selection_after(selected: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }

    (selected as isize + delta).rem_euclid(len as isize) as usize
}

fn insert_char_at(query: &str, cursor: usize, ch: char) -> (String, usize) {
    let mut chars = query.chars().collect::<Vec<char>>();
    let index = cursor.min(chars.len());
    chars.insert(index, ch);
    (chars.into_iter().collect(), index + 1)
}

fn delete_char_before_cursor(query: &str, cursor: usize) -> (String, usize) {
    let mut chars = query.chars().collect::<Vec<char>>();
    let index = cursor.min(chars.len());
    if index == 0 {
        return (query.to_string(), 0);
    }

    chars.remove(index - 1);
    (chars.into_iter().collect(), index - 1)
}

fn delete_char_at_cursor(query: &str, cursor: usize) -> (String, usize) {
    let mut chars = query.chars().collect::<Vec<char>>();
    let index = cursor.min(chars.len());
    if index < chars.len() {
        chars.remove(index);
    }

    (chars.into_iter().collect(), index)
}

fn delete_word_before_cursor(query: &str, cursor: usize) -> (String, usize) {
    let mut chars = query.chars().collect::<Vec<char>>();
    let cursor = cursor.min(chars.len());
    let mut index = cursor;
    while index > 0 && chars[index - 1].is_whitespace() {
        index -= 1;
    }
    while index > 0 && !chars[index - 1].is_whitespace() {
        index -= 1;
    }
    chars.drain(index..cursor);
    (chars.into_iter().collect(), index)
}

fn history_index_after(current: Option<usize>, len: usize, delta: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }

    match current {
        Some(index) => Some((index as isize + delta).rem_euclid(len as isize) as usize),
        None if delta >= 0 => Some(0),
        None => Some(len - 1),
    }
}

fn palette_command_to_action(command: &str) -> Option<AppAction> {
    match command.trim() {
        "q" | "quit" => Some(AppAction::Quit),
        "refresh" => Some(AppAction::Refresh),
        "open" => Some(AppAction::Open),
        "pin" => Some(AppAction::PinSelected),
        "unpin" => Some(AppAction::UnpinSelected),
        "pins" | "pinned" => Some(AppAction::LoadPinned),
        "back" | "jump-back" => Some(AppAction::JumpNavigation(-1)),
        "forward" | "jump-forward" => Some(AppAction::JumpNavigation(1)),
        "cycle" | "cycle-next" => Some(AppAction::CycleOperation(1)),
        "cycle-prev" | "cycle-back" => Some(AppAction::CycleOperation(-1)),
        "def" | "definition" => Some(AppAction::Definition),
        "refs" | "references" => Some(AppAction::References),
        "type" | "type-def" => Some(AppAction::TypeDefinition),
        "impl" | "implementation" => Some(AppAction::Implementation),
        "symbols" | "doc-symbols" => Some(AppAction::DocumentSymbols),
        "diag" | "diagnostics" => Some(AppAction::Diagnostics),
        "incoming" => Some(AppAction::IncomingCalls),
        "outgoing" => Some(AppAction::OutgoingCalls),
        "hover" => Some(AppAction::Hover),
        "trace" | "bookmark" => Some(AppAction::AddTrace),
        "break" | "breakpoint" => Some(AppAction::AddBreakpoint),
        "debug" => Some(AppAction::ShowDebug),
        "run" => Some(AppAction::RequestDebugRun),
        "help" => Some(AppAction::ShowHelp),
        _ => None,
    }
}

fn palette_command_names() -> &'static [&'static str] {
    &[
        "def",
        "refs",
        "type",
        "impl",
        "symbols",
        "diag",
        "incoming",
        "outgoing",
        "hover",
        "pin",
        "unpin",
        "pins",
        "back",
        "forward",
        "cycle",
        "cycle back",
        "trace",
        "break",
        "debug",
        "dap smoke",
        "dap start",
        "dap start ",
        "dap real ",
        "dap thread ",
        "dap frame ",
        "dap sync",
        "dap next",
        "dap continue",
        "dap pause",
        "dap refresh",
        "dap step-in",
        "dap step-out",
        "dap restart",
        "dap terminate",
        "dap disconnect",
        "dap adapters",
        "dap templates",
        "dap stop",
        "dap jump",
        "dap break",
        "dap-smoke",
        "dap-sync",
        "watch add ",
        "watch del ",
        "watch clear",
        "watch refresh",
        "var expand ",
        "var page ",
        "eval ",
        "break if ",
        "break hit ",
        "break log ",
        "break enable ",
        "break disable ",
        "break delete ",
        "break sync",
        "trace breakpoint",
        "trace dap-profile ",
        "run",
        "open",
        "refresh",
        "delete",
        "files",
        "search",
        "source files",
        "source refs",
        "source symbols",
        "source diagnostics",
        "source trace",
        "source pinned",
        "source debug",
        "query ",
        "preview lock",
        "preview up",
        "preview down",
        "preview reset",
        "quit",
    ]
}

fn palette_command_matches(command: &str) -> Vec<&'static str> {
    let command = command.trim();
    if command.is_empty() {
        return palette_command_names().iter().take(8).copied().collect();
    }

    let mut scored = palette_command_names()
        .iter()
        .filter_map(|name| fuzzy_score(name, command).map(|score| (score, *name)))
        .collect::<Vec<_>>();
    scored.sort_by_key(|(score, name)| (*score, *name));
    scored.into_iter().take(8).map(|(_, name)| name).collect()
}

fn command_hint_text(command: &str) -> String {
    let matches = palette_command_matches(command);
    if matches.is_empty() {
        return format!("No command match for '{}'", command.trim());
    }

    format!("Command matches: {}", matches.join(", "))
}

fn command_help_text() -> String {
    "Commands: source <mode> | query <text> | def refs type impl symbols diag incoming outgoing hover | trace breakpoint/dap-profile | break if/hit/log/delete/sync | dap start/real/sync/next/continue/pause/restart/stop/jump/adapters | watch add/del/clear/refresh | eval <expr> | preview lock/up/down quit"
        .to_string()
}

fn compact_status(text: &str) -> String {
    let mut value = text.lines().collect::<Vec<&str>>().join(" ");
    const MAX_STATUS_LEN: usize = 180;
    if value.chars().count() > MAX_STATUS_LEN {
        value = value.chars().take(MAX_STATUS_LEN).collect::<String>();
        value.push_str("...");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_score_matches_ordered_characters() {
        assert!(fuzzy_score("src/main.rs", "smr").is_some());
        assert!(fuzzy_score("src/main.rs", "zzz").is_none());
    }

    #[test]
    fn parse_mode_accepts_aliases() {
        assert_eq!(parse_mode(Some("symbol")).unwrap(), SourceMode::Symbols);
        assert_eq!(parse_mode(Some("refs")).unwrap(), SourceMode::References);
    }

    #[test]
    fn source_mode_wraps_forward_and_backward() {
        assert_eq!(source_mode_after(SourceMode::Search, -1), SourceMode::Debug);
        assert_eq!(source_mode_after(SourceMode::Debug, 1), SourceMode::Search);
        assert_eq!(source_mode_after(SourceMode::Files, 2), SourceMode::References);
    }

    #[test]
    fn selection_wraps_and_handles_empty_results() {
        assert_eq!(selection_after(0, 0, 1), 0);
        assert_eq!(selection_after(0, 3, -1), 2);
        assert_eq!(selection_after(2, 3, 1), 0);
        assert_eq!(selection_after(1, 3, 5), 0);
    }

    #[test]
    fn query_edit_helpers_handle_unicode_and_words() {
        assert_eq!(query_with_cursor("main", 2, true), "ma|in");
        assert_eq!(query_with_cursor("main", 2, false), "main");

        let (query, cursor) = insert_char_at("ab", 1, '界');
        assert_eq!(query, "a界b");
        assert_eq!(cursor, 2);

        let (query, cursor) = delete_char_before_cursor(&query, cursor);
        assert_eq!(query, "ab");
        assert_eq!(cursor, 1);

        let (query, cursor) = delete_char_at_cursor(&query, cursor);
        assert_eq!(query, "a");
        assert_eq!(cursor, 1);

        let (query, cursor) = delete_word_before_cursor("foo bar  ", 9);
        assert_eq!(query, "foo ");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn history_index_wraps_without_mutating_empty_history() {
        assert_eq!(history_index_after(None, 0, 1), None);
        assert_eq!(history_index_after(None, 3, 1), Some(0));
        assert_eq!(history_index_after(None, 3, -1), Some(2));
        assert_eq!(history_index_after(Some(2), 3, 1), Some(0));
        assert_eq!(history_index_after(Some(0), 3, -1), Some(2));
    }

    #[test]
    fn palette_commands_map_to_actions() {
        assert_eq!(palette_command_to_action("def"), Some(AppAction::Definition));
        assert_eq!(palette_command_to_action(" outgoing "), Some(AppAction::OutgoingCalls));
        assert_eq!(palette_command_to_action("missing"), None);
    }

    #[test]
    fn palette_suggests_debug_watch_commands() {
        assert!(palette_command_matches("dap ref").contains(&"dap refresh"));
        assert!(palette_command_matches("watch del").contains(&"watch del "));
        assert!(palette_command_matches("dap real").contains(&"dap real "));
        assert!(palette_command_matches("break sy").contains(&"break sync"));
        assert!(palette_command_matches("trace dap").contains(&"trace dap-profile "));
        assert!(command_help_text().contains("watch add/del/clear/refresh"));
    }

    #[test]
    fn source_worker_ignores_stale_responses_when_draining() {
        let (request_sender, _request_receiver) = std::sync::mpsc::channel::<SourceRequest>();
        let (response_sender, response_receiver) = std::sync::mpsc::channel::<SourceResponse>();
        let mut worker = SourceWorker {
            sender: request_sender,
            receiver: response_receiver,
            next_id: 1,
            latest_id: 2,
            latest_cancel: None,
        };

        response_sender
            .send(SourceResponse {
                id: 1,
                mode: SourceMode::Search,
                query: "old".to_string(),
                result: Ok(vec![CodeItem::file("old.rs")]),
            })
            .unwrap();
        response_sender
            .send(SourceResponse {
                id: 2,
                mode: SourceMode::Search,
                query: "new".to_string(),
                result: Ok(vec![CodeItem::file("new.rs")]),
            })
            .unwrap();

        let response = worker.try_recv_latest().expect("latest response should be kept");
        let items = response.result.unwrap();

        assert_eq!(response.id, 2);
        assert_eq!(response.query, "new");
        assert_eq!(items[0].display_text(), "new.rs");
        assert!(worker.try_recv_latest().is_none());
    }

    #[test]
    fn search_cancel_token_flips_state() {
        let cancel = crate::search::SearchCancel::default();
        assert!(!cancel.is_cancelled());

        cancel.cancel();

        assert!(cancel.is_cancelled());
    }
}
