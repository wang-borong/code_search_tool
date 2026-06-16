use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::config::Config;
use crate::core::{CodeItem, Location};
use crate::errors::{AppError, Result};

#[derive(Debug, Clone)]
pub(super) enum LspCommand {
    Definition(Location),
    TypeDefinition(Location),
    Implementation(Location),
    References(Location),
    Diagnostics(PathBuf),
    WorkspaceSymbols(String),
    DocumentSymbols(PathBuf),
    Hover(Location),
    IncomingCalls(Location),
    OutgoingCalls(Location),
}

#[derive(Debug)]
pub(super) enum LspPayload {
    Items(Vec<CodeItem>),
    Text(String),
}

#[derive(Debug)]
struct LspRequest {
    id: u64,
    command: LspCommand,
}

#[derive(Debug)]
pub(super) struct LspResponse {
    pub(super) id: u64,
    pub(super) label: &'static str,
    pub(super) result: Result<LspPayload>,
}

#[derive(Debug)]
pub(super) struct LspWorker {
    sender: Sender<LspRequest>,
    receiver: Receiver<LspResponse>,
    next_id: u64,
    latest_id: u64,
}

impl LspWorker {
    pub(super) fn start(root: PathBuf, config: Config) -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<LspRequest>();
        let (response_sender, response_receiver) = mpsc::channel::<LspResponse>();

        thread::spawn(move || {
            let mut clients: HashMap<crate::lsp::LspProviderKind, crate::lsp::LspClient> = HashMap::new();
            while let Ok(mut request) = request_receiver.recv() {
                while let Ok(newer_request) = request_receiver.try_recv() {
                    request = newer_request;
                }

                let (label, result) = run_lsp_request(&root, &config, &mut clients, request.command);
                if response_sender
                    .send(LspResponse {
                        id: request.id,
                        label,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            sender: request_sender,
            receiver: response_receiver,
            next_id: 1,
            latest_id: 0,
        }
    }

    pub(super) fn request(&mut self, command: LspCommand) -> Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        self.latest_id = id;
        self.sender
            .send(LspRequest { id, command })
            .map_err(|err| AppError::General(err.to_string()))?;
        Ok(id)
    }

    pub(super) fn try_recv_latest(&mut self) -> Option<LspResponse> {
        let mut latest = None;
        while let Ok(response) = self.receiver.try_recv() {
            if response.id == self.latest_id {
                latest = Some(response);
            }
        }
        latest
    }
}

fn run_lsp_request(
    root: &Path,
    config: &Config,
    clients: &mut HashMap<crate::lsp::LspProviderKind, crate::lsp::LspClient>,
    command: LspCommand,
) -> (&'static str, Result<LspPayload>) {
    let label = match &command {
        LspCommand::Definition(_) => "Definitions",
        LspCommand::TypeDefinition(_) => "Type Definitions",
        LspCommand::Implementation(_) => "Implementations",
        LspCommand::References(_) => "References",
        LspCommand::Diagnostics(_) => "Diagnostics",
        LspCommand::WorkspaceSymbols(_) => "Workspace Symbols",
        LspCommand::DocumentSymbols(_) => "Document Symbols",
        LspCommand::Hover(_) => "Hover",
        LspCommand::IncomingCalls(_) => "Incoming Calls",
        LspCommand::OutgoingCalls(_) => "Outgoing Calls",
    };

    let result = run_lsp_command(root, config, clients, command);
    (label, result)
}

fn run_lsp_command(
    root: &Path,
    config: &Config,
    clients: &mut HashMap<crate::lsp::LspProviderKind, crate::lsp::LspClient>,
    command: LspCommand,
) -> Result<LspPayload> {
    let fallback_request = semantic_fallback_request(&command);
    let primary_result = run_lsp_command_primary(root, config, clients, command);
    match primary_result {
        Ok(LspPayload::Items(items)) if items.is_empty() => {
            if let Some(request) = fallback_request {
                let fallback = semantic_fallback_items(root, &request, "lsp returned no edges")?;
                if !fallback.is_empty() {
                    return Ok(LspPayload::Items(fallback));
                }
            }
            Ok(LspPayload::Items(items))
        }
        Err(err) => {
            if let Some(request) = fallback_request {
                let reason = format!("lsp failed: {err}");
                let fallback = semantic_fallback_items(root, &request, &reason)?;
                if !fallback.is_empty() {
                    return Ok(LspPayload::Items(fallback));
                }
            }
            Err(err)
        }
        result => result,
    }
}

fn run_lsp_command_primary(
    root: &Path,
    config: &Config,
    clients: &mut HashMap<crate::lsp::LspProviderKind, crate::lsp::LspClient>,
    command: LspCommand,
) -> Result<LspPayload> {
    let provider = provider_for_command(root, &config.lsp, &command)?;
    let provider_kind = provider.kind();
    if let std::collections::hash_map::Entry::Vacant(entry) = clients.entry(provider_kind) {
        let client = crate::lsp::LspClient::start(provider, root, config.lsp.request_timeout_ms)?;
        entry.insert(client);
    }

    let client = clients
        .get_mut(&provider_kind)
        .ok_or_else(|| AppError::General("Failed to initialize LSP provider".to_string()))?;
    match command {
        LspCommand::Definition(location) => client.definition(&location).map(LspPayload::Items),
        LspCommand::TypeDefinition(location) => client.type_definition(&location).map(LspPayload::Items),
        LspCommand::Implementation(location) => client.implementation(&location).map(LspPayload::Items),
        LspCommand::References(location) => client.references(&location).map(LspPayload::Items),
        LspCommand::Diagnostics(path) => client.diagnostics(&path).map(LspPayload::Items),
        LspCommand::WorkspaceSymbols(query) => client.workspace_symbols(&query).map(LspPayload::Items),
        LspCommand::DocumentSymbols(path) => client.document_symbols(&path).map(LspPayload::Items),
        LspCommand::Hover(location) => client.hover(&location).map(LspPayload::Text),
        LspCommand::IncomingCalls(location) => client.incoming_calls(&location).map(LspPayload::Items),
        LspCommand::OutgoingCalls(location) => client.outgoing_calls(&location).map(LspPayload::Items),
    }
}

#[derive(Debug, Clone)]
struct SemanticFallbackRequest {
    location: Location,
    relation: String,
}

fn semantic_fallback_request(command: &LspCommand) -> Option<SemanticFallbackRequest> {
    let (location, relation) = match command {
        LspCommand::Definition(location) => (location, "definition"),
        LspCommand::TypeDefinition(location) => (location, "type"),
        LspCommand::Implementation(location) => (location, "implementation"),
        LspCommand::References(location) => (location, "references"),
        LspCommand::IncomingCalls(location) => (location, "incoming"),
        LspCommand::OutgoingCalls(location) => (location, "outgoing"),
        LspCommand::Diagnostics(_)
        | LspCommand::WorkspaceSymbols(_)
        | LspCommand::DocumentSymbols(_)
        | LspCommand::Hover(_) => return None,
    };
    Some(SemanticFallbackRequest {
        location: location.clone(),
        relation: relation.to_string(),
    })
}

fn semantic_fallback_items(root: &Path, request: &SemanticFallbackRequest, reason: &str) -> Result<Vec<CodeItem>> {
    let edges = crate::graph::index_fallback_edges(
        root,
        &request.location,
        &request.relation,
        reason,
        &crate::graph::GraphOptions {
            limit: 50,
            depth: 1,
            fanout: 0,
            exclude: Vec::new(),
        },
    )?;
    Ok(edges
        .into_iter()
        .map(|edge| fallback_edge_to_item(root, edge))
        .collect())
}

fn fallback_edge_to_item(root: &Path, edge: crate::graph::GraphEdge) -> CodeItem {
    let (path_label, line, name) = parse_fallback_target(&edge.to);
    let path = PathBuf::from(&path_label);
    let path = if path.is_absolute() { path } else { root.join(path) };
    CodeItem::symbol(
        path,
        path_label,
        line,
        None,
        format!("{name} ({})", edge.detail),
        edge.kind,
    )
}

fn parse_fallback_target(target: &str) -> (String, usize, String) {
    let Some((path, rest)) = target.split_once(':') else {
        return (target.to_string(), 1, target.to_string());
    };
    let digits = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    let line = digits.parse::<usize>().unwrap_or(1).max(1);
    let name = rest
        .get(digits.len()..)
        .unwrap_or_default()
        .trim()
        .trim_start_matches(':')
        .trim()
        .to_string();
    let name = if name.is_empty() { path.to_string() } else { name };
    (path.to_string(), line, name)
}

fn provider_for_command(
    root: &Path,
    config: &crate::config::LspConfig,
    command: &LspCommand,
) -> Result<crate::lsp::LspProviderSpec> {
    match command {
        LspCommand::Definition(location)
        | LspCommand::TypeDefinition(location)
        | LspCommand::Implementation(location)
        | LspCommand::References(location)
        | LspCommand::Hover(location)
        | LspCommand::IncomingCalls(location)
        | LspCommand::OutgoingCalls(location) => crate::lsp::provider_for_path(location.path(), config),
        LspCommand::Diagnostics(path) | LspCommand::DocumentSymbols(path) => {
            crate::lsp::provider_for_path(path, config)
        }
        LspCommand::WorkspaceSymbols(_) => Ok(crate::lsp::provider_for_workspace(root, config)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fallback_target_with_path_line_and_name() {
        let (path, line, name) = parse_fallback_target("src/main.rs:42 handle_request");

        assert_eq!(path, "src/main.rs");
        assert_eq!(line, 42);
        assert_eq!(name, "handle_request");
    }

    #[test]
    fn semantic_fallback_excludes_non_navigation_commands() {
        let command = LspCommand::Hover(Location::new("src/main.rs", Some(1), None));

        assert!(semantic_fallback_request(&command).is_none());
    }
}
