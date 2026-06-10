use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::TuiKeymapConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AppAction {
    Quit,
    ActivateQuery,
    ActivateCommandPalette,
    BeginGoto,
    Refresh,
    PinSelected,
    UnpinSelected,
    LoadPinned,
    AddTrace,
    AddBreakpoint,
    AddTraceBreakpoints,
    DeleteSelected,
    ShowDebug,
    RequestDebugRun,
    WorkspaceSymbols,
    ShowHelp,
    JumpNavigation(isize),
    CycleOperation(isize),
    IncomingCalls,
    OutgoingCalls,
    Diagnostics,
    Hover,
    Implementation,
    DocumentSymbols,
    TypeDefinition,
    Open,
    NextSource,
    PreviousSource,
    MoveSelection(isize),
    ScrollPreview(isize),
    TogglePreviewLock,
    Definition,
    References,
}

pub(super) fn action_for_key(key: KeyEvent, keymap: &TuiKeymapConfig) -> Option<AppAction> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(AppAction::Quit);
    }

    if let Some(action) = configured_action_for_key(key, keymap) {
        return Some(action);
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(AppAction::Quit),
        KeyCode::Char('/') => Some(AppAction::ActivateQuery),
        KeyCode::Char(':') => Some(AppAction::ActivateCommandPalette),
        KeyCode::Char('g') => Some(AppAction::BeginGoto),
        KeyCode::Char('r') => Some(AppAction::Refresh),
        KeyCode::Char('p') => Some(AppAction::PinSelected),
        KeyCode::Char('u') => Some(AppAction::UnpinSelected),
        KeyCode::Char('a') => Some(AppAction::AddTrace),
        KeyCode::Char('b') => Some(AppAction::AddBreakpoint),
        KeyCode::Char('B') => Some(AppAction::AddTraceBreakpoints),
        KeyCode::Char('x') => Some(AppAction::DeleteSelected),
        KeyCode::Char('D') => Some(AppAction::ShowDebug),
        KeyCode::Char('X') => Some(AppAction::RequestDebugRun),
        KeyCode::Char('P') => Some(AppAction::TogglePreviewLock),
        KeyCode::Char('W') => Some(AppAction::WorkspaceSymbols),
        KeyCode::Char('?') => Some(AppAction::ShowHelp),
        KeyCode::Char('[') => Some(AppAction::JumpNavigation(-1)),
        KeyCode::Char(']') => Some(AppAction::JumpNavigation(1)),
        KeyCode::Char('n') => Some(AppAction::CycleOperation(1)),
        KeyCode::Char('N') => Some(AppAction::CycleOperation(-1)),
        KeyCode::Char('c') => Some(AppAction::IncomingCalls),
        KeyCode::Char('C') => Some(AppAction::OutgoingCalls),
        KeyCode::Char('e') => Some(AppAction::Diagnostics),
        KeyCode::Char('h') => Some(AppAction::Hover),
        KeyCode::Char('i') => Some(AppAction::Implementation),
        KeyCode::Char('s') => Some(AppAction::DocumentSymbols),
        KeyCode::Char('t') => Some(AppAction::TypeDefinition),
        KeyCode::Enter | KeyCode::Char('o') => Some(AppAction::Open),
        KeyCode::Tab | KeyCode::Right => Some(AppAction::NextSource),
        KeyCode::BackTab | KeyCode::Left => Some(AppAction::PreviousSource),
        KeyCode::Down | KeyCode::Char('j') => Some(AppAction::MoveSelection(1)),
        KeyCode::Up | KeyCode::Char('k') => Some(AppAction::MoveSelection(-1)),
        KeyCode::PageDown => Some(AppAction::ScrollPreview(5)),
        KeyCode::PageUp => Some(AppAction::ScrollPreview(-5)),
        _ => None,
    }
}

pub(super) fn goto_action_for_key(key: KeyEvent) -> Option<AppAction> {
    match key.code {
        KeyCode::Char('d') => Some(AppAction::Definition),
        KeyCode::Char('r') => Some(AppAction::References),
        KeyCode::Char('t') => Some(AppAction::TypeDefinition),
        KeyCode::Char('i') => Some(AppAction::Implementation),
        _ => None,
    }
}

fn configured_action_for_key(key: KeyEvent, keymap: &TuiKeymapConfig) -> Option<AppAction> {
    let value = key_to_string(key)?;
    match value.as_str() {
        value if value == keymap.command_palette => Some(AppAction::ActivateCommandPalette),
        value if value == keymap.query => Some(AppAction::ActivateQuery),
        value if value == keymap.open => Some(AppAction::Open),
        value if value == keymap.refresh => Some(AppAction::Refresh),
        value if value == keymap.trace => Some(AppAction::AddTrace),
        value if value == keymap.breakpoint => Some(AppAction::AddBreakpoint),
        value if value == keymap.debug => Some(AppAction::ShowDebug),
        _ => None,
    }
}

fn key_to_string(key: KeyEvent) -> Option<String> {
    match key.code {
        KeyCode::Char(ch) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => Some(ch.to_string()),
        KeyCode::Enter => Some("enter".to_string()),
        KeyCode::Tab => Some("tab".to_string()),
        KeyCode::BackTab => Some("shift-tab".to_string()),
        KeyCode::Esc => Some("esc".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_navigation_and_lsp_keys() {
        assert_eq!(
            action_for_key(KeyEvent::from(KeyCode::Char('g')), &TuiKeymapConfig::default()),
            Some(AppAction::BeginGoto)
        );
        assert_eq!(
            goto_action_for_key(KeyEvent::from(KeyCode::Char('d'))),
            Some(AppAction::Definition)
        );
        assert_eq!(
            action_for_key(KeyEvent::from(KeyCode::Char('C')), &TuiKeymapConfig::default()),
            Some(AppAction::OutgoingCalls)
        );
    }

    #[test]
    fn configured_keymap_overrides_core_actions() {
        let keymap = TuiKeymapConfig {
            command_palette: ";".to_string(),
            ..Default::default()
        };

        assert_eq!(
            action_for_key(KeyEvent::from(KeyCode::Char(';')), &keymap),
            Some(AppAction::ActivateCommandPalette)
        );
    }
}
