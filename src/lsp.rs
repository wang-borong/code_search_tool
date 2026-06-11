use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::LspConfig;
use crate::core::{CodeItem, Location};
use crate::errors::{AppError, Result};

pub const RUST_ANALYZER_COMMAND: &str = "rust-analyzer";
const DEFAULT_LSP_RETRIES: usize = 1;
const SEMANTIC_TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "class",
    "enum",
    "interface",
    "struct",
    "typeParameter",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "event",
    "function",
    "method",
    "macro",
    "keyword",
    "modifier",
    "comment",
    "string",
    "number",
    "regexp",
    "operator",
    "decorator",
];
const SEMANTIC_TOKEN_MODIFIERS: &[&str] = &[
    "declaration",
    "definition",
    "readonly",
    "static",
    "deprecated",
    "abstract",
    "async",
    "modification",
    "documentation",
    "defaultLibrary",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LspProviderKind {
    Clangd,
    RustAnalyzer,
}

impl LspProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clangd => "clangd",
            Self::RustAnalyzer => "rust-analyzer",
        }
    }
}

pub trait LanguageServerProvider {
    fn kind(&self) -> LspProviderKind;
    fn command(&self, config: &LspConfig) -> String;
    fn supports_path(&self, path: &Path) -> bool;
    fn language_id(&self, path: &Path) -> &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspProviderHealthStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspProviderHealth {
    pub kind: LspProviderKind,
    pub command: String,
    pub status: LspProviderHealthStatus,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspClientHealthStatus {
    Running,
    Exited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspClientHealth {
    pub kind: LspProviderKind,
    pub command: String,
    pub status: LspClientHealthStatus,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEdit {
    pub edits: Vec<LspTextEdit>,
    pub unsupported_operations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspTextEdit {
    pub path: PathBuf,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub new_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEditApplyReport {
    pub dry_run: bool,
    pub edit_count: usize,
    pub changed_files: Vec<PathBuf>,
    pub unsupported_operations: Vec<String>,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeActionCandidate {
    pub title: String,
    pub kind: String,
    pub edit: WorkspaceEdit,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSymbolNode {
    pub name: String,
    pub kind: String,
    pub path: PathBuf,
    pub line: usize,
    pub column: Option<usize>,
    pub end_line: usize,
    pub end_column: Option<usize>,
    #[serde(default)]
    pub children: Vec<DocumentSymbolNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticToken {
    pub line: usize,
    pub start_column: usize,
    pub length: usize,
    pub token_type: String,
    pub modifiers: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ClangdProvider;

impl LanguageServerProvider for ClangdProvider {
    fn kind(&self) -> LspProviderKind {
        LspProviderKind::Clangd
    }

    fn command(&self, config: &LspConfig) -> String {
        config.clangd_command.clone()
    }

    fn supports_path(&self, path: &Path) -> bool {
        matches!(
            path.extension().and_then(|extension| extension.to_str()).unwrap_or(""),
            "c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" | "hxx"
        )
    }

    fn language_id(&self, path: &Path) -> &'static str {
        match path.extension().and_then(|extension| extension.to_str()).unwrap_or("") {
            "c" | "h" => "c",
            "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => "cpp",
            _ => "plaintext",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RustAnalyzerProvider;

impl LanguageServerProvider for RustAnalyzerProvider {
    fn kind(&self) -> LspProviderKind {
        LspProviderKind::RustAnalyzer
    }

    fn command(&self, _config: &LspConfig) -> String {
        RUST_ANALYZER_COMMAND.to_string()
    }

    fn supports_path(&self, path: &Path) -> bool {
        path.extension().and_then(|extension| extension.to_str()) == Some("rs")
    }

    fn language_id(&self, path: &Path) -> &'static str {
        if self.supports_path(path) {
            "rust"
        } else {
            "plaintext"
        }
    }
}

#[derive(Debug, Clone)]
pub struct LspProviderSpec {
    kind: LspProviderKind,
    command: String,
}

impl LspProviderSpec {
    pub fn clangd(command: &str) -> Self {
        Self {
            kind: LspProviderKind::Clangd,
            command: command.to_string(),
        }
    }

    pub fn rust_analyzer() -> Self {
        Self {
            kind: LspProviderKind::RustAnalyzer,
            command: RUST_ANALYZER_COMMAND.to_string(),
        }
    }

    pub fn from_provider(provider: &impl LanguageServerProvider, config: &LspConfig) -> Self {
        Self {
            kind: provider.kind(),
            command: provider.command(config),
        }
    }

    pub fn kind(&self) -> LspProviderKind {
        self.kind
    }

    pub fn name(&self) -> &'static str {
        self.kind.as_str()
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    fn language_id(&self, path: &Path) -> &'static str {
        match self.kind {
            LspProviderKind::Clangd => ClangdProvider.language_id(path),
            LspProviderKind::RustAnalyzer => RustAnalyzerProvider.language_id(path),
        }
    }

    fn install_hint(&self) -> &'static str {
        match self.kind {
            LspProviderKind::Clangd => "Install clangd or set lsp.clangd_command in fcs.toml.",
            LspProviderKind::RustAnalyzer => "Install rust-analyzer and ensure it is available on PATH.",
        }
    }
}

pub fn provider_for_path(path: &Path, config: &LspConfig) -> Result<LspProviderSpec> {
    if RustAnalyzerProvider.supports_path(path) {
        return Ok(LspProviderSpec::from_provider(&RustAnalyzerProvider, config));
    }

    if ClangdProvider.supports_path(path) {
        return Ok(LspProviderSpec::from_provider(&ClangdProvider, config));
    }

    Err(AppError::General(format!(
        "No LSP provider supports this file: {}",
        path.display()
    )))
}

pub fn provider_for_workspace(root: &Path, config: &LspConfig) -> LspProviderSpec {
    if root.join("Cargo.toml").exists()
        || root.join("src").join("lib.rs").exists()
        || root.join("src").join("main.rs").exists()
    {
        return LspProviderSpec::from_provider(&RustAnalyzerProvider, config);
    }

    LspProviderSpec::from_provider(&ClangdProvider, config)
}

pub fn provider_health(provider: &LspProviderSpec) -> LspProviderHealth {
    match provider_version(provider.command()) {
        Some(version) => LspProviderHealth {
            kind: provider.kind(),
            command: provider.command().to_string(),
            status: LspProviderHealthStatus::Available,
            version: Some(version),
            message: format!("{} provider is available", provider.name()),
        },
        None => LspProviderHealth {
            kind: provider.kind(),
            command: provider.command().to_string(),
            status: LspProviderHealthStatus::Unavailable,
            version: None,
            message: provider.install_hint().to_string(),
        },
    }
}

pub fn workspace_provider_health(root: &Path, config: &LspConfig) -> LspProviderHealth {
    let provider = provider_for_workspace(root, config);
    provider_health(&provider)
}

pub fn provider_version(command: &str) -> Option<String> {
    let (program, mut args) = split_command(command).ok()?;
    args.push("--version".to_string());
    let output = Command::new(program).args(args).output().ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .and_then(|stdout| stdout.lines().next().map(ToOwned::to_owned))
}

pub struct LspClient {
    provider: LspProviderSpec,
    root: PathBuf,
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<Value>,
    next_id: u64,
    timeout: Duration,
    retries: usize,
    opened_documents: HashMap<PathBuf, OpenDocumentSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenDocumentSnapshot {
    version: i32,
    size_bytes: u64,
    modified: Option<SystemTime>,
}

impl LspClient {
    pub fn start(provider: LspProviderSpec, root: &Path, timeout_ms: u64) -> Result<Self> {
        if provider_health(&provider).status != LspProviderHealthStatus::Available {
            return Err(AppError::General(format!(
                "{} provider is not available with command `{}`. {}",
                provider.name(),
                provider.command(),
                provider.install_hint()
            )));
        }

        let root = root.to_path_buf();
        let (child, stdin, receiver) = spawn_provider_process(&provider, &root)?;

        let mut client = Self {
            provider,
            root,
            child,
            stdin,
            receiver,
            next_id: 1,
            timeout: Duration::from_millis(timeout_ms),
            retries: DEFAULT_LSP_RETRIES,
            opened_documents: HashMap::new(),
        };

        client.initialize()?;
        Ok(client)
    }

    pub fn start_for_path(path: &Path, root: &Path, config: &LspConfig) -> Result<Self> {
        let provider = provider_for_path(path, config)?;
        Self::start(provider, root, config.request_timeout_ms)
    }

    pub fn start_for_workspace(root: &Path, config: &LspConfig) -> Result<Self> {
        let provider = provider_for_workspace(root, config);
        Self::start(provider, root, config.request_timeout_ms)
    }

    pub fn provider(&self) -> &LspProviderSpec {
        &self.provider
    }

    pub fn health(&mut self) -> LspClientHealth {
        match self.child.try_wait() {
            Ok(Some(status)) => LspClientHealth {
                kind: self.provider.kind(),
                command: self.provider.command().to_string(),
                status: LspClientHealthStatus::Exited,
                message: format!("{} provider exited with status {status}", self.provider.name()),
            },
            Ok(None) => LspClientHealth {
                kind: self.provider.kind(),
                command: self.provider.command().to_string(),
                status: LspClientHealthStatus::Running,
                message: format!("{} provider is running", self.provider.name()),
            },
            Err(err) => LspClientHealth {
                kind: self.provider.kind(),
                command: self.provider.command().to_string(),
                status: LspClientHealthStatus::Exited,
                message: format!("Failed to inspect {} provider: {err}", self.provider.name()),
            },
        }
    }

    pub fn restart(&mut self) -> Result<()> {
        let _ = self.child.kill();
        let _ = self.child.wait();

        let (child, stdin, receiver) = spawn_provider_process(&self.provider, &self.root)?;
        self.child = child;
        self.stdin = stdin;
        self.receiver = receiver;
        self.next_id = 1;
        self.opened_documents.clear();
        self.initialize()
    }

    pub fn definition(&mut self, location: &Location) -> Result<Vec<CodeItem>> {
        self.open_document(location.path())?;

        let response = self.request_with_retry(
            "textDocument/definition",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
            }),
        )?;

        locations_to_items(response.get("result").unwrap_or(&Value::Null), "definition")
    }

    pub fn type_definition(&mut self, location: &Location) -> Result<Vec<CodeItem>> {
        self.open_document(location.path())?;

        let response = self.request_with_retry(
            "textDocument/typeDefinition",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
            }),
        )?;

        locations_to_items(response.get("result").unwrap_or(&Value::Null), "type-definition")
    }

    pub fn implementation(&mut self, location: &Location) -> Result<Vec<CodeItem>> {
        self.open_document(location.path())?;

        let response = self.request_with_retry(
            "textDocument/implementation",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
            }),
        )?;

        locations_to_items(response.get("result").unwrap_or(&Value::Null), "implementation")
    }

    pub fn references(&mut self, location: &Location) -> Result<Vec<CodeItem>> {
        self.open_document(location.path())?;

        let response = self.request_with_retry(
            "textDocument/references",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
                "context": {
                    "includeDeclaration": true
                }
            }),
        )?;

        locations_to_items(response.get("result").unwrap_or(&Value::Null), "reference")
    }

    pub fn references_grouped(&mut self, location: &Location) -> Result<String> {
        let items = self.references(location)?;
        Ok(group_code_items("LSP References", &items))
    }

    pub fn document_highlights(&mut self, location: &Location) -> Result<Vec<CodeItem>> {
        self.open_document(location.path())?;

        let response = self.request_with_retry(
            "textDocument/documentHighlight",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
            }),
        )?;

        document_highlights_to_items(location.path(), response.get("result").unwrap_or(&Value::Null))
    }

    pub fn code_actions(&mut self, location: &Location) -> Result<Vec<CodeItem>> {
        self.open_document(location.path())?;

        let response = self.request_with_retry(
            "textDocument/codeAction",
            json!({
                "textDocument": text_document(location.path())?,
                "range": point_range(location),
                "context": {
                    "diagnostics": []
                }
            }),
        )?;

        code_actions_to_items(location, response.get("result").unwrap_or(&Value::Null))
    }

    pub fn code_action_candidates(&mut self, location: &Location) -> Result<Vec<CodeActionCandidate>> {
        self.code_action_candidates_with_only(location, &[])
    }

    fn code_action_candidates_with_only(
        &mut self,
        location: &Location,
        only: &[&str],
    ) -> Result<Vec<CodeActionCandidate>> {
        self.open_document(location.path())?;
        let only_value = if only.is_empty() {
            Value::Null
        } else {
            Value::Array(only.iter().map(|value| Value::String((*value).to_string())).collect())
        };
        let mut context = json!({ "diagnostics": [] });
        if !only_value.is_null() {
            context["only"] = only_value;
        }
        let response = self.request_with_retry(
            "textDocument/codeAction",
            json!({
                "textDocument": text_document(location.path())?,
                "range": point_range(location),
                "context": context
            }),
        )?;

        code_action_candidates_from_value(response.get("result").unwrap_or(&Value::Null))
    }

    pub fn organize_imports_candidates(&mut self, path: &Path) -> Result<Vec<CodeActionCandidate>> {
        self.open_document(path)?;
        let location = Location::new(path, Some(1), Some(1));
        self.code_action_candidates_with_only(&location, &["source.organizeImports"])
    }

    pub fn apply_code_action(
        &mut self,
        location: &Location,
        one_based_index: usize,
        dry_run: bool,
    ) -> Result<WorkspaceEditApplyReport> {
        let actions = self.code_action_candidates(location)?;
        let action = actions
            .get(one_based_index.saturating_sub(1))
            .ok_or_else(|| AppError::General(format!("Code action index out of range: {one_based_index}")))?;
        apply_workspace_edit(&action.edit, dry_run)
    }

    pub fn diagnostics(&mut self, path: &Path) -> Result<Vec<CodeItem>> {
        self.open_document(path)?;

        let uri = path_to_uri(path)?;
        let deadline = Instant::now() + self.timeout;
        let mut diagnostics = Vec::new();

        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let message = match self.receiver.recv_timeout(remaining) {
                Ok(message) => message,
                Err(_) => break,
            };

            if message.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
                continue;
            }

            let params = message.get("params").unwrap_or(&Value::Null);
            if params.get("uri").and_then(Value::as_str) != Some(uri.as_str()) {
                continue;
            }

            if let Some(values) = params.get("diagnostics").and_then(Value::as_array) {
                diagnostics.extend(diagnostics_to_items(path, values));
                break;
            }
        }

        Ok(diagnostics)
    }

    pub fn workspace_symbols(&mut self, query: &str) -> Result<Vec<CodeItem>> {
        let response = self.request_with_retry(
            "workspace/symbol",
            json!({
                "query": query
            }),
        )?;
        workspace_symbols_to_items(response.get("result").unwrap_or(&Value::Null))
    }

    pub fn document_symbols(&mut self, path: &Path) -> Result<Vec<CodeItem>> {
        self.open_document(path)?;
        let response = self.request_with_retry(
            "textDocument/documentSymbol",
            json!({
                "textDocument": text_document(path)?,
            }),
        )?;
        document_symbols_to_items(path, response.get("result").unwrap_or(&Value::Null))
    }

    pub fn hover(&mut self, location: &Location) -> Result<String> {
        self.open_document(location.path())?;
        let response = self.request_with_retry(
            "textDocument/hover",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
            }),
        )?;
        Ok(hover_to_string(response.get("result").unwrap_or(&Value::Null)))
    }

    pub fn rename_preview(&mut self, location: &Location, new_name: &str) -> Result<String> {
        Ok(self.rename_edit(location, new_name)?.preview())
    }

    pub fn rename_edit(&mut self, location: &Location, new_name: &str) -> Result<WorkspaceEdit> {
        self.open_document(location.path())?;
        let response = self.request_with_retry(
            "textDocument/rename",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
                "newName": new_name,
            }),
        )?;
        workspace_edit_from_value(response.get("result").unwrap_or(&Value::Null))
    }

    pub fn apply_rename(
        &mut self,
        location: &Location,
        new_name: &str,
        dry_run: bool,
    ) -> Result<WorkspaceEditApplyReport> {
        let edit = self.rename_edit(location, new_name)?;
        apply_workspace_edit(&edit, dry_run)
    }

    pub fn document_outline(&mut self, path: &Path) -> Result<Vec<DocumentSymbolNode>> {
        self.open_document(path)?;
        let response = self.request_with_retry(
            "textDocument/documentSymbol",
            json!({
                "textDocument": text_document(path)?,
            }),
        )?;
        document_outline_from_value(path, response.get("result").unwrap_or(&Value::Null))
    }

    pub fn breadcrumbs(&mut self, location: &Location) -> Result<Vec<DocumentSymbolNode>> {
        let outline = self.document_outline(location.path())?;
        Ok(breadcrumbs_from_outline(
            &outline,
            location.line.unwrap_or(1),
            location.column.unwrap_or(1),
        ))
    }

    pub fn semantic_tokens(&mut self, path: &Path, line_filter: Option<usize>) -> Result<Vec<SemanticToken>> {
        self.open_document(path)?;
        let response = self.request_with_retry(
            "textDocument/semanticTokens/full",
            json!({
                "textDocument": text_document(path)?,
            }),
        )?;
        let mut tokens = semantic_tokens_from_value(response.get("result").unwrap_or(&Value::Null))?;
        if let Some(line) = line_filter {
            tokens.retain(|token| token.line == line);
        }
        Ok(tokens)
    }

    pub fn incoming_calls(&mut self, location: &Location) -> Result<Vec<CodeItem>> {
        self.open_document(location.path())?;
        let prepare = self.request_with_retry(
            "textDocument/prepareCallHierarchy",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
            }),
        )?;
        let Some(item) = prepare
            .get("result")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .cloned()
        else {
            return Ok(Vec::new());
        };

        let response = self.request_with_retry(
            "callHierarchy/incomingCalls",
            json!({
                "item": item
            }),
        )?;
        call_hierarchy_items(response.get("result").unwrap_or(&Value::Null), "from", "incoming")
    }

    pub fn outgoing_calls(&mut self, location: &Location) -> Result<Vec<CodeItem>> {
        self.open_document(location.path())?;
        let prepare = self.request_with_retry(
            "textDocument/prepareCallHierarchy",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
            }),
        )?;
        let Some(item) = prepare
            .get("result")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .cloned()
        else {
            return Ok(Vec::new());
        };

        let response = self.request_with_retry(
            "callHierarchy/outgoingCalls",
            json!({
                "item": item
            }),
        )?;
        call_hierarchy_items(response.get("result").unwrap_or(&Value::Null), "to", "outgoing")
    }

    pub fn call_tree(&mut self, location: &Location) -> Result<String> {
        let incoming = self.incoming_calls(location)?;
        let outgoing = self.outgoing_calls(location)?;
        let mut output = String::from("# LSP Call Tree\n\n");
        output.push_str(&group_code_items("Incoming", &incoming));
        output.push('\n');
        output.push_str(&group_code_items("Outgoing", &outgoing));
        Ok(output)
    }

    fn initialize(&mut self) -> Result<()> {
        let root_uri = path_to_uri(&self.root)?;
        self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "synchronization": {
                            "didSave": true,
                            "dynamicRegistration": false
                        },
                        "definition": {},
                        "typeDefinition": {},
                        "implementation": {},
                        "references": {},
                        "documentHighlight": {},
                        "documentSymbol": {},
                        "hover": {},
                        "codeAction": {},
                        "rename": {
                            "prepareSupport": true
                        },
                        "semanticTokens": {
                            "dynamicRegistration": false,
                            "requests": {
                                "full": true,
                                "range": false
                            },
                            "tokenTypes": SEMANTIC_TOKEN_TYPES,
                            "tokenModifiers": SEMANTIC_TOKEN_MODIFIERS,
                            "formats": ["relative"],
                            "overlappingTokenSupport": false,
                            "multilineTokenSupport": false
                        },
                        "callHierarchy": {},
                        "publishDiagnostics": {}
                    },
                    "workspace": {
                        "symbol": {},
                        "workspaceEdit": {
                            "documentChanges": true
                        }
                    }
                }
            }),
        )?;
        self.notify("initialized", json!({}))?;
        Ok(())
    }

    fn open_document(&mut self, path: &Path) -> Result<()> {
        let text = fs::read_to_string(path)?;
        let snapshot = document_snapshot(path);
        let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(previous) = self.opened_documents.get_mut(&key) {
            if same_document_snapshot(*previous, snapshot) {
                return Ok(());
            }

            let next_version = previous.version + 1;
            *previous = OpenDocumentSnapshot {
                version: next_version,
                ..snapshot
            };
            return self.notify(
                "textDocument/didChange",
                json!({
                    "textDocument": {
                        "uri": path_to_uri(path)?,
                        "version": next_version,
                    },
                    "contentChanges": [
                        {
                            "text": text
                        }
                    ]
                }),
            );
        }

        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": path_to_uri(path)?,
                    "languageId": self.provider.language_id(path),
                    "version": 1,
                    "text": text
                }
            }),
        )?;
        self.opened_documents.insert(key, snapshot);
        Ok(())
    }

    fn request_with_retry(&mut self, method: &str, params: Value) -> Result<Value> {
        let mut last_error = None;
        for attempt in 0..=self.retries {
            match self.request(method, params.clone()) {
                Ok(value) => return Ok(value),
                Err(err) if attempt < self.retries && is_retryable_lsp_error(&err) => {
                    last_error = Some(err);
                    self.restart()?;
                    if let Some(path) = request_text_document_path(&params)? {
                        self.open_document(&path)?;
                    }
                }
                Err(err) => return Err(err),
            }
        }

        Err(last_error
            .unwrap_or_else(|| AppError::General(format!("Timed out waiting for LSP response after retry: {method}"))))
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        self.write(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))?;

        let deadline = Instant::now() + self.timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let message = self
                .receiver
                .recv_timeout(remaining)
                .map_err(|_| AppError::General(format!("Timed out waiting for LSP response: {method}")))?;

            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    return Err(AppError::General(format!("LSP request failed: {error}")));
                }
                return Ok(message);
            }
        }

        Err(AppError::General(format!(
            "Timed out waiting for LSP response: {method}"
        )))
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.write(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }))
    }

    fn write(&mut self, message: Value) -> Result<()> {
        let body = serde_json::to_vec(&message).map_err(|e| AppError::General(e.to_string()))?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len())?;
        self.stdin.write_all(&body)?;
        self.stdin.flush()?;
        Ok(())
    }
}

fn spawn_provider_process(provider: &LspProviderSpec, root: &Path) -> Result<(Child, ChildStdin, Receiver<Value>)> {
    let (program, args) = split_command(provider.command())?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            AppError::General(format!(
                "Failed to start {} provider with command `{}`: {e}. {}",
                provider.name(),
                provider.command(),
                provider.install_hint()
            ))
        })?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::General(format!("Failed to open {} stdin", provider.name())))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::General(format!("Failed to open {} stdout", provider.name())))?;
    let receiver = spawn_reader(stdout);

    Ok((child, stdin, receiver))
}

fn split_command(command: &str) -> Result<(String, Vec<String>)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for ch in command.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            None => current.push(ch),
        }
    }

    if escaped {
        current.push('\\');
    }

    if quote.is_some() {
        return Err(AppError::General(format!("Unclosed quote in LSP command: {command}")));
    }

    if !current.is_empty() {
        parts.push(current);
    }

    let program = parts
        .first()
        .cloned()
        .ok_or_else(|| AppError::General("LSP command is empty".to_string()))?;
    Ok((program, parts.into_iter().skip(1).collect()))
}

fn document_snapshot(path: &Path) -> OpenDocumentSnapshot {
    let metadata = fs::metadata(path).ok();
    OpenDocumentSnapshot {
        version: 1,
        size_bytes: metadata.as_ref().map_or(0, |value| value.len()),
        modified: metadata.and_then(|value| value.modified().ok()),
    }
}

fn same_document_snapshot(left: OpenDocumentSnapshot, right: OpenDocumentSnapshot) -> bool {
    left.size_bytes == right.size_bytes && left.modified == right.modified
}

fn is_retryable_lsp_error(err: &AppError) -> bool {
    let message = err.to_string();
    message.contains("Timed out waiting for LSP response")
        || message.contains("Broken pipe")
        || message.contains("Connection reset")
        || message.contains("LSP stdout closed")
}

fn request_text_document_path(params: &Value) -> Result<Option<PathBuf>> {
    let Some(uri) = params
        .get("textDocument")
        .and_then(|document| document.get("uri"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };

    uri_to_path(uri).map(Some)
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.notify("exit", json!({}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_reader(stdout: impl Read + Send + 'static) -> Receiver<Value> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Ok(message) = read_message(&mut reader) {
            if sender.send(message).is_err() {
                break;
            }
        }
    });
    receiver
}

fn read_message(reader: &mut BufReader<impl Read>) -> Result<Value> {
    let mut content_len = None;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Err(AppError::General("LSP stdout closed".to_string()));
        }

        let line = line.trim_end_matches(&['\r', '\n'][..]);
        if line.is_empty() {
            break;
        }

        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            content_len = line
                .split_once(':')
                .and_then(|(_, value)| value.trim().parse::<usize>().ok());
        }
    }

    let content_len = content_len.ok_or_else(|| AppError::General("Missing LSP Content-Length".to_string()))?;
    let mut buffer = vec![0; content_len];
    reader.read_exact(&mut buffer)?;
    serde_json::from_slice(&buffer).map_err(|e| AppError::General(e.to_string()))
}

fn locations_to_items(value: &Value, detail: &str) -> Result<Vec<CodeItem>> {
    let locations = match value {
        Value::Null => Vec::new(),
        Value::Array(values) => values.clone(),
        Value::Object(_) => vec![value.clone()],
        _ => Vec::new(),
    };

    let mut items = Vec::new();
    for location in locations {
        if let Some(item) = location_to_item(&location, detail)? {
            items.push(item);
        }
    }

    Ok(items)
}

fn location_to_item(value: &Value, detail: &str) -> Result<Option<CodeItem>> {
    let uri = value
        .get("uri")
        .or_else(|| value.get("targetUri"))
        .and_then(Value::as_str);
    let range = value
        .get("range")
        .or_else(|| value.get("targetSelectionRange"))
        .unwrap_or(&Value::Null);

    let Some(uri) = uri else {
        return Ok(None);
    };

    let path = uri_to_path(uri)?;
    let line = range
        .get("start")
        .and_then(|start| start.get("line"))
        .and_then(Value::as_u64)
        .map(|line| line as usize + 1)
        .unwrap_or(1);
    let column = range
        .get("start")
        .and_then(|start| start.get("character"))
        .and_then(Value::as_u64)
        .map(|column| column as usize + 1);

    let display_path = path.to_string_lossy().replace('\\', "/");
    Ok(Some(CodeItem::symbol(path, display_path, line, column, detail, "lsp")))
}

fn diagnostics_to_items(path: &Path, diagnostics: &[Value]) -> Vec<CodeItem> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let range = diagnostic.get("range").unwrap_or(&Value::Null);
            let line = range
                .get("start")
                .and_then(|start| start.get("line"))
                .and_then(Value::as_u64)
                .map(|line| line as usize + 1)
                .unwrap_or(1);
            let column = range
                .get("start")
                .and_then(|start| start.get("character"))
                .and_then(Value::as_u64)
                .map(|column| column as usize + 1);
            let message = diagnostic
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("diagnostic");
            let severity = diagnostic
                .get("severity")
                .and_then(Value::as_u64)
                .map(severity_label)
                .unwrap_or("info");
            let detail = format!("{message} [{severity}]");
            let display_path = path.to_string_lossy().replace('\\', "/");

            CodeItem::symbol(path.to_path_buf(), display_path, line, column, detail, "diagnostic")
        })
        .collect()
}

fn document_highlights_to_items(path: &Path, value: &Value) -> Result<Vec<CodeItem>> {
    let Some(values) = value.as_array() else {
        return Ok(Vec::new());
    };

    let display_path = path.to_string_lossy().replace('\\', "/");
    Ok(values
        .iter()
        .map(|highlight| {
            let range = highlight.get("range").unwrap_or(&Value::Null);
            let (line, column) = range_position(range);
            let kind = highlight
                .get("kind")
                .and_then(Value::as_u64)
                .map(highlight_kind_label)
                .unwrap_or("text");
            CodeItem::symbol(
                path.to_path_buf(),
                display_path.clone(),
                line,
                column,
                kind,
                "highlight",
            )
        })
        .collect())
}

fn code_actions_to_items(location: &Location, value: &Value) -> Result<Vec<CodeItem>> {
    let Some(values) = value.as_array() else {
        return Ok(Vec::new());
    };

    let line = location.line.unwrap_or(1);
    let display_path = location.display_path();
    Ok(values
        .iter()
        .filter_map(|action| {
            let title = action.get("title").and_then(Value::as_str)?;
            let kind = action
                .get("kind")
                .and_then(Value::as_str)
                .or_else(|| {
                    action
                        .get("command")
                        .and_then(|command| command.get("command"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("code-action");
            Some(CodeItem::symbol(
                location.path.clone(),
                display_path.clone(),
                line,
                location.column,
                title,
                kind,
            ))
        })
        .collect())
}

fn code_action_candidates_from_value(value: &Value) -> Result<Vec<CodeActionCandidate>> {
    let Some(values) = value.as_array() else {
        return Ok(Vec::new());
    };

    values
        .iter()
        .map(|action| {
            let title = action
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("code action")
                .to_string();
            let kind = action
                .get("kind")
                .and_then(Value::as_str)
                .or_else(|| {
                    action
                        .get("command")
                        .and_then(|command| command.get("command"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("code-action")
                .to_string();
            let edit = workspace_edit_from_value(action.get("edit").unwrap_or(&Value::Null))?;
            let command = action
                .get("command")
                .and_then(|command| {
                    command
                        .get("command")
                        .and_then(Value::as_str)
                        .or_else(|| command.as_str())
                })
                .map(ToOwned::to_owned);
            Ok(CodeActionCandidate {
                title,
                kind,
                edit,
                command,
            })
        })
        .collect()
}

fn workspace_symbols_to_items(value: &Value) -> Result<Vec<CodeItem>> {
    let Some(values) = value.as_array() else {
        return Ok(Vec::new());
    };

    let mut items = Vec::new();
    for value in values {
        let name = value.get("name").and_then(Value::as_str).unwrap_or("symbol");
        let label = value
            .get("containerName")
            .and_then(Value::as_str)
            .filter(|container| !container.is_empty())
            .map(|container| format!("{container}::{name}"))
            .unwrap_or_else(|| name.to_string());
        let kind = value
            .get("kind")
            .and_then(Value::as_u64)
            .map(symbol_kind_label)
            .unwrap_or("symbol");
        let location = value.get("location").unwrap_or(&Value::Null);
        if let Some(item) = location_to_named_item(location, &label, kind)? {
            items.push(item);
        }
    }

    Ok(items)
}

fn group_code_items(title: &str, items: &[CodeItem]) -> String {
    let mut output = format!("# {title}\n\n");
    if items.is_empty() {
        output.push_str("No results\n");
        return output;
    }

    let mut groups: BTreeMap<String, Vec<&CodeItem>> = BTreeMap::new();
    for item in items {
        groups.entry(item.location.display_path()).or_default().push(item);
    }

    for (path, entries) in groups {
        output.push_str(&format!("## {path}\n"));
        for entry in entries {
            let line = entry.location.line.unwrap_or(1);
            let column = entry
                .location
                .column
                .map(|column| format!(":{column}"))
                .unwrap_or_default();
            output.push_str(&format!("- {line}{column} {}\n", entry.detail));
        }
        output.push('\n');
    }

    output
}

fn document_symbols_to_items(path: &Path, value: &Value) -> Result<Vec<CodeItem>> {
    let Some(values) = value.as_array() else {
        return Ok(Vec::new());
    };

    let mut items = Vec::new();
    for value in values {
        collect_document_symbol(path, value, None, &mut items)?;
    }

    Ok(items)
}

fn document_outline_from_value(path: &Path, value: &Value) -> Result<Vec<DocumentSymbolNode>> {
    let Some(values) = value.as_array() else {
        return Ok(Vec::new());
    };

    values
        .iter()
        .filter_map(|value| match document_symbol_node(path, value) {
            Ok(Some(node)) => Some(Ok(node)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        })
        .collect()
}

fn document_symbol_node(path: &Path, value: &Value) -> Result<Option<DocumentSymbolNode>> {
    if let Some(location) = value.get("location") {
        let Some(mut node) = symbol_information_node(value, location)? else {
            return Ok(None);
        };
        node.path = path.to_path_buf();
        return Ok(Some(node));
    }

    let name = value.get("name").and_then(Value::as_str).unwrap_or("symbol");
    let kind = value
        .get("kind")
        .and_then(Value::as_u64)
        .map(symbol_kind_label)
        .unwrap_or("symbol");
    let range = value.get("range").unwrap_or(&Value::Null);
    let ((line, column), (end_line, end_column)) = edit_range_position(range);
    let mut children = Vec::new();
    if let Some(values) = value.get("children").and_then(Value::as_array) {
        for child in values {
            if let Some(child) = document_symbol_node(path, child)? {
                children.push(child);
            }
        }
    }

    Ok(Some(DocumentSymbolNode {
        name: name.to_string(),
        kind: kind.to_string(),
        path: path.to_path_buf(),
        line,
        column: Some(column),
        end_line,
        end_column: Some(end_column),
        children,
    }))
}

fn symbol_information_node(value: &Value, location: &Value) -> Result<Option<DocumentSymbolNode>> {
    let name = value.get("name").and_then(Value::as_str).unwrap_or("symbol");
    let kind = value
        .get("kind")
        .and_then(Value::as_u64)
        .map(symbol_kind_label)
        .unwrap_or("symbol");
    let Some(uri) = location.get("uri").and_then(Value::as_str) else {
        return Ok(None);
    };
    let path = uri_to_path(uri)?;
    let range = location.get("range").unwrap_or(&Value::Null);
    let ((line, column), (end_line, end_column)) = edit_range_position(range);
    Ok(Some(DocumentSymbolNode {
        name: name.to_string(),
        kind: kind.to_string(),
        path,
        line,
        column: Some(column),
        end_line,
        end_column: Some(end_column),
        children: Vec::new(),
    }))
}

fn breadcrumbs_from_outline(outline: &[DocumentSymbolNode], line: usize, column: usize) -> Vec<DocumentSymbolNode> {
    for node in outline {
        if !node_contains(node, line, column) {
            continue;
        }
        let mut path = vec![node.clone()];
        path.extend(breadcrumbs_from_outline(&node.children, line, column));
        return path;
    }
    Vec::new()
}

fn node_contains(node: &DocumentSymbolNode, line: usize, column: usize) -> bool {
    let start_column = node.column.unwrap_or(1);
    let end_column = node.end_column.unwrap_or(usize::MAX);
    if line < node.line || line > node.end_line {
        return false;
    }
    if line == node.line && column < start_column {
        return false;
    }
    if line == node.end_line && column > end_column {
        return false;
    }
    true
}

pub fn format_outline_text(nodes: &[DocumentSymbolNode]) -> String {
    let mut output = String::new();
    for node in nodes {
        append_outline_node(&mut output, node, 0);
    }
    if output.is_empty() {
        output.push_str("No document outline\n");
    }
    output
}

fn append_outline_node(output: &mut String, node: &DocumentSymbolNode, depth: usize) {
    let indent = "  ".repeat(depth);
    output.push_str(&format!(
        "{}- {} [{}] {}:{}\n",
        indent,
        node.name,
        node.kind,
        node.path.display(),
        node.line
    ));
    for child in &node.children {
        append_outline_node(output, child, depth + 1);
    }
}

pub fn format_breadcrumbs_text(nodes: &[DocumentSymbolNode]) -> String {
    if nodes.is_empty() {
        return "No breadcrumbs\n".to_string();
    }
    let names = nodes
        .iter()
        .map(|node| format!("{} [{}]", node.name, node.kind))
        .collect::<Vec<String>>();
    format!("{}\n", names.join(" > "))
}

fn semantic_tokens_from_value(value: &Value) -> Result<Vec<SemanticToken>> {
    let Some(data) = value.get("data").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    if data.len() % 5 != 0 {
        return Err(AppError::General("Invalid semantic token data length".to_string()));
    }

    let mut line = 0usize;
    let mut start = 0usize;
    let mut tokens = Vec::new();
    for chunk in data.chunks(5) {
        let delta_line = chunk[0].as_u64().unwrap_or(0) as usize;
        let delta_start = chunk[1].as_u64().unwrap_or(0) as usize;
        let length = chunk[2].as_u64().unwrap_or(0) as usize;
        let token_type = chunk[3].as_u64().unwrap_or(0) as usize;
        let modifier_bits = chunk[4].as_u64().unwrap_or(0);
        line += delta_line;
        if delta_line == 0 {
            start += delta_start;
        } else {
            start = delta_start;
        }
        tokens.push(SemanticToken {
            line: line + 1,
            start_column: start + 1,
            length,
            token_type: SEMANTIC_TOKEN_TYPES
                .get(token_type)
                .copied()
                .unwrap_or("unknown")
                .to_string(),
            modifiers: token_modifiers(modifier_bits),
        });
    }
    Ok(tokens)
}

pub fn format_semantic_tokens_text(tokens: &[SemanticToken]) -> String {
    if tokens.is_empty() {
        return "No semantic tokens\n".to_string();
    }
    tokens
        .iter()
        .map(|token| {
            let modifiers = if token.modifiers.is_empty() {
                "-".to_string()
            } else {
                token.modifiers.join("|")
            };
            format!(
                "{}:{} len={} type={} modifiers={}",
                token.line, token.start_column, token.length, token.token_type, modifiers
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
        + "\n"
}

fn token_modifiers(bits: u64) -> Vec<String> {
    SEMANTIC_TOKEN_MODIFIERS
        .iter()
        .enumerate()
        .filter(|(index, _)| bits & (1_u64 << index) != 0)
        .map(|(_, value)| (*value).to_string())
        .collect()
}

fn collect_document_symbol(path: &Path, value: &Value, parent: Option<&str>, items: &mut Vec<CodeItem>) -> Result<()> {
    if let Some(location) = value.get("location") {
        let name = value.get("name").and_then(Value::as_str).unwrap_or("symbol");
        let kind = value
            .get("kind")
            .and_then(Value::as_u64)
            .map(symbol_kind_label)
            .unwrap_or("symbol");
        let label = scoped_symbol_name(parent, name);
        if let Some(item) = location_to_named_item(location, &label, kind)? {
            items.push(item);
        }
        return Ok(());
    }

    let name = value.get("name").and_then(Value::as_str).unwrap_or("symbol");
    let kind = value
        .get("kind")
        .and_then(Value::as_u64)
        .map(symbol_kind_label)
        .unwrap_or("symbol");
    let range = value
        .get("selectionRange")
        .or_else(|| value.get("range"))
        .unwrap_or(&Value::Null);
    let (line, column) = range_position(range);
    let display_path = path.to_string_lossy().replace('\\', "/");
    let label = scoped_symbol_name(parent, name);
    items.push(CodeItem::symbol(
        path.to_path_buf(),
        display_path,
        line,
        column,
        &label,
        kind,
    ));

    if let Some(children) = value.get("children").and_then(Value::as_array) {
        for child in children {
            collect_document_symbol(path, child, Some(&label), items)?;
        }
    }

    Ok(())
}

fn scoped_symbol_name(parent: Option<&str>, name: &str) -> String {
    match parent {
        Some(parent) => format!("{parent}::{name}"),
        None => name.to_string(),
    }
}

fn call_hierarchy_items(value: &Value, item_field: &str, detail: &str) -> Result<Vec<CodeItem>> {
    let Some(values) = value.as_array() else {
        return Ok(Vec::new());
    };

    let mut items = Vec::new();
    for value in values {
        let call_item = value.get(item_field).unwrap_or(&Value::Null);
        let name = call_item.get("name").and_then(Value::as_str).unwrap_or(detail);
        let Some(uri) = call_item.get("uri").and_then(Value::as_str) else {
            continue;
        };
        let path = uri_to_path(uri)?;
        let range = call_item.get("selectionRange").unwrap_or(&Value::Null);
        let (line, column) = range_position(range);
        let display_path = path.to_string_lossy().replace('\\', "/");
        items.push(CodeItem::symbol(path, display_path, line, column, name, detail));
    }

    Ok(items)
}

fn location_to_named_item(value: &Value, name: &str, kind: &str) -> Result<Option<CodeItem>> {
    let Some(uri) = value
        .get("uri")
        .or_else(|| value.get("targetUri"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let path = uri_to_path(uri)?;
    let range = value
        .get("range")
        .or_else(|| value.get("targetSelectionRange"))
        .unwrap_or(&Value::Null);
    let (line, column) = range_position(range);
    let display_path = path.to_string_lossy().replace('\\', "/");

    Ok(Some(CodeItem::symbol(path, display_path, line, column, name, kind)))
}

fn range_position(range: &Value) -> (usize, Option<usize>) {
    let line = range
        .get("start")
        .and_then(|start| start.get("line"))
        .and_then(Value::as_u64)
        .map(|line| line as usize + 1)
        .unwrap_or(1);
    let column = range
        .get("start")
        .and_then(|start| start.get("character"))
        .and_then(Value::as_u64)
        .map(|column| column as usize + 1);
    (line, column)
}

fn edit_range_position(range: &Value) -> ((usize, usize), (usize, usize)) {
    let start = range.get("start").unwrap_or(&Value::Null);
    let end = range.get("end").unwrap_or(start);
    (lsp_position_to_one_based(start), lsp_position_to_one_based(end))
}

fn lsp_position_to_one_based(position: &Value) -> (usize, usize) {
    let line = position
        .get("line")
        .and_then(Value::as_u64)
        .map(|line| line as usize + 1)
        .unwrap_or(1);
    let column = position
        .get("character")
        .and_then(Value::as_u64)
        .map(|column| column as usize + 1)
        .unwrap_or(1);
    (line, column)
}

fn hover_to_string(value: &Value) -> String {
    let contents = value.get("contents").unwrap_or(value);
    let text = match contents {
        Value::String(text) => text.clone(),
        Value::Object(map) => map
            .get("value")
            .and_then(Value::as_str)
            .or_else(|| map.get("language").and_then(Value::as_str))
            .unwrap_or("No hover text")
            .to_string(),
        Value::Array(values) => values
            .iter()
            .map(hover_to_string)
            .filter(|value| !value.trim().is_empty() && value != "No hover text")
            .collect::<Vec<String>>()
            .join("\n"),
        _ => "No hover text".to_string(),
    };

    if text.trim().is_empty() {
        "No hover text".to_string()
    } else {
        text
    }
}

impl WorkspaceEdit {
    pub fn preview(&self) -> String {
        let mut output = String::from("# LSP Workspace Edit Preview\n\n");
        if self.edits.is_empty() && self.unsupported_operations.is_empty() {
            output.push_str("No edits\n");
            return output;
        }

        let mut grouped = BTreeMap::<PathBuf, Vec<&LspTextEdit>>::new();
        for edit in &self.edits {
            grouped.entry(edit.path.clone()).or_default().push(edit);
        }
        for (path, edits) in grouped {
            output.push_str(&format!("## {}\n", path.display()));
            for edit in edits {
                let new_text = edit.new_text.replace('\n', "\\n");
                output.push_str(&format!(
                    "- {}:{}-{}:{} -> `{}`\n",
                    edit.start_line, edit.start_column, edit.end_line, edit.end_column, new_text
                ));
            }
            output.push('\n');
        }
        if !self.unsupported_operations.is_empty() {
            output.push_str("## Unsupported Operations\n");
            for operation in &self.unsupported_operations {
                output.push_str(&format!("- {operation}\n"));
            }
            output.push('\n');
        }
        output.push_str(&format!("Total edits: {}\n", self.edits.len()));
        output
    }
}

pub fn apply_workspace_edit(edit: &WorkspaceEdit, dry_run: bool) -> Result<WorkspaceEditApplyReport> {
    let preview = edit.preview();
    let mut grouped = BTreeMap::<PathBuf, Vec<LspTextEdit>>::new();
    for text_edit in &edit.edits {
        grouped
            .entry(text_edit.path.clone())
            .or_default()
            .push(text_edit.clone());
    }

    let mut changed_files = Vec::new();
    for (path, edits) in grouped {
        let original = fs::read_to_string(&path)?;
        let updated = apply_text_edits_to_string(&original, &edits, &path)?;
        if updated != original {
            changed_files.push(path.clone());
            if !dry_run {
                fs::write(&path, updated)?;
            }
        }
    }

    Ok(WorkspaceEditApplyReport {
        dry_run,
        edit_count: edit.edits.len(),
        changed_files,
        unsupported_operations: edit.unsupported_operations.clone(),
        preview,
    })
}

fn workspace_edit_from_value(value: &Value) -> Result<WorkspaceEdit> {
    if value.is_null() {
        return Ok(WorkspaceEdit::default());
    }

    let mut edit = WorkspaceEdit::default();
    if let Some(changes) = value.get("changes").and_then(Value::as_object) {
        for (uri, edits) in changes {
            let path = uri_to_path(uri)?;
            edit.edits.extend(parse_text_edits(&path, edits)?);
        }
    }

    if let Some(document_changes) = value.get("documentChanges").and_then(Value::as_array) {
        for change in document_changes {
            let Some(text_document) = change.get("textDocument") else {
                edit.unsupported_operations.push(resource_operation_label(change));
                continue;
            };
            let Some(uri) = text_document.get("uri").and_then(Value::as_str) else {
                continue;
            };
            let path = uri_to_path(uri)?;
            edit.edits
                .extend(parse_text_edits(&path, change.get("edits").unwrap_or(&Value::Null))?);
        }
    }

    Ok(edit)
}

fn parse_text_edits(path: &Path, edits: &Value) -> Result<Vec<LspTextEdit>> {
    let Some(values) = edits.as_array() else {
        return Ok(Vec::new());
    };

    values
        .iter()
        .map(|edit| {
            let range = edit.get("range").unwrap_or(&Value::Null);
            let ((start_line, start_column), (end_line, end_column)) = edit_range_position(range);
            Ok(LspTextEdit {
                path: path.to_path_buf(),
                start_line,
                start_column,
                end_line,
                end_column,
                new_text: edit.get("newText").and_then(Value::as_str).unwrap_or("").to_string(),
            })
        })
        .collect()
}

fn resource_operation_label(operation: &Value) -> String {
    let kind = operation.get("kind").and_then(Value::as_str).unwrap_or("resource");
    let uri = operation
        .get("uri")
        .or_else(|| operation.get("oldUri"))
        .or_else(|| operation.get("newUri"))
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    format!("{kind}: {uri}")
}

#[derive(Debug, Clone)]
struct ResolvedTextEdit {
    start: usize,
    end: usize,
    new_text: String,
}

fn apply_text_edits_to_string(original: &str, edits: &[LspTextEdit], path: &Path) -> Result<String> {
    let mut resolved = edits
        .iter()
        .map(|edit| {
            let start = line_column_to_offset(original, edit.start_line, edit.start_column, path)?;
            let end = line_column_to_offset(original, edit.end_line, edit.end_column, path)?;
            if start > end {
                return Err(AppError::General(format!(
                    "Invalid LSP edit range in {}: start is after end",
                    path.display()
                )));
            }
            Ok(ResolvedTextEdit {
                start,
                end,
                new_text: edit.new_text.clone(),
            })
        })
        .collect::<Result<Vec<ResolvedTextEdit>>>()?;
    resolved.sort_by_key(|edit| (edit.start, edit.end));
    for pair in resolved.windows(2) {
        if pair[0].end > pair[1].start {
            return Err(AppError::General(format!(
                "Overlapping LSP edits are not supported for {}",
                path.display()
            )));
        }
    }

    let mut updated = original.to_string();
    for edit in resolved.into_iter().rev() {
        updated.replace_range(edit.start..edit.end, &edit.new_text);
    }
    Ok(updated)
}

fn line_column_to_offset(text: &str, line: usize, column: usize, path: &Path) -> Result<usize> {
    let mut current_line = 1usize;
    let mut line_start = 0usize;
    for (index, ch) in text.char_indices() {
        if current_line == line {
            return column_to_offset_in_line(text, line_start, column, path);
        }
        if ch == '\n' {
            current_line += 1;
            line_start = index + ch.len_utf8();
        }
    }

    if current_line == line {
        return column_to_offset_in_line(text, line_start, column, path);
    }
    if current_line + 1 == line && column == 1 {
        return Ok(text.len());
    }

    Err(AppError::General(format!(
        "LSP edit line {line} is outside {}",
        path.display()
    )))
}

fn column_to_offset_in_line(text: &str, line_start: usize, column: usize, path: &Path) -> Result<usize> {
    let target = column.saturating_sub(1);
    let line = &text[line_start..];
    for (char_index, (byte_index, ch)) in line.char_indices().enumerate() {
        if ch == '\n' {
            break;
        }
        if char_index == target {
            return Ok(line_start + byte_index);
        }
    }

    let line_end = line
        .find('\n')
        .map(|offset| line_start + offset)
        .unwrap_or_else(|| text.len());
    if line[..line_end.saturating_sub(line_start)].chars().count() == target {
        return Ok(line_end);
    }

    Err(AppError::General(format!(
        "LSP edit column {column} is outside {}",
        path.display()
    )))
}

fn symbol_kind_label(kind: u64) -> &'static str {
    match kind {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum-member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type",
        _ => "symbol",
    }
}

fn highlight_kind_label(kind: u64) -> &'static str {
    match kind {
        2 => "read",
        3 => "write",
        _ => "text",
    }
}

fn severity_label(value: u64) -> &'static str {
    match value {
        1 => "error",
        2 => "warning",
        3 => "info",
        4 => "hint",
        _ => "info",
    }
}

fn text_document(path: &Path) -> Result<Value> {
    Ok(json!({ "uri": path_to_uri(path)? }))
}

fn position(location: &Location) -> Value {
    json!({
        "line": location.line.unwrap_or(1).saturating_sub(1),
        "character": location.column.unwrap_or(1).saturating_sub(1)
    })
}

fn point_range(location: &Location) -> Value {
    let start = position(location);
    let end = start.clone();
    json!({
        "start": start,
        "end": end
    })
}

fn path_to_uri(path: &Path) -> Result<String> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Ok(format!("file://{}", percent_encode_path(&path.to_string_lossy())))
}

fn uri_to_path(uri: &str) -> Result<PathBuf> {
    let path = uri
        .strip_prefix("file://")
        .ok_or_else(|| AppError::General(format!("Unsupported URI: {uri}")))?;
    Ok(PathBuf::from(percent_decode_path(path)?))
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'-' | b'_' | b'~' => {
                encoded.push(*byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode_path(path: &str) -> Result<String> {
    let mut bytes = Vec::new();
    let raw = path.as_bytes();
    let mut index = 0;

    while index < raw.len() {
        if raw[index] == b'%' && index + 2 < raw.len() {
            let hex = std::str::from_utf8(&raw[index + 1..index + 3]).map_err(|e| AppError::General(e.to_string()))?;
            let value = u8::from_str_radix(hex, 16).map_err(|e| AppError::General(e.to_string()))?;
            bytes.push(value);
            index += 3;
        } else {
            bytes.push(raw[index]);
            index += 1;
        }
    }

    String::from_utf8(bytes).map_err(|e| AppError::General(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uri_round_trip_preserves_spaces() {
        let path = PathBuf::from("/tmp/fcs uri test/main.c");
        let uri = format!("file://{}", percent_encode_path(&path.to_string_lossy()));
        let decoded = uri_to_path(&uri).unwrap();

        assert_eq!(decoded, path);
    }

    #[test]
    fn provider_selection_uses_file_language() {
        let config = LspConfig::default();

        let rust_provider = provider_for_path(Path::new("src/main.rs"), &config).unwrap();
        let cpp_provider = provider_for_path(Path::new("src/main.cpp"), &config).unwrap();

        assert_eq!(rust_provider.kind(), LspProviderKind::RustAnalyzer);
        assert_eq!(rust_provider.language_id(Path::new("src/main.rs")), "rust");
        assert_eq!(cpp_provider.kind(), LspProviderKind::Clangd);
        assert_eq!(cpp_provider.language_id(Path::new("src/main.cpp")), "cpp");
    }

    #[test]
    fn provider_selection_rejects_unknown_files() {
        let config = LspConfig::default();

        let result = provider_for_path(Path::new("README.md"), &config);

        assert!(result.is_err());
    }

    #[test]
    fn split_command_preserves_quoted_arguments() {
        let (program, args) = split_command("clangd --query-driver '/opt/toolchains/bin/*' --log=error").unwrap();

        assert_eq!(program, "clangd");
        assert_eq!(
            args,
            vec![
                "--query-driver".to_string(),
                "/opt/toolchains/bin/*".to_string(),
                "--log=error".to_string()
            ]
        );
    }

    #[test]
    fn provider_health_reports_missing_command() {
        let provider = LspProviderSpec::clangd("definitely-missing-fcs-lsp-command");
        let health = provider_health(&provider);

        assert_eq!(health.status, LspProviderHealthStatus::Unavailable);
        assert_eq!(health.kind, LspProviderKind::Clangd);
        assert!(health.message.contains("clangd"));
    }

    #[test]
    fn document_symbols_include_nested_scope() {
        let path = PathBuf::from("/tmp/main.cpp");
        let value = json!([
            {
                "name": "Widget",
                "kind": 5,
                "selectionRange": {
                    "start": { "line": 9, "character": 4 },
                    "end": { "line": 9, "character": 10 }
                },
                "children": [
                    {
                        "name": "run",
                        "kind": 6,
                        "selectionRange": {
                            "start": { "line": 12, "character": 8 },
                            "end": { "line": 12, "character": 11 }
                        }
                    }
                ]
            }
        ]);

        let items = document_symbols_to_items(&path, &value).unwrap();

        assert_eq!(items.len(), 2);
        assert!(items[0].display_text().contains("Widget [class]"));
        assert!(items[1].display_text().contains("Widget::run [method]"));
        assert_eq!(items[1].location.line, Some(13));
    }

    #[test]
    fn workspace_symbols_accept_container_and_uri_only_location() {
        let value = json!([
            {
                "name": "run",
                "containerName": "app",
                "kind": 12,
                "location": {
                    "uri": "file:///tmp/fcs_workspace_symbol.rs"
                }
            },
            {
                "name": "Widget",
                "kind": 23,
                "location": {
                    "targetUri": "file:///tmp/widget.rs",
                    "targetSelectionRange": {
                        "start": { "line": 4, "character": 2 },
                        "end": { "line": 4, "character": 8 }
                    }
                }
            }
        ]);

        let items = workspace_symbols_to_items(&value).unwrap();

        assert_eq!(items.len(), 2);
        assert!(items[0].display_text().contains("app::run [function]"));
        assert_eq!(items[0].location.line, Some(1));
        assert!(items[1].display_text().contains("Widget [struct]"));
        assert_eq!(items[1].location.line, Some(5));
        assert_eq!(items[1].location.column, Some(3));
    }

    #[test]
    fn hover_string_handles_markup_marked_strings_and_empty_values() {
        let value = json!({
            "contents": [
                { "language": "rust", "value": "fn main()" },
                { "kind": "markdown", "value": "**docs**" },
                ""
            ]
        });

        assert_eq!(hover_to_string(&value), "fn main()\n**docs**");
        assert_eq!(hover_to_string(&json!({ "contents": [] })), "No hover text");
    }

    #[test]
    fn call_hierarchy_can_read_outgoing_to_items() {
        let value = json!([
            {
                "to": {
                    "name": "callee",
                    "uri": "file:///tmp/main.cpp",
                    "selectionRange": {
                        "start": { "line": 2, "character": 3 },
                        "end": { "line": 2, "character": 9 }
                    }
                }
            }
        ]);

        let items = call_hierarchy_items(&value, "to", "outgoing").unwrap();

        assert_eq!(items.len(), 1);
        assert!(items[0].display_text().contains("callee [outgoing]"));
        assert_eq!(items[0].location.column, Some(4));
    }

    #[test]
    fn workspace_edit_applies_and_supports_dry_run() {
        let path = std::env::temp_dir().join(format!("fcs_lsp_edit_{}.rs", std::process::id()));
        fs::write(&path, "fn main() {\n    let value = 1;\n}\n").unwrap();
        let edit = WorkspaceEdit {
            edits: vec![LspTextEdit {
                path: path.clone(),
                start_line: 2,
                start_column: 9,
                end_line: 2,
                end_column: 14,
                new_text: "count".to_string(),
            }],
            unsupported_operations: Vec::new(),
        };

        let dry_run = apply_workspace_edit(&edit, true).unwrap();
        assert!(dry_run.dry_run);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "fn main() {\n    let value = 1;\n}\n"
        );

        let applied = apply_workspace_edit(&edit, false).unwrap();
        assert_eq!(applied.changed_files, vec![path.clone()]);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "fn main() {\n    let count = 1;\n}\n"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn workspace_edit_rejects_overlapping_ranges() {
        let path = PathBuf::from("/tmp/fcs_lsp_overlap.rs");
        let edits = vec![
            LspTextEdit {
                path: path.clone(),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 4,
                new_text: "abc".to_string(),
            },
            LspTextEdit {
                path: path.clone(),
                start_line: 1,
                start_column: 3,
                end_line: 1,
                end_column: 5,
                new_text: "de".to_string(),
            },
        ];

        let error = apply_text_edits_to_string("value\n", &edits, &path).unwrap_err();

        assert!(error.to_string().contains("Overlapping LSP edits"));
    }

    #[test]
    fn outline_and_breadcrumbs_preserve_nested_symbols() {
        let path = PathBuf::from("/tmp/main.rs");
        let value = json!([
            {
                "name": "App",
                "kind": 5,
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 20, "character": 0 }
                },
                "children": [
                    {
                        "name": "run",
                        "kind": 6,
                        "range": {
                            "start": { "line": 4, "character": 4 },
                            "end": { "line": 9, "character": 5 }
                        }
                    }
                ]
            }
        ]);

        let outline = document_outline_from_value(&path, &value).unwrap();
        let breadcrumbs = breadcrumbs_from_outline(&outline, 5, 5);
        let formatted = format_breadcrumbs_text(&breadcrumbs);

        assert_eq!(outline[0].name, "App");
        assert_eq!(outline[0].children[0].name, "run");
        assert!(formatted.contains("App [class] > run [method]"));
    }

    #[test]
    fn semantic_tokens_decode_relative_lsp_data() {
        let value = json!({
            "data": [
                0, 0, 4, 12, 3,
                0, 5, 5, 8, 0,
                2, 2, 3, 15, 0
            ]
        });

        let tokens = semantic_tokens_from_value(&value).unwrap();

        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[0].start_column, 1);
        assert_eq!(tokens[0].token_type, "function");
        assert_eq!(tokens[0].modifiers, vec!["declaration", "definition"]);
        assert_eq!(tokens[1].start_column, 6);
        assert_eq!(tokens[1].token_type, "variable");
        assert_eq!(tokens[2].line, 3);
        assert_eq!(tokens[2].token_type, "keyword");
    }
}
