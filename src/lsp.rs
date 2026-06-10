use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

use crate::config::LspConfig;
use crate::core::{CodeItem, Location};
use crate::errors::{AppError, Result};

pub const RUST_ANALYZER_COMMAND: &str = "rust-analyzer";
const DEFAULT_LSP_RETRIES: usize = 1;

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
        self.open_document(location.path())?;
        let response = self.request_with_retry(
            "textDocument/rename",
            json!({
                "textDocument": text_document(location.path())?,
                "position": position(location),
                "newName": new_name,
            }),
        )?;
        workspace_edit_to_preview(response.get("result").unwrap_or(&Value::Null))
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

fn workspace_edit_to_preview(value: &Value) -> Result<String> {
    if value.is_null() {
        return Ok("No rename edits".to_string());
    }

    let mut output = String::from("# LSP Rename Preview\n\n");
    let mut edit_count = 0usize;

    if let Some(changes) = value.get("changes").and_then(Value::as_object) {
        for (uri, edits) in changes {
            let path = uri_to_path(uri)?;
            edit_count += append_text_edits(&mut output, &path, edits);
        }
    }

    if let Some(document_changes) = value.get("documentChanges").and_then(Value::as_array) {
        for change in document_changes {
            let Some(text_document) = change.get("textDocument") else {
                append_resource_operation(&mut output, change);
                continue;
            };
            let Some(uri) = text_document.get("uri").and_then(Value::as_str) else {
                continue;
            };
            let path = uri_to_path(uri)?;
            edit_count += append_text_edits(&mut output, &path, change.get("edits").unwrap_or(&Value::Null));
        }
    }

    if edit_count == 0 && output.trim() == "# LSP Rename Preview" {
        output.push_str("No rename edits\n");
    } else {
        output.push_str(&format!("Total edits: {edit_count}\n"));
    }

    Ok(output)
}

fn append_text_edits(output: &mut String, path: &Path, edits: &Value) -> usize {
    let Some(values) = edits.as_array() else {
        return 0;
    };

    output.push_str(&format!("## {}\n", path.display()));
    for edit in values {
        let range = edit.get("range").unwrap_or(&Value::Null);
        let (line, column) = range_position(range);
        let new_text = edit
            .get("newText")
            .and_then(Value::as_str)
            .unwrap_or("")
            .replace('\n', "\\n");
        let column = column.map(|value| format!(":{value}")).unwrap_or_default();
        output.push_str(&format!("- {line}{column} -> `{new_text}`\n"));
    }
    output.push('\n');
    values.len()
}

fn append_resource_operation(output: &mut String, operation: &Value) {
    let kind = operation.get("kind").and_then(Value::as_str).unwrap_or("resource");
    let uri = operation
        .get("uri")
        .or_else(|| operation.get("oldUri"))
        .or_else(|| operation.get("newUri"))
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    output.push_str(&format!("## {kind}\n- {uri}\n\n"));
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
}
