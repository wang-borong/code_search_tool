use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::{CodeItem, CodeItemKind, Location};
use crate::errors::{AppError, Result};

use super::SourceMode;

const TUI_STATE_FILE: &str = "tui_state.toml";
const TUI_STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TuiPersistentState {
    #[serde(default = "default_version")]
    pub(super) version: u32,
    #[serde(default)]
    pub(super) mode: Option<String>,
    #[serde(default)]
    pub(super) query: String,
    #[serde(default)]
    pub(super) pinned_items: Vec<TuiSavedItem>,
    #[serde(default)]
    pub(super) navigation: Vec<TuiSavedItem>,
    #[serde(default)]
    pub(super) navigation_index: Option<usize>,
    #[serde(default)]
    pub(super) breakpoints: Vec<TuiSavedLocation>,
    #[serde(default)]
    pub(super) debug_breakpoints: Vec<TuiSavedBreakpoint>,
    #[serde(default)]
    pub(super) locked_preview: Option<TuiSavedLocation>,
    #[serde(default)]
    pub(super) preview_scroll: isize,
    #[serde(default)]
    pub(super) command_history: Vec<String>,
    #[serde(default)]
    pub(super) active_trace_session: Option<String>,
    #[serde(default)]
    pub(super) layout_preset: Option<String>,
    #[serde(default)]
    pub(super) trace_view: Option<String>,
    #[serde(default)]
    pub(super) result_filter: Option<String>,
    #[serde(default)]
    pub(super) result_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TuiSavedItem {
    pub(super) kind: String,
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) path: PathBuf,
    #[serde(default)]
    pub(super) line: Option<usize>,
    #[serde(default)]
    pub(super) column: Option<usize>,
    pub(super) display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TuiSavedLocation {
    pub(super) path: PathBuf,
    #[serde(default)]
    pub(super) line: Option<usize>,
    #[serde(default)]
    pub(super) column: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TuiSavedBreakpoint {
    pub(super) location: TuiSavedLocation,
    #[serde(default = "default_true")]
    pub(super) enabled: bool,
    #[serde(default)]
    pub(super) condition: Option<String>,
    #[serde(default)]
    pub(super) hit_condition: Option<String>,
    #[serde(default)]
    pub(super) log_message: Option<String>,
}

impl TuiPersistentState {
    pub(super) fn mode(&self) -> Option<SourceMode> {
        self.mode.as_deref().and_then(SourceMode::from_short_label)
    }
}

impl Default for TuiPersistentState {
    fn default() -> Self {
        Self {
            version: TUI_STATE_VERSION,
            mode: None,
            query: String::new(),
            pinned_items: Vec::new(),
            navigation: Vec::new(),
            navigation_index: None,
            breakpoints: Vec::new(),
            debug_breakpoints: Vec::new(),
            locked_preview: None,
            preview_scroll: 0,
            command_history: Vec::new(),
            active_trace_session: None,
            layout_preset: None,
            trace_view: None,
            result_filter: None,
            result_group: None,
        }
    }
}

impl TuiSavedItem {
    pub(super) fn from_code_item(item: &CodeItem) -> Self {
        Self {
            kind: kind_label(&item.kind).to_string(),
            label: item.label.clone(),
            detail: item.detail.clone(),
            path: item.location.path.clone(),
            line: item.location.line,
            column: item.location.column,
            display: item.display_text().to_string(),
        }
    }

    pub(super) fn into_code_item(self) -> Option<CodeItem> {
        Some(CodeItem::from_parts(
            parse_kind(&self.kind)?,
            self.label,
            self.detail,
            Location::new(self.path, self.line, self.column),
            self.display,
        ))
    }
}

impl TuiSavedLocation {
    pub(super) fn from_location(location: &Location) -> Self {
        Self {
            path: location.path.clone(),
            line: location.line,
            column: location.column,
        }
    }

    pub(super) fn into_location(self) -> Location {
        Location::new(self.path, self.line, self.column)
    }
}

impl TuiSavedBreakpoint {
    pub(super) fn from_breakpoint(breakpoint: &crate::dap::DapBreakpoint) -> Self {
        Self {
            location: TuiSavedLocation {
                path: breakpoint.path.clone(),
                line: Some(breakpoint.line),
                column: breakpoint.column,
            },
            enabled: breakpoint.enabled,
            condition: breakpoint.condition.clone(),
            hit_condition: breakpoint.hit_condition.clone(),
            log_message: breakpoint.log_message.clone(),
        }
    }

    pub(super) fn into_breakpoint(self) -> crate::dap::DapBreakpoint {
        let location = self.location.into_location();
        crate::dap::DapBreakpoint {
            path: location.path,
            line: location.line.unwrap_or(1),
            column: location.column,
            enabled: self.enabled,
            condition: self.condition,
            hit_condition: self.hit_condition,
            log_message: self.log_message,
        }
    }
}

pub(super) fn load(root: &Path) -> Result<Option<TuiPersistentState>> {
    let path = state_path(root)?;
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)?;
    let mut state = toml::from_str::<TuiPersistentState>(&contents)
        .map_err(|err| AppError::General(format!("Failed to parse TUI state: {err}")))?;
    if state.version == 0 {
        state.version = TUI_STATE_VERSION;
    }
    Ok(Some(state))
}

pub(super) fn save(root: &Path, state: &TuiPersistentState) -> Result<PathBuf> {
    let path = state_path(root)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(state).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(&path, contents)?;
    Ok(path)
}

fn state_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join(TUI_STATE_FILE))
}

fn default_version() -> u32 {
    TUI_STATE_VERSION
}

fn default_true() -> bool {
    true
}

fn kind_label(kind: &CodeItemKind) -> &'static str {
    match kind {
        CodeItemKind::File => "file",
        CodeItemKind::Symbol => "symbol",
        CodeItemKind::TextMatch => "text-match",
    }
}

fn parse_kind(value: &str) -> Option<CodeItemKind> {
    match value {
        "file" => Some(CodeItemKind::File),
        "symbol" => Some(CodeItemKind::Symbol),
        "text-match" | "text" => Some(CodeItemKind::TextMatch),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_item_round_trips_code_item() {
        let item = CodeItem::symbol("src/main.rs", "src/main.rs", 7, Some(2), "main", "function");
        let saved = TuiSavedItem::from_code_item(&item);
        let restored = saved.into_code_item().unwrap();

        assert_eq!(restored.display_text(), item.display_text());
        assert_eq!(restored.location, item.location);
    }

    #[test]
    fn persistent_state_maps_saved_mode() {
        let state = TuiPersistentState {
            mode: Some("debug".to_string()),
            ..TuiPersistentState::default()
        };

        assert_eq!(state.mode(), Some(SourceMode::Debug));
    }

    #[test]
    fn saved_breakpoint_preserves_advanced_fields() {
        let saved = TuiSavedBreakpoint {
            location: TuiSavedLocation {
                path: PathBuf::from("src/main.rs"),
                line: Some(9),
                column: Some(2),
            },
            enabled: false,
            condition: Some("argc > 1".to_string()),
            hit_condition: Some("3".to_string()),
            log_message: Some("argc changed".to_string()),
        };

        let breakpoint = saved.clone().into_breakpoint();
        let restored = TuiSavedBreakpoint::from_breakpoint(&breakpoint);

        assert_eq!(breakpoint.line, 9);
        assert!(!breakpoint.enabled);
        assert_eq!(restored.condition, saved.condition);
        assert_eq!(restored.hit_condition, saved.hit_condition);
        assert_eq!(restored.log_message, saved.log_message);
    }
}
