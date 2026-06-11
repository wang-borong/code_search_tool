use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::Location;
use crate::errors::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapEnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapBreakpoint {
    pub path: PathBuf,
    pub line: usize,
    #[serde(default)]
    pub column: Option<usize>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(rename = "hitCondition", default, skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
    #[serde(rename = "logMessage", default, skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
}

impl DapBreakpoint {
    pub fn from_location(location: &Location) -> Self {
        Self {
            path: location.path.clone(),
            line: location.line.unwrap_or(1),
            column: location.column,
            enabled: true,
            condition: None,
            hit_condition: None,
            log_message: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapLaunchProfile {
    pub name: String,
    pub adapter: String,
    #[serde(default = "default_dap_request")]
    pub request: String,
    pub program: PathBuf,
    #[serde(rename = "processId", default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u64>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<DapEnvVar>,
    #[serde(default)]
    pub breakpoints: Vec<DapBreakpoint>,
    #[serde(default)]
    pub stop_on_entry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapAdapterProcessSpec {
    pub command: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: Vec<DapEnvVar>,
}

impl DapAdapterProcessSpec {
    pub fn new(command: impl Into<PathBuf>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapAdapterDiscovery {
    pub adapter: String,
    pub command: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    pub available: bool,
    pub detail: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl DapAdapterDiscovery {
    pub fn spec(&self, cwd: Option<PathBuf>) -> DapAdapterProcessSpec {
        DapAdapterProcessSpec {
            command: self.command.clone(),
            args: self.args.clone(),
            cwd,
            env: Vec::new(),
        }
    }

    pub fn command_line(&self) -> String {
        let mut parts = vec![self.command.display().to_string()];
        parts.extend(self.args.clone());
        parts.join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapLaunchArguments {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<PathBuf>,
    #[serde(rename = "processId", skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(rename = "stopOnEntry", skip_serializing_if = "Option::is_none")]
    pub stop_on_entry: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DapSessionState {
    #[default]
    Idle,
    Starting,
    Initialized,
    Running,
    Stopped,
    Terminated,
    Disconnected,
    Errored,
}

impl DapSessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Initialized => "initialized",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Terminated => "terminated",
            Self::Disconnected => "disconnected",
            Self::Errored => "errored",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapSource {
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapResponseSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapSourceBreakpoint {
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(rename = "hitCondition", skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
    #[serde(rename = "logMessage", skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapSetBreakpointsArguments {
    pub source: DapSource,
    pub breakpoints: Vec<DapSourceBreakpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapBreakpointResult {
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapSetBreakpointsBody {
    #[serde(default)]
    pub breakpoints: Vec<DapBreakpointResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapInitializeArguments {
    #[serde(rename = "clientID")]
    pub client_id: String,
    #[serde(rename = "adapterID")]
    pub adapter_id: String,
    #[serde(rename = "pathFormat")]
    pub path_format: String,
    #[serde(rename = "linesStartAt1")]
    pub lines_start_at_1: bool,
    #[serde(rename = "columnsStartAt1")]
    pub columns_start_at_1: bool,
    #[serde(rename = "supportsVariableType")]
    pub supports_variable_type: bool,
    #[serde(rename = "supportsVariablePaging")]
    pub supports_variable_paging: bool,
    #[serde(rename = "supportsRunInTerminalRequest")]
    pub supports_run_in_terminal_request: bool,
}

impl DapInitializeArguments {
    pub fn new(adapter_id: &str) -> Self {
        Self {
            client_id: "fcs".to_string(),
            adapter_id: adapter_id.to_string(),
            path_format: "path".to_string(),
            lines_start_at_1: true,
            columns_start_at_1: true,
            supports_variable_type: true,
            supports_variable_paging: true,
            supports_run_in_terminal_request: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapThreadArguments {
    #[serde(rename = "threadId")]
    pub thread_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapStackTraceArguments {
    #[serde(rename = "threadId")]
    pub thread_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapScopesArguments {
    #[serde(rename = "frameId")]
    pub frame_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapVariablesArguments {
    #[serde(rename = "variablesReference")]
    pub variables_reference: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapEvaluateArguments {
    pub expression: String,
    #[serde(rename = "frameId", skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<u64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub context: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapThread {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapThreadsBody {
    #[serde(default)]
    pub threads: Vec<DapThread>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapStackFrame {
    pub id: u64,
    pub name: String,
    pub line: usize,
    pub column: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<DapResponseSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapStackTraceBody {
    #[serde(rename = "stackFrames", default)]
    pub stack_frames: Vec<DapStackFrame>,
    #[serde(rename = "totalFrames", default, skip_serializing_if = "Option::is_none")]
    pub total_frames: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapScope {
    pub name: String,
    #[serde(rename = "variablesReference")]
    pub variables_reference: u64,
    #[serde(default)]
    pub expensive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapScopesBody {
    #[serde(default)]
    pub scopes: Vec<DapScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapVariable {
    pub name: String,
    pub value: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(rename = "variablesReference")]
    pub variables_reference: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapVariablesBody {
    #[serde(default)]
    pub variables: Vec<DapVariable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapEvaluateBody {
    pub result: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(rename = "variablesReference", default)]
    pub variables_reference: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapContinueBody {
    #[serde(rename = "allThreadsContinued", default)]
    pub all_threads_continued: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapDisconnectArguments {
    #[serde(rename = "terminateDebuggee")]
    pub terminate_debuggee: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapCapabilities {
    #[serde(rename = "supportsConfigurationDoneRequest", default)]
    pub supports_configuration_done_request: bool,
    #[serde(rename = "supportsFunctionBreakpoints", default)]
    pub supports_function_breakpoints: bool,
    #[serde(rename = "supportsConditionalBreakpoints", default)]
    pub supports_conditional_breakpoints: bool,
    #[serde(rename = "supportsHitConditionalBreakpoints", default)]
    pub supports_hit_conditional_breakpoints: bool,
    #[serde(rename = "supportsLogPoints", default)]
    pub supports_log_points: bool,
    #[serde(rename = "supportsEvaluateForHovers", default)]
    pub supports_evaluate_for_hovers: bool,
    #[serde(rename = "supportsSetVariable", default)]
    pub supports_set_variable: bool,
    #[serde(rename = "supportsStepBack", default)]
    pub supports_step_back: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapAdapterTemplate {
    pub adapter: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub request: String,
    pub detail: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub launch_fields: Vec<String>,
    #[serde(default)]
    pub attach_fields: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub arguments_preview: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapRequest<T> {
    pub seq: u64,
    #[serde(rename = "type")]
    pub message_type: String,
    pub command: String,
    pub arguments: T,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DapClientRequest {
    pub seq: u64,
    #[serde(rename = "type")]
    pub message_type: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DapResponse {
    pub seq: u64,
    #[serde(rename = "type")]
    pub message_type: String,
    pub request_seq: u64,
    pub success: bool,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DapEvent {
    pub seq: u64,
    #[serde(rename = "type")]
    pub message_type: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DapInboundMessage {
    Response(DapResponse),
    Event(DapEvent),
    Request(DapClientRequest),
}

#[derive(Debug, Default)]
pub struct DapFrameCodec {
    buffer: Vec<u8>,
}

pub trait DapTransport {
    fn write_frame(&mut self, frame: &[u8]) -> Result<()>;
    fn read_frame(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DapClientOptions {
    pub request_timeout: Duration,
    pub event_timeout: Duration,
    pub max_read_frames: usize,
}

impl Default for DapClientOptions {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(DEFAULT_DAP_REQUEST_TIMEOUT_SECS),
            event_timeout: Duration::from_secs(DEFAULT_DAP_EVENT_TIMEOUT_SECS),
            max_read_frames: DAP_MAX_READ_FRAMES,
        }
    }
}

#[derive(Debug)]
pub struct DapClient<T: DapTransport> {
    transport: T,
    next_seq: u64,
    options: DapClientOptions,
    sent_requests: Vec<DapClientRequest>,
    received_responses: Vec<DapResponse>,
    pending_responses: BTreeMap<u64, DapResponse>,
    events: Vec<DapEvent>,
}

pub struct DapProcessTransport {
    child: Child,
    stdin: ChildStdin,
    stdout_rx: Receiver<std::result::Result<Vec<u8>, String>>,
    stderr_rx: Receiver<String>,
}

impl std::fmt::Debug for DapProcessTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DapProcessTransport")
            .field("child_id", &self.child.id())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub struct MockDapAdapter {
    outbound: VecDeque<Vec<u8>>,
    requests: Vec<DapClientRequest>,
    next_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DapSessionSmokeReport {
    pub request_count: usize,
    pub response_count: usize,
    pub commands: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DapLaunchSessionReport {
    pub request_count: usize,
    pub response_count: usize,
    pub breakpoint_response_count: usize,
    pub breakpoint_results: Vec<DapBreakpointResult>,
    pub capabilities: Option<DapCapabilities>,
    pub state: DapSessionState,
    pub last_request: Option<String>,
    pub last_error: Option<String>,
    pub initialized: bool,
    pub launch_completed: bool,
    pub commands: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapSessionSnapshot {
    pub adapter: String,
    #[serde(default)]
    pub state: DapSessionState,
    pub status: String,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_thread_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_frame_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables_reference: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables_start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables_count: Option<usize>,
    pub request_count: usize,
    pub response_count: usize,
    pub commands: Vec<String>,
    pub events: Vec<String>,
    pub threads: Vec<String>,
    pub stack: Vec<String>,
    pub scopes: Vec<String>,
    pub variables: Vec<String>,
    #[serde(default)]
    pub breakpoints: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub thread_items: Vec<DapThread>,
    #[serde(default)]
    pub frame_items: Vec<DapStackFrame>,
    #[serde(default)]
    pub scope_items: Vec<DapScope>,
    #[serde(default)]
    pub variable_items: Vec<DapVariable>,
    #[serde(default)]
    pub watches: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_evaluation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_location: Option<DapStoppedLocation>,
}

#[derive(Debug, Clone)]
pub struct DapSnapshotRequest<'a> {
    pub profile: &'a DapLaunchProfile,
    pub adapter: &'a str,
    pub status: &'a str,
    pub watch_expressions: &'a [String],
    pub last_evaluation: Option<String>,
    pub breakpoint_results: &'a [DapBreakpointResult],
    pub selected_thread_id: Option<u64>,
    pub selected_frame_id: Option<u64>,
    pub variables_reference: Option<u64>,
    pub variables_start: Option<usize>,
    pub variables_count: Option<usize>,
    pub capabilities: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapStoppedLocation {
    pub path: PathBuf,
    pub line: usize,
    pub column: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DapProfileStore {
    profiles: Vec<DapLaunchProfile>,
}

pub fn launch_request(profile: &DapLaunchProfile, seq: u64) -> DapRequest<DapLaunchArguments> {
    DapRequest {
        seq,
        message_type: "request".to_string(),
        command: profile.request.clone(),
        arguments: launch_arguments(profile),
    }
}

pub fn set_breakpoints_requests(
    profile: &DapLaunchProfile,
    first_seq: u64,
) -> Vec<DapRequest<DapSetBreakpointsArguments>> {
    breakpoint_groups(profile)
        .into_iter()
        .enumerate()
        .map(|(offset, (path, breakpoints))| DapRequest {
            seq: first_seq + offset as u64,
            message_type: "request".to_string(),
            command: "setBreakpoints".to_string(),
            arguments: DapSetBreakpointsArguments {
                source: DapSource { path },
                breakpoints,
            },
        })
        .collect()
}

pub fn launch_request_json(profile: &DapLaunchProfile) -> Result<String> {
    json_with_newline(&launch_request(profile, 1))
}

pub fn request_bundle_json(profile: &DapLaunchProfile) -> Result<String> {
    let mut requests = Vec::new();
    for request in set_breakpoints_requests(profile, 1) {
        requests.push(serde_json::to_value(request).map_err(|err| AppError::General(err.to_string()))?);
    }
    let launch_seq = requests.len() as u64 + 1;
    requests.push(
        serde_json::to_value(launch_request(profile, launch_seq)).map_err(|err| AppError::General(err.to_string()))?,
    );
    json_with_newline(&requests)
}

pub fn build_client_request<T: Serialize>(seq: u64, command: &str, arguments: &T) -> Result<DapClientRequest> {
    Ok(DapClientRequest {
        seq,
        message_type: "request".to_string(),
        command: command.to_string(),
        arguments: Some(serde_json::to_value(arguments).map_err(|err| AppError::General(err.to_string()))?),
    })
}

pub fn build_client_request_without_args(seq: u64, command: &str) -> DapClientRequest {
    DapClientRequest {
        seq,
        message_type: "request".to_string(),
        command: command.to_string(),
        arguments: None,
    }
}

pub fn encode_frame<T: Serialize>(message: &T) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(message).map_err(|err| AppError::General(err.to_string()))?;
    let mut frame = format!("Content-Length: {}\r\n\r\n", payload.len()).into_bytes();
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<T> {
    let payload = frame_payload(frame)?;
    serde_json::from_slice(payload).map_err(|err| AppError::General(err.to_string()))
}

pub fn decode_inbound_frame(frame: &[u8]) -> Result<DapInboundMessage> {
    decode_frame(frame)
}

impl DapFrameCodec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        let Some(header_end) = find_header_end(&self.buffer) else {
            return Ok(None);
        };
        let headers = std::str::from_utf8(&self.buffer[..header_end])
            .map_err(|err| AppError::General(format!("Invalid DAP header utf8: {err}")))?;
        let content_length = parse_content_length(headers)?;
        let payload_start = header_end + DAP_HEADER_SEPARATOR.len();
        let frame_len = payload_start + content_length;
        if self.buffer.len() < frame_len {
            return Ok(None);
        }

        Ok(Some(self.buffer.drain(..frame_len).collect()))
    }
}

impl DapProcessTransport {
    pub fn spawn(spec: &DapAdapterProcessSpec) -> Result<Self> {
        let mut command = Command::new(&spec.command);
        command.args(&spec.args);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        for env in &spec.env {
            command.env(&env.name, &env.value);
        }

        let mut child = command.spawn().map_err(|err| {
            AppError::General(format!("Failed to start DAP adapter {}: {err}", spec.command.display()))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::General("DAP adapter stdin was not captured".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::General("DAP adapter stdout was not captured".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::General("DAP adapter stderr was not captured".to_string()))?;
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let (stderr_tx, stderr_rx) = mpsc::channel();

        thread::spawn(move || read_adapter_stdout(stdout, stdout_tx));
        thread::spawn(move || read_adapter_stderr(stderr, stderr_tx));

        Ok(Self {
            child,
            stdin,
            stdout_rx,
            stderr_rx,
        })
    }

    fn adapter_exit_message(&mut self, context: &str) -> Result<Option<String>> {
        let Some(status) = self.child.try_wait()? else {
            return Ok(None);
        };
        let stderr = self.drain_stderr();
        let suffix = if stderr.is_empty() {
            String::new()
        } else {
            format!(" stderr: {stderr}")
        };
        Ok(Some(format!(
            "{context}: DAP adapter exited with status {status}.{suffix}"
        )))
    }

    fn drain_stderr(&mut self) -> String {
        let mut chunks = Vec::new();
        while let Ok(chunk) = self.stderr_rx.try_recv() {
            chunks.push(chunk);
        }
        chunks.join("").trim().to_string()
    }
}

impl DapTransport for DapProcessTransport {
    fn write_frame(&mut self, frame: &[u8]) -> Result<()> {
        if let Some(message) = self.adapter_exit_message("Cannot write DAP frame")? {
            return Err(AppError::General(message));
        }

        self.stdin
            .write_all(frame)
            .map_err(|err| AppError::General(format!("Failed to write DAP frame to adapter stdin: {err}")))?;
        self.stdin
            .flush()
            .map_err(|err| AppError::General(format!("Failed to flush DAP frame to adapter stdin: {err}")))
    }

    fn read_frame(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        match self.stdout_rx.recv_timeout(timeout) {
            Ok(Ok(frame)) => Ok(Some(frame)),
            Ok(Err(message)) => Err(AppError::General(message)),
            Err(RecvTimeoutError::Timeout) => {
                if let Some(message) = self.adapter_exit_message("Cannot read DAP frame")? {
                    return Err(AppError::General(message));
                }
                Ok(None)
            }
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(message) = self.adapter_exit_message("DAP stdout reader stopped")? {
                    return Err(AppError::General(message));
                }
                Err(AppError::General(
                    "DAP stdout reader stopped before a complete frame was read".to_string(),
                ))
            }
        }
    }
}

impl Drop for DapProcessTransport {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

impl<T: DapTransport> DapClient<T> {
    pub fn new(transport: T) -> Self {
        Self::with_options(transport, DapClientOptions::default())
    }

    pub fn with_options(transport: T, options: DapClientOptions) -> Self {
        Self {
            transport,
            next_seq: 1,
            options,
            sent_requests: Vec::new(),
            received_responses: Vec::new(),
            pending_responses: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    pub fn sent_requests(&self) -> &[DapClientRequest] {
        &self.sent_requests
    }

    pub fn events(&self) -> &[DapEvent] {
        &self.events
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn received_responses(&self) -> &[DapResponse] {
        &self.received_responses
    }

    pub fn into_transport(self) -> T {
        self.transport
    }

    pub fn initialize(&mut self, adapter_id: &str) -> Result<DapResponse> {
        self.send_request("initialize", Some(json_value(DapInitializeArguments::new(adapter_id))?))
    }

    pub fn initialize_data(&mut self, adapter_id: &str) -> Result<DapCapabilities> {
        let response = self.initialize(adapter_id)?;
        response
            .body
            .clone()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| AppError::General(format!("Failed to parse DAP capabilities: {err}")))?
            .ok_or_else(|| AppError::General("DAP initialize response did not include capabilities".to_string()))
    }

    pub fn set_breakpoints(&mut self, path: PathBuf, breakpoints: Vec<DapSourceBreakpoint>) -> Result<DapResponse> {
        self.send_request(
            "setBreakpoints",
            Some(json_value(DapSetBreakpointsArguments {
                source: DapSource { path },
                breakpoints,
            })?),
        )
    }

    pub fn set_breakpoints_data(
        &mut self,
        path: PathBuf,
        breakpoints: Vec<DapSourceBreakpoint>,
    ) -> Result<Vec<DapBreakpointResult>> {
        let response = self.set_breakpoints(path, breakpoints)?;
        Ok(response_body_as::<DapSetBreakpointsBody>(&response, "setBreakpoints")?.breakpoints)
    }

    pub fn set_profile_breakpoints(&mut self, profile: &DapLaunchProfile) -> Result<Vec<DapResponse>> {
        let mut responses = Vec::new();
        for (path, breakpoints) in breakpoint_groups(profile) {
            responses.push(self.set_breakpoints(path, breakpoints)?);
        }
        Ok(responses)
    }

    pub fn launch(&mut self, profile: &DapLaunchProfile) -> Result<DapResponse> {
        self.send_request(&profile.request, Some(json_value(launch_arguments(profile))?))
    }

    pub fn send_launch(&mut self, profile: &DapLaunchProfile) -> Result<u64> {
        self.send_request_without_wait(&profile.request, Some(json_value(launch_arguments(profile))?))
    }

    pub fn configuration_done(&mut self) -> Result<DapResponse> {
        self.send_request("configurationDone", None)
    }

    pub fn continue_thread(&mut self, thread_id: u64) -> Result<DapResponse> {
        self.send_thread_request("continue", thread_id)
    }

    pub fn next(&mut self, thread_id: u64) -> Result<DapResponse> {
        self.send_thread_request("next", thread_id)
    }

    pub fn step_in(&mut self, thread_id: u64) -> Result<DapResponse> {
        self.send_thread_request("stepIn", thread_id)
    }

    pub fn step_out(&mut self, thread_id: u64) -> Result<DapResponse> {
        self.send_thread_request("stepOut", thread_id)
    }

    pub fn pause(&mut self, thread_id: u64) -> Result<DapResponse> {
        self.send_thread_request("pause", thread_id)
    }

    pub fn terminate(&mut self) -> Result<DapResponse> {
        self.send_request("terminate", None)
    }

    pub fn disconnect(&mut self, terminate_debuggee: bool) -> Result<DapResponse> {
        self.send_request(
            "disconnect",
            Some(json_value(DapDisconnectArguments { terminate_debuggee })?),
        )
    }

    pub fn threads(&mut self) -> Result<DapResponse> {
        self.send_request("threads", None)
    }

    pub fn threads_data(&mut self) -> Result<Vec<DapThread>> {
        let response = self.threads()?;
        Ok(response_body_as::<DapThreadsBody>(&response, "threads")?.threads)
    }

    pub fn stack_trace(&mut self, thread_id: u64) -> Result<DapResponse> {
        self.send_request("stackTrace", Some(json_value(DapStackTraceArguments { thread_id })?))
    }

    pub fn stack_trace_data(&mut self, thread_id: u64) -> Result<DapStackTraceBody> {
        let response = self.stack_trace(thread_id)?;
        response_body_as(&response, "stackTrace")
    }

    pub fn scopes(&mut self, frame_id: u64) -> Result<DapResponse> {
        self.send_request("scopes", Some(json_value(DapScopesArguments { frame_id })?))
    }

    pub fn scopes_data(&mut self, frame_id: u64) -> Result<Vec<DapScope>> {
        let response = self.scopes(frame_id)?;
        Ok(response_body_as::<DapScopesBody>(&response, "scopes")?.scopes)
    }

    pub fn variables(&mut self, variables_reference: u64) -> Result<DapResponse> {
        self.variables_range(variables_reference, None, None)
    }

    pub fn variables_range(
        &mut self,
        variables_reference: u64,
        start: Option<usize>,
        count: Option<usize>,
    ) -> Result<DapResponse> {
        self.send_request(
            "variables",
            Some(json_value(DapVariablesArguments {
                variables_reference,
                start,
                count,
            })?),
        )
    }

    pub fn variables_data(&mut self, variables_reference: u64) -> Result<Vec<DapVariable>> {
        self.variables_range_data(variables_reference, None, None)
    }

    pub fn variables_range_data(
        &mut self,
        variables_reference: u64,
        start: Option<usize>,
        count: Option<usize>,
    ) -> Result<Vec<DapVariable>> {
        let response = self.variables_range(variables_reference, start, count)?;
        Ok(response_body_as::<DapVariablesBody>(&response, "variables")?.variables)
    }

    pub fn evaluate(&mut self, expression: &str, frame_id: Option<u64>, context: &str) -> Result<DapResponse> {
        self.send_request(
            "evaluate",
            Some(json_value(DapEvaluateArguments {
                expression: expression.to_string(),
                frame_id,
                context: context.to_string(),
            })?),
        )
    }

    pub fn evaluate_data(&mut self, expression: &str, frame_id: Option<u64>, context: &str) -> Result<DapEvaluateBody> {
        let response = self.evaluate(expression, frame_id, context)?;
        response_body_as(&response, "evaluate")
    }

    pub fn wait_for_response_seq(&mut self, request_seq: u64) -> Result<DapResponse> {
        self.wait_for_response(request_seq)
    }

    pub fn wait_for_event(&mut self, event_name: &str) -> Result<DapEvent> {
        self.wait_for_event_from(event_name, 0)
    }

    pub fn wait_for_event_from(&mut self, event_name: &str, first_event_index: usize) -> Result<DapEvent> {
        if let Some(event) = self
            .events
            .iter()
            .skip(first_event_index)
            .find(|event| event.event == event_name)
        {
            return Ok(event.clone());
        }

        let deadline = Instant::now() + self.options.event_timeout;
        for _ in 0..self.options.max_read_frames {
            let Some(message) = self.read_inbound_until(deadline)? else {
                return Err(self.timeout_error("event", event_name));
            };

            match message {
                DapInboundMessage::Response(response) => {
                    self.pending_responses.insert(response.request_seq, response);
                }
                DapInboundMessage::Event(event) => {
                    let matches = event.event == event_name;
                    self.events.push(event.clone());
                    if matches {
                        return Ok(event);
                    }
                }
                DapInboundMessage::Request(request) => return Err(unsupported_reverse_request_error(&request)),
            }
        }

        Err(AppError::General(format!(
            "DAP adapter event wait exceeded frame limit for {event_name}"
        )))
    }

    fn send_thread_request(&mut self, command: &str, thread_id: u64) -> Result<DapResponse> {
        self.send_request(command, Some(json_value(DapThreadArguments { thread_id })?))
    }

    fn send_request(&mut self, command: &str, arguments: Option<Value>) -> Result<DapResponse> {
        let request_seq = self.send_request_without_wait(command, arguments)?;
        self.wait_for_response(request_seq)
    }

    fn send_request_without_wait(&mut self, command: &str, arguments: Option<Value>) -> Result<u64> {
        let request = DapClientRequest {
            seq: self.next_seq,
            message_type: "request".to_string(),
            command: command.to_string(),
            arguments,
        };
        self.next_seq += 1;

        let frame = encode_frame(&request)?;
        self.transport.write_frame(&frame)?;
        let request_seq = request.seq;
        self.sent_requests.push(request.clone());
        Ok(request_seq)
    }

    fn wait_for_response(&mut self, request_seq: u64) -> Result<DapResponse> {
        if let Some(response) = self.pending_responses.remove(&request_seq) {
            return self.complete_response(response);
        }

        let deadline = Instant::now() + self.options.request_timeout;
        for _ in 0..self.options.max_read_frames {
            let Some(message) = self.read_inbound_until(deadline)? else {
                return Err(self.timeout_error("response", &format!("request {request_seq}")));
            };

            match message {
                DapInboundMessage::Response(response) if response.request_seq == request_seq => {
                    return self.complete_response(response);
                }
                DapInboundMessage::Response(response) => {
                    self.pending_responses.insert(response.request_seq, response);
                }
                DapInboundMessage::Event(event) => self.events.push(event),
                DapInboundMessage::Request(request) => return Err(unsupported_reverse_request_error(&request)),
            }
        }

        Err(AppError::General(format!(
            "DAP adapter response wait exceeded frame limit for request {request_seq}"
        )))
    }

    fn read_inbound_until(&mut self, deadline: Instant) -> Result<Option<DapInboundMessage>> {
        let Some(timeout) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(None);
        };
        if timeout.is_zero() {
            return Ok(None);
        }

        let Some(frame) = self.transport.read_frame(timeout)? else {
            return Ok(None);
        };
        decode_inbound_frame(&frame).map(Some)
    }

    fn complete_response(&mut self, response: DapResponse) -> Result<DapResponse> {
        if response.success {
            self.received_responses.push(response.clone());
            return Ok(response);
        }

        Err(AppError::General(format!(
            "DAP request {} failed: {}",
            response.command,
            response.message.unwrap_or_else(|| "unknown error".to_string())
        )))
    }

    fn timeout_error(&self, wait_kind: &str, target: &str) -> AppError {
        AppError::General(format!(
            "Timed out waiting for DAP {wait_kind} {target} after {:?}",
            self.options.request_timeout
        ))
    }
}

impl MockDapAdapter {
    pub fn requests(&self) -> &[DapClientRequest] {
        &self.requests
    }

    fn adapter_seq(&mut self) -> u64 {
        if self.next_seq == 0 {
            self.next_seq = 1;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    fn queue_response(&mut self, request: &DapClientRequest, body: Option<Value>) -> Result<()> {
        let response = DapResponse {
            seq: self.adapter_seq(),
            message_type: "response".to_string(),
            request_seq: request.seq,
            success: true,
            command: request.command.clone(),
            message: None,
            body,
        };
        self.outbound.push_back(encode_frame(&response)?);
        Ok(())
    }

    fn queue_event(&mut self, event: &str, body: Option<Value>) -> Result<()> {
        let event = DapEvent {
            seq: self.adapter_seq(),
            message_type: "event".to_string(),
            event: event.to_string(),
            body,
        };
        self.outbound.push_back(encode_frame(&event)?);
        Ok(())
    }

    fn response_body(request: &DapClientRequest) -> Option<Value> {
        match request.command.as_str() {
            "initialize" => Some(json!({
                "supportsConfigurationDoneRequest": true,
                "supportsFunctionBreakpoints": false,
                "supportsConditionalBreakpoints": true,
                "supportsHitConditionalBreakpoints": true,
                "supportsLogPoints": true,
                "supportsEvaluateForHovers": true,
                "supportsSetVariable": true
            })),
            "setBreakpoints" => Some(json!({
                "breakpoints": mock_breakpoints(request.arguments.as_ref())
            })),
            "continue" => Some(json!({ "allThreadsContinued": false })),
            "terminate" | "disconnect" => Some(json!({})),
            "threads" => Some(json!({
                "threads": [
                    { "id": 1, "name": "main" }
                ]
            })),
            "stackTrace" => Some(json!({
                "stackFrames": [
                    {
                        "id": 1,
                        "name": "main",
                        "line": 1,
                        "column": 1,
                        "source": { "path": "src/main.rs" }
                    }
                ],
                "totalFrames": 1
            })),
            "scopes" => Some(json!({
                "scopes": [
                    {
                        "name": "Locals",
                        "variablesReference": 100,
                        "expensive": false
                    }
                ]
            })),
            "variables" => {
                let start = request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("start"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                let count = request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("count"))
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(usize::MAX);
                let variables = vec![
                    {
                        json!({
                            "name": "argc",
                            "value": "1",
                            "type": "int",
                            "variablesReference": 0
                        })
                    },
                    {
                        json!({
                            "name": "argv",
                            "value": "0x1000",
                            "type": "char **",
                            "variablesReference": 200
                        })
                    },
                ];
                Some(json!({
                    "variables": variables.into_iter().skip(start).take(count).collect::<Vec<Value>>()
                }))
            }
            "evaluate" => {
                let expression = request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("expression"))
                    .and_then(Value::as_str)
                    .unwrap_or("<expr>");
                Some(json!({
                    "result": format!("mock({expression})"),
                    "type": "mock",
                    "variablesReference": 0
                }))
            }
            _ => None,
        }
    }
}

impl DapTransport for MockDapAdapter {
    fn write_frame(&mut self, frame: &[u8]) -> Result<()> {
        let request: DapClientRequest = decode_frame(frame)?;
        if matches!(request.command.as_str(), "next" | "stepIn" | "stepOut" | "pause") {
            self.queue_event(
                "stopped",
                Some(json!({
                    "reason": request.command,
                    "threadId": 1,
                    "allThreadsStopped": true
                })),
            )?;
        }

        let body = Self::response_body(&request);
        self.queue_response(&request, body)?;
        if matches!(request.command.as_str(), "launch" | "attach") {
            self.queue_event("initialized", None)?;
        }
        self.requests.push(request);
        Ok(())
    }

    fn read_frame(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>> {
        Ok(self.outbound.pop_front())
    }
}

pub fn run_launch_session<T: DapTransport>(
    client: &mut DapClient<T>,
    profile: &DapLaunchProfile,
) -> Result<DapLaunchSessionReport> {
    let request_start = client.sent_requests().len();
    let response_start = client.received_responses().len();
    let event_start = client.events().len();

    let initialize_data = client.initialize_data(&profile.adapter)?;
    let supports_configuration_done = initialize_data.supports_configuration_done_request;
    let capabilities = Some(initialize_data);
    let launch_seq = client.send_launch(profile)?;
    if profile.request != "attach" {
        client.wait_for_event_from("initialized", event_start)?;
    }
    let breakpoint_responses = client.set_profile_breakpoints(profile)?;
    let breakpoint_response_count = breakpoint_responses.len();
    let breakpoint_results = breakpoint_results_from_responses(&breakpoint_responses);
    if supports_configuration_done {
        client.configuration_done()?;
    }
    client.wait_for_response_seq(launch_seq)?;

    let commands = client.sent_requests()[request_start..]
        .iter()
        .map(|request| request.command.clone())
        .collect::<Vec<String>>();
    let events = client.events()[event_start..]
        .iter()
        .map(|event| event.event.clone())
        .collect::<Vec<String>>();
    let state = infer_session_state("launch completed", &events, None);

    Ok(DapLaunchSessionReport {
        request_count: commands.len(),
        response_count: client.received_responses().len() - response_start,
        breakpoint_response_count,
        breakpoint_results,
        capabilities,
        state,
        last_request: commands.last().cloned(),
        last_error: None,
        initialized: events.iter().any(|event| event == "initialized"),
        launch_completed: true,
        commands,
        events,
    })
}

pub fn collect_session_snapshot<T: DapTransport>(
    client: &mut DapClient<T>,
    profile: &DapLaunchProfile,
    adapter: &str,
) -> Result<DapSessionSnapshot> {
    let launch_report = run_launch_session(client, profile)?;
    let initial = refresh_session_snapshot_with_breakpoints(
        client,
        profile,
        adapter,
        "launch completed",
        &[],
        None,
        &launch_report.breakpoint_results,
    )?;
    let thread_id = initial.selected_thread_id.unwrap_or(1);

    client.next(thread_id)?;
    client.step_in(thread_id)?;
    client.step_out(thread_id)?;
    client.pause(thread_id)?;
    client.continue_thread(thread_id)?;

    let mut snapshot = refresh_session_snapshot_with_breakpoints(
        client,
        profile,
        adapter,
        "launch completed",
        &[],
        None,
        &launch_report.breakpoint_results,
    )?;
    if !launch_report.launch_completed {
        snapshot.status = "launch incomplete".to_string();
        snapshot.state = DapSessionState::Errored;
    }
    snapshot.capabilities = launch_report
        .capabilities
        .as_ref()
        .map(format_capabilities)
        .unwrap_or_default();
    snapshot.state = launch_report.state;
    snapshot.last_request = launch_report.last_request;
    snapshot.last_error = launch_report.last_error;
    Ok(snapshot)
}

pub fn refresh_session_snapshot<T: DapTransport>(
    client: &mut DapClient<T>,
    profile: &DapLaunchProfile,
    adapter: &str,
    status: &str,
    watch_expressions: &[String],
    last_evaluation: Option<String>,
) -> Result<DapSessionSnapshot> {
    refresh_session_snapshot_with_breakpoints(
        client,
        profile,
        adapter,
        status,
        watch_expressions,
        last_evaluation,
        &[],
    )
}

pub fn refresh_session_snapshot_with_breakpoints<T: DapTransport>(
    client: &mut DapClient<T>,
    profile: &DapLaunchProfile,
    adapter: &str,
    status: &str,
    watch_expressions: &[String],
    last_evaluation: Option<String>,
    breakpoint_results: &[DapBreakpointResult],
) -> Result<DapSessionSnapshot> {
    refresh_session_snapshot_with_request(
        client,
        DapSnapshotRequest {
            profile,
            adapter,
            status,
            watch_expressions,
            last_evaluation,
            breakpoint_results,
            selected_thread_id: None,
            selected_frame_id: None,
            variables_reference: None,
            variables_start: None,
            variables_count: None,
            capabilities: &[],
        },
    )
}

pub fn refresh_session_snapshot_with_request<T: DapTransport>(
    client: &mut DapClient<T>,
    request: DapSnapshotRequest<'_>,
) -> Result<DapSessionSnapshot> {
    let threads = client.threads_data()?;
    let thread_id = request
        .selected_thread_id
        .filter(|selected| threads.iter().any(|thread| thread.id == *selected))
        .or_else(|| threads.first().map(|thread| thread.id))
        .unwrap_or(1);
    let stack_trace = client.stack_trace_data(thread_id)?;
    let frame_id = request
        .selected_frame_id
        .filter(|selected| stack_trace.stack_frames.iter().any(|frame| frame.id == *selected))
        .or_else(|| stack_trace.stack_frames.first().map(|frame| frame.id))
        .unwrap_or(1);
    let scopes = client.scopes_data(frame_id)?;
    let variables_reference = request
        .variables_reference
        .or_else(|| scopes.first().map(|scope| scope.variables_reference))
        .unwrap_or(100);
    let variables =
        client.variables_range_data(variables_reference, request.variables_start, request.variables_count)?;
    let mut watches = Vec::new();
    for expression in request.watch_expressions {
        match client.evaluate_data(expression, Some(frame_id), "watch") {
            Ok(result) => watches.push(format_evaluation(expression, &result)),
            Err(err) => watches.push(format!("{expression} ! {err}")),
        }
    }
    let stopped_location = stack_trace.stack_frames.first().and_then(stack_frame_location);
    let events = client
        .events()
        .iter()
        .map(|event| event.event.clone())
        .collect::<Vec<String>>();
    let stop_reason = latest_stopped_reason(client.events());
    let last_event = events.last().cloned();
    let last_request = client.sent_requests().last().map(|request| request.command.clone());
    let state = infer_session_state(request.status, &events, None);

    Ok(DapSessionSnapshot {
        adapter: request.adapter.to_string(),
        state,
        status: request.status.to_string(),
        profile: request.profile.name.clone(),
        selected_thread_id: Some(thread_id),
        selected_frame_id: Some(frame_id),
        variables_reference: Some(variables_reference),
        variables_start: request.variables_start,
        variables_count: request.variables_count,
        request_count: client.sent_requests().len(),
        response_count: client.received_responses().len(),
        commands: client
            .sent_requests()
            .iter()
            .map(|request| request.command.clone())
            .collect(),
        events,
        threads: threads
            .iter()
            .map(|thread| format!("{} {}", thread.id, thread.name))
            .collect(),
        stack: stack_trace.stack_frames.iter().map(format_stack_frame).collect(),
        scopes: scopes
            .iter()
            .map(|scope| format!("{} ref={}", scope.name, scope.variables_reference))
            .collect(),
        variables: variables.iter().map(format_variable).collect(),
        breakpoints: format_breakpoints(request.profile, request.breakpoint_results),
        capabilities: request.capabilities.to_vec(),
        thread_items: threads,
        frame_items: stack_trace.stack_frames,
        scope_items: scopes,
        variable_items: variables,
        watches,
        last_evaluation: request.last_evaluation,
        stop_reason,
        last_event,
        last_request,
        last_error: None,
        error: None,
        stopped_location,
    })
}

pub fn run_mock_session_smoke(profile: &DapLaunchProfile) -> Result<DapSessionSmokeReport> {
    let mut client = DapClient::new(MockDapAdapter::default());

    collect_session_snapshot(&mut client, profile, "mock")?;

    let commands = client
        .sent_requests()
        .iter()
        .map(|request| request.command.clone())
        .collect::<Vec<String>>();
    let events = client
        .events()
        .iter()
        .map(|event| event.event.clone())
        .collect::<Vec<String>>();

    Ok(DapSessionSmokeReport {
        request_count: commands.len(),
        response_count: client.received_responses().len(),
        commands,
        events,
    })
}

pub fn run_mock_session_snapshot(profile: &DapLaunchProfile) -> Result<DapSessionSnapshot> {
    let mut client = DapClient::new(MockDapAdapter::default());
    collect_session_snapshot(&mut client, profile, "mock")
}

pub fn run_adapter_session_snapshot(
    spec: &DapAdapterProcessSpec,
    profile: &DapLaunchProfile,
) -> Result<DapSessionSnapshot> {
    let transport = DapProcessTransport::spawn(spec)?;
    let mut client = DapClient::new(transport);
    collect_session_snapshot(&mut client, profile, &spec.command.to_string_lossy())
}

pub fn discover_adapters() -> Vec<DapAdapterDiscovery> {
    let mut seen = BTreeSet::new();
    let mut adapters = Vec::new();
    for candidate in adapter_candidates() {
        if !seen.insert((candidate.adapter.clone(), candidate.command_line())) {
            continue;
        }
        adapters.push(candidate);
    }
    adapters.sort_by_key(|adapter| (!adapter.available, adapter.adapter.clone(), adapter.command_line()));
    adapters
}

pub fn best_adapter_for_profile(profile: &DapLaunchProfile) -> Option<DapAdapterDiscovery> {
    let discoveries = discover_adapters();
    let adapter = profile.adapter.to_ascii_lowercase();
    discoveries
        .iter()
        .find(|candidate| candidate.available && adapter_matches_profile(&adapter, candidate))
        .cloned()
        .or_else(|| discoveries.into_iter().find(|candidate| candidate.available))
}

pub fn adapter_templates() -> Vec<DapAdapterTemplate> {
    vec![
        DapAdapterTemplate {
            adapter: "codelldb".to_string(),
            command: "codelldb".to_string(),
            args: Vec::new(),
            request: "launch".to_string(),
            detail: "Native C/C++/Rust launch via CodeLLDB".to_string(),
            capabilities: vec![
                "conditional-breakpoints".to_string(),
                "hit-conditions".to_string(),
                "logpoints".to_string(),
                "evaluate".to_string(),
            ],
            launch_fields: common_launch_fields(),
            attach_fields: common_attach_fields(),
            notes: vec![
                "Use `request=attach` with `processId` for an existing native process".to_string(),
                "CodeLLDB may require adapter-specific source map settings outside this generic profile".to_string(),
            ],
            arguments_preview: native_arguments_preview("codelldb"),
        },
        DapAdapterTemplate {
            adapter: "lldb-dap".to_string(),
            command: "lldb-dap".to_string(),
            args: Vec::new(),
            request: "launch".to_string(),
            detail: "LLVM LLDB DAP launch/attach template".to_string(),
            capabilities: vec![
                "conditional-breakpoints".to_string(),
                "hit-conditions".to_string(),
                "logpoints".to_string(),
                "attach".to_string(),
            ],
            launch_fields: common_launch_fields(),
            attach_fields: common_attach_fields(),
            notes: vec![
                "LLDB DAP accepts the generic launch/attach profile used by fcs".to_string(),
                "Attach uses `processId`; launch uses `program`, `cwd`, `args`, and `env`".to_string(),
            ],
            arguments_preview: native_arguments_preview("lldb-dap"),
        },
        DapAdapterTemplate {
            adapter: "cppdbg".to_string(),
            command: "OpenDebugAD7".to_string(),
            args: Vec::new(),
            request: "launch".to_string(),
            detail: "VS Code cpptools OpenDebugAD7 launch template".to_string(),
            capabilities: vec![
                "conditional-breakpoints".to_string(),
                "hit-conditions".to_string(),
                "logpoints".to_string(),
            ],
            launch_fields: common_launch_fields(),
            attach_fields: common_attach_fields(),
            notes: vec![
                "OpenDebugAD7 is discovered from VS Code cpptools installations when available".to_string(),
                "For MI debugger customization, store extra adapter settings in your external launch config".to_string(),
            ],
            arguments_preview: native_arguments_preview("cppdbg"),
        },
        DapAdapterTemplate {
            adapter: "debugpy".to_string(),
            command: "python3".to_string(),
            args: vec!["-m".to_string(), "debugpy.adapter".to_string()],
            request: "launch".to_string(),
            detail: "Python debugpy adapter template".to_string(),
            capabilities: vec![
                "conditional-breakpoints".to_string(),
                "logpoints".to_string(),
                "attach".to_string(),
            ],
            launch_fields: vec![
                "name".to_string(),
                "program".to_string(),
                "cwd".to_string(),
                "args".to_string(),
                "env".to_string(),
                "stopOnEntry".to_string(),
            ],
            attach_fields: vec![
                "name".to_string(),
                "processId".to_string(),
                "connect/listen".to_string(),
            ],
            notes: vec![
                "fcs emits process attach with `processId` when the profile request is `attach`".to_string(),
                "Remote debugpy attach still needs adapter-specific `connect` or `listen` config outside the generic profile"
                    .to_string(),
            ],
            arguments_preview: debugpy_arguments_preview(),
        },
    ]
}

fn common_launch_fields() -> Vec<String> {
    vec![
        "name".to_string(),
        "program".to_string(),
        "cwd".to_string(),
        "args".to_string(),
        "env".to_string(),
        "stopOnEntry".to_string(),
        "breakpoints".to_string(),
    ]
}

fn common_attach_fields() -> Vec<String> {
    vec!["name".to_string(), "processId".to_string(), "breakpoints".to_string()]
}

fn native_arguments_preview(adapter: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "launch".to_string(),
            json!({
                "request": "launch",
                "adapter": adapter,
                "arguments": {
                    "name": "default",
                    "program": "target/debug/app",
                    "cwd": ".",
                    "args": [],
                    "env": {},
                    "stopOnEntry": false
                }
            }),
        ),
        (
            "attach".to_string(),
            json!({
                "request": "attach",
                "adapter": adapter,
                "arguments": {
                    "name": "default",
                    "processId": 12345
                }
            }),
        ),
    ])
}

fn debugpy_arguments_preview() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "launch".to_string(),
            json!({
                "request": "launch",
                "adapter": "debugpy",
                "arguments": {
                    "name": "python",
                    "program": "app.py",
                    "cwd": ".",
                    "args": [],
                    "env": {},
                    "stopOnEntry": false
                }
            }),
        ),
        (
            "attach".to_string(),
            json!({
                "request": "attach",
                "adapter": "debugpy",
                "arguments": {
                    "name": "python",
                    "processId": 12345,
                    "connect": {
                        "host": "127.0.0.1",
                        "port": 5678
                    }
                }
            }),
        ),
    ])
}

pub fn save_profile(root: &Path, profile: DapLaunchProfile) -> Result<()> {
    let path = profile_path(root)?;
    save_profile_to_path(&path, profile)
}

pub fn list_profiles(root: &Path) -> Result<Vec<DapLaunchProfile>> {
    let path = profile_path(root)?;
    list_profiles_from_path(&path)
}

pub fn load_profile(root: &Path, name: &str) -> Result<DapLaunchProfile> {
    let path = profile_path(root)?;
    load_profile_from_path(&path, name)
}

pub fn parse_env_var(value: &str) -> Result<DapEnvVar> {
    let Some((name, raw_value)) = value.split_once('=') else {
        return Err(AppError::General(format!(
            "Invalid DAP environment assignment: {value}"
        )));
    };
    if name.is_empty() {
        return Err(AppError::General("DAP environment variable name is empty".to_string()));
    }

    Ok(DapEnvVar {
        name: name.to_string(),
        value: raw_value.to_string(),
    })
}

fn env_map(env: &[DapEnvVar]) -> BTreeMap<String, String> {
    env.iter()
        .map(|entry| (entry.name.clone(), entry.value.clone()))
        .collect()
}

fn launch_arguments(profile: &DapLaunchProfile) -> DapLaunchArguments {
    if profile.request == "attach" {
        return DapLaunchArguments {
            name: profile.name.clone(),
            program: None,
            process_id: profile.process_id,
            cwd: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            stop_on_entry: None,
        };
    }

    DapLaunchArguments {
        name: profile.name.clone(),
        program: Some(profile.program.clone()),
        process_id: profile.process_id,
        cwd: profile.cwd.clone(),
        args: profile.args.clone(),
        env: env_map(&profile.env),
        stop_on_entry: Some(profile.stop_on_entry),
    }
}

fn breakpoint_groups(profile: &DapLaunchProfile) -> BTreeMap<PathBuf, Vec<DapSourceBreakpoint>> {
    let mut groups: BTreeMap<PathBuf, Vec<DapSourceBreakpoint>> = BTreeMap::new();
    for breakpoint in profile.breakpoints.iter().filter(|breakpoint| breakpoint.enabled) {
        groups
            .entry(breakpoint.path.clone())
            .or_default()
            .push(DapSourceBreakpoint {
                line: breakpoint.line,
                column: breakpoint.column,
                condition: breakpoint.condition.clone(),
                hit_condition: breakpoint.hit_condition.clone(),
                log_message: breakpoint.log_message.clone(),
            });
    }

    for breakpoints in groups.values_mut() {
        breakpoints.sort_by_key(|breakpoint| (breakpoint.line, breakpoint.column.unwrap_or(0)));
    }
    groups
}

fn json_with_newline<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| AppError::General(err.to_string()))
}

fn default_dap_request() -> String {
    "launch".to_string()
}

fn json_value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|err| AppError::General(err.to_string()))
}

fn response_body_as<T: DeserializeOwned>(response: &DapResponse, expected_command: &str) -> Result<T> {
    if response.command != expected_command {
        return Err(AppError::General(format!(
            "Expected DAP response body for {expected_command}, got {}",
            response.command
        )));
    }
    let body = response
        .body
        .clone()
        .ok_or_else(|| AppError::General(format!("DAP response for {expected_command} did not include a body")))?;
    serde_json::from_value(body).map_err(|err| {
        AppError::General(format!(
            "Failed to parse DAP response body for {expected_command}: {err}"
        ))
    })
}

pub fn breakpoint_results_from_responses(responses: &[DapResponse]) -> Vec<DapBreakpointResult> {
    let mut results = Vec::new();
    for response in responses {
        if let Ok(body) = response_body_as::<DapSetBreakpointsBody>(response, "setBreakpoints") {
            results.extend(body.breakpoints);
        }
    }
    results
}

const DAP_HEADER_SEPARATOR: &[u8] = b"\r\n\r\n";
const DEFAULT_DAP_REQUEST_TIMEOUT_SECS: u64 = 10;
const DEFAULT_DAP_EVENT_TIMEOUT_SECS: u64 = 5;
const DAP_MAX_READ_FRAMES: usize = 128;
const DAP_READ_BUFFER_SIZE: usize = 8192;

fn frame_payload(frame: &[u8]) -> Result<&[u8]> {
    let header_end =
        find_header_end(frame).ok_or_else(|| AppError::General("DAP frame missing header separator".to_string()))?;
    let headers = std::str::from_utf8(&frame[..header_end])
        .map_err(|err| AppError::General(format!("Invalid DAP header utf8: {err}")))?;
    let content_length = parse_content_length(headers)?;
    let payload_start = header_end + DAP_HEADER_SEPARATOR.len();
    let payload_end = payload_start + content_length;

    if frame.len() < payload_end {
        return Err(AppError::General(format!(
            "Incomplete DAP frame: expected {content_length} content bytes"
        )));
    }

    Ok(&frame[payload_start..payload_end])
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(DAP_HEADER_SEPARATOR.len())
        .position(|window| window == DAP_HEADER_SEPARATOR)
}

fn parse_content_length(headers: &str) -> Result<usize> {
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            return value
                .trim()
                .parse::<usize>()
                .map_err(|err| AppError::General(format!("Invalid DAP Content-Length: {err}")));
        }
    }

    Err(AppError::General("DAP frame missing Content-Length header".to_string()))
}

fn read_adapter_stdout(mut stdout: std::process::ChildStdout, tx: mpsc::Sender<std::result::Result<Vec<u8>, String>>) {
    let mut codec = DapFrameCodec::new();
    let mut buffer = [0_u8; DAP_READ_BUFFER_SIZE];

    loop {
        match stdout.read(&mut buffer) {
            Ok(0) => {
                let _ = tx.send(Err("DAP adapter stdout closed".to_string()));
                return;
            }
            Ok(read_len) => {
                codec.push(&buffer[..read_len]);
                loop {
                    match codec.next_frame() {
                        Ok(Some(frame)) => {
                            if tx.send(Ok(frame)).is_err() {
                                return;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Err(err.to_string()));
                            return;
                        }
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) => {
                let _ = tx.send(Err(format!("DAP adapter stdout read failed: {err}")));
                return;
            }
        }
    }
}

fn read_adapter_stderr(mut stderr: std::process::ChildStderr, tx: mpsc::Sender<String>) {
    let mut contents = String::new();
    match stderr.read_to_string(&mut contents) {
        Ok(_) if !contents.trim().is_empty() => {
            let _ = tx.send(contents);
        }
        Ok(_) => {}
        Err(err) => {
            let _ = tx.send(format!("Failed to read DAP adapter stderr: {err}"));
        }
    }
}

fn unsupported_reverse_request_error(request: &DapClientRequest) -> AppError {
    AppError::General(format!(
        "DAP adapter sent unsupported reverse request: {}",
        request.command
    ))
}

fn mock_breakpoints(arguments: Option<&Value>) -> Vec<Value> {
    arguments
        .and_then(|value| value.get("breakpoints"))
        .and_then(Value::as_array)
        .map(|breakpoints| {
            breakpoints
                .iter()
                .map(|breakpoint| {
                    let line = breakpoint.get("line").and_then(Value::as_u64).unwrap_or(1);
                    let column = breakpoint.get("column").and_then(Value::as_u64);
                    match column {
                        Some(column) => json!({ "verified": true, "line": line, "column": column }),
                        None => json!({ "verified": true, "line": line }),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn stack_frame_location(frame: &DapStackFrame) -> Option<DapStoppedLocation> {
    let path = frame.source.as_ref()?.path.clone()?;
    Some(DapStoppedLocation {
        path,
        line: frame.line,
        column: Some(frame.column),
    })
}

fn latest_stopped_reason(events: &[DapEvent]) -> Option<String> {
    events
        .iter()
        .rev()
        .find(|event| event.event == "stopped")
        .and_then(|event| event.body.as_ref())
        .and_then(|body| body.get("reason"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn infer_session_state(status: &str, events: &[String], error: Option<&str>) -> DapSessionState {
    if error.is_some() {
        return DapSessionState::Errored;
    }

    let normalized_status = status.to_ascii_lowercase();
    if normalized_status.contains("error") || normalized_status.contains("failed") {
        return DapSessionState::Errored;
    }
    if normalized_status.contains("disconnect") {
        return DapSessionState::Disconnected;
    }
    if normalized_status.contains("terminate") {
        return DapSessionState::Terminated;
    }
    if normalized_status.contains("stop") || normalized_status.contains("pause") {
        return DapSessionState::Stopped;
    }
    if normalized_status.contains("start") || normalized_status.contains("launching") {
        return DapSessionState::Starting;
    }

    for event in events.iter().rev() {
        match event.as_str() {
            "terminated" => return DapSessionState::Terminated,
            "exited" => return DapSessionState::Terminated,
            "disconnect" | "disconnected" => return DapSessionState::Disconnected,
            "stopped" => return DapSessionState::Stopped,
            "continued" | "thread" => return DapSessionState::Running,
            "initialized" => return DapSessionState::Initialized,
            _ => {}
        }
    }

    if normalized_status.contains("launch completed") || normalized_status.contains("running") {
        DapSessionState::Running
    } else {
        DapSessionState::Idle
    }
}

fn format_breakpoints(profile: &DapLaunchProfile, results: &[DapBreakpointResult]) -> Vec<String> {
    profile
        .breakpoints
        .iter()
        .filter(|breakpoint| breakpoint.enabled)
        .enumerate()
        .map(|(index, breakpoint)| {
            let status = results
                .get(index)
                .map(format_breakpoint_result)
                .unwrap_or_else(|| "pending".to_string());
            let column = breakpoint.column.map(|column| format!(":{column}")).unwrap_or_default();
            let mut parts = vec![format!(
                "{}:{}{} {}",
                breakpoint.path.display(),
                breakpoint.line,
                column,
                status
            )];
            if let Some(condition) = breakpoint.condition.as_deref() {
                parts.push(format!("if {condition}"));
            }
            if let Some(hit_condition) = breakpoint.hit_condition.as_deref() {
                parts.push(format!("hit {hit_condition}"));
            }
            if let Some(log_message) = breakpoint.log_message.as_deref() {
                parts.push(format!("log {log_message}"));
            }
            parts.join(" ")
        })
        .collect()
}

fn format_breakpoint_result(result: &DapBreakpointResult) -> String {
    if result.verified {
        return "verified".to_string();
    }
    result
        .message
        .as_deref()
        .filter(|message| !message.trim().is_empty())
        .map(|message| format!("unverified:{message}"))
        .unwrap_or_else(|| "unverified".to_string())
}

fn format_capabilities(capabilities: &DapCapabilities) -> Vec<String> {
    let mut lines = Vec::new();
    if capabilities.supports_configuration_done_request {
        lines.push("configurationDone".to_string());
    }
    if capabilities.supports_conditional_breakpoints {
        lines.push("conditional-breakpoints".to_string());
    }
    if capabilities.supports_hit_conditional_breakpoints {
        lines.push("hit-conditions".to_string());
    }
    if capabilities.supports_log_points {
        lines.push("logpoints".to_string());
    }
    if capabilities.supports_evaluate_for_hovers {
        lines.push("hover-evaluate".to_string());
    }
    if capabilities.supports_set_variable {
        lines.push("set-variable".to_string());
    }
    if capabilities.supports_step_back {
        lines.push("step-back".to_string());
    }
    if lines.is_empty() {
        lines.push("basic".to_string());
    }
    lines
}

fn format_stack_frame(frame: &DapStackFrame) -> String {
    let path = frame
        .source
        .as_ref()
        .and_then(|source| source.path.as_ref())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());

    format!("{} {}:{} {}", frame.id, path, frame.line, frame.name)
}

fn adapter_candidates() -> Vec<DapAdapterDiscovery> {
    let mut candidates = Vec::new();
    for (adapter, command, detail, capabilities) in [
        (
            "codelldb",
            "codelldb",
            "CodeLLDB adapter",
            &["conditional-breakpoints", "hit-conditions", "logpoints", "evaluate"][..],
        ),
        (
            "lldb-dap",
            "lldb-dap",
            "LLVM lldb-dap adapter",
            &["conditional-breakpoints", "hit-conditions", "logpoints", "attach"][..],
        ),
        (
            "lldb-vscode",
            "lldb-vscode",
            "legacy LLDB VS Code adapter",
            &["conditional-breakpoints", "hit-conditions"][..],
        ),
        (
            "cppdbg",
            "OpenDebugAD7",
            "cpptools OpenDebugAD7 adapter",
            &["conditional-breakpoints", "hit-conditions", "logpoints"][..],
        ),
        (
            "js-debug",
            "js-debug-adapter",
            "VS Code JavaScript debug adapter",
            &["conditional-breakpoints", "logpoints"][..],
        ),
        (
            "node",
            "node",
            "Node runtime; use with a JS DAP adapter when configured",
            &["runtime"][..],
        ),
    ] {
        candidates.push(discovery_for_command(adapter, command, &[], detail, capabilities));
    }

    if let Some(python) = command_on_path("python3").or_else(|| command_on_path("python")) {
        candidates.push(DapAdapterDiscovery {
            adapter: "debugpy".to_string(),
            command: python,
            args: vec!["-m".to_string(), "debugpy.adapter".to_string()],
            available: true,
            detail: "Python is available; debugpy module is checked when the adapter starts".to_string(),
            capabilities: vec![
                "conditional-breakpoints".to_string(),
                "logpoints".to_string(),
                "attach".to_string(),
            ],
        });
    } else {
        candidates.push(DapAdapterDiscovery {
            adapter: "debugpy".to_string(),
            command: PathBuf::from("python"),
            args: vec!["-m".to_string(), "debugpy.adapter".to_string()],
            available: false,
            detail: "python was not found on PATH".to_string(),
            capabilities: vec![
                "conditional-breakpoints".to_string(),
                "logpoints".to_string(),
                "attach".to_string(),
            ],
        });
    }
    candidates
}

fn discovery_for_command(
    adapter: &str,
    command: &str,
    args: &[&str],
    detail: &str,
    capabilities: &[&str],
) -> DapAdapterDiscovery {
    let resolved = command_on_path(command);
    let available = resolved.is_some();
    DapAdapterDiscovery {
        adapter: adapter.to_string(),
        command: resolved.unwrap_or_else(|| PathBuf::from(command)),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        available,
        detail: if available {
            detail.to_string()
        } else {
            format!("{command} was not found on PATH")
        },
        capabilities: capabilities
            .iter()
            .map(|capability| (*capability).to_string())
            .collect(),
    }
}

fn adapter_matches_profile(profile_adapter: &str, candidate: &DapAdapterDiscovery) -> bool {
    let candidate_adapter = candidate.adapter.to_ascii_lowercase();
    if profile_adapter == candidate_adapter {
        return true;
    }
    matches!(
        (profile_adapter, candidate_adapter.as_str()),
        ("cppdbg", "codelldb")
            | ("cppdbg", "lldb-dap")
            | ("cppdbg", "lldb-vscode")
            | ("cppdbg", "cppdbg")
            | ("lldb", "codelldb")
            | ("lldb", "lldb-dap")
            | ("python", "debugpy")
            | ("debugpy", "debugpy")
            | ("node", "js-debug")
            | ("node", "node")
    )
}

fn command_on_path(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        return command_path.is_file().then(|| command_path.to_path_buf());
    }

    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = directory.join(format!("{command}.exe"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn format_variable(variable: &DapVariable) -> String {
    let type_name = variable
        .type_name
        .as_deref()
        .map(|value| format!(":{value}"))
        .unwrap_or_default();

    format!("{}{} = {}", variable.name, type_name, variable.value)
}

fn format_evaluation(expression: &str, evaluation: &DapEvaluateBody) -> String {
    let type_name = evaluation
        .type_name
        .as_deref()
        .map(|value| format!(":{value}"))
        .unwrap_or_default();
    format!("{expression}{type_name} = {}", evaluation.result)
}

fn load_store_from_path(path: &Path) -> Result<DapProfileStore> {
    if !path.exists() {
        return Ok(DapProfileStore::default());
    }

    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents).map_err(|err| AppError::General(err.to_string()))
}

fn save_store_to_path(path: &Path, store: &DapProfileStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = toml::to_string_pretty(store).map_err(|err| AppError::General(err.to_string()))?;
    fs::write(path, contents)?;
    Ok(())
}

fn save_profile_to_path(path: &Path, profile: DapLaunchProfile) -> Result<()> {
    let mut store = load_store_from_path(path)?;
    store.profiles.retain(|existing| existing.name != profile.name);
    store.profiles.push(profile);
    store.profiles.sort_by(|left, right| left.name.cmp(&right.name));
    save_store_to_path(path, &store)
}

fn list_profiles_from_path(path: &Path) -> Result<Vec<DapLaunchProfile>> {
    Ok(load_store_from_path(path)?.profiles)
}

fn load_profile_from_path(path: &Path, name: &str) -> Result<DapLaunchProfile> {
    load_store_from_path(path)?
        .profiles
        .into_iter()
        .find(|profile| profile.name == name)
        .ok_or_else(|| AppError::General(format!("DAP profile not found: {name}")))
}

fn profile_path(root: &Path) -> Result<PathBuf> {
    Ok(crate::workspace::cache_dir_for_root(root)?.join("dap_profiles.toml"))
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn temp_profile_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("fcs_dap_{name}_{}", std::process::id()))
            .join("dap_profiles.toml")
    }

    fn profile(name: &str) -> DapLaunchProfile {
        DapLaunchProfile {
            name: name.to_string(),
            adapter: "cppdbg".to_string(),
            request: "launch".to_string(),
            program: PathBuf::from("target/debug/app"),
            process_id: None,
            cwd: Some(PathBuf::from("/tmp/project")),
            args: vec!["--case".to_string(), "42".to_string()],
            env: vec![
                DapEnvVar {
                    name: "RUST_LOG".to_string(),
                    value: "debug".to_string(),
                },
                DapEnvVar {
                    name: "RUST_BACKTRACE".to_string(),
                    value: "1".to_string(),
                },
            ],
            breakpoints: vec![
                DapBreakpoint {
                    path: PathBuf::from("src/main.rs"),
                    line: 12,
                    column: Some(3),
                    enabled: true,
                    condition: Some("argc > 1".to_string()),
                    hit_condition: Some("2".to_string()),
                    log_message: Some("hit main".to_string()),
                },
                DapBreakpoint {
                    path: PathBuf::from("src/main.rs"),
                    line: 4,
                    column: None,
                    enabled: false,
                    condition: None,
                    hit_condition: None,
                    log_message: None,
                },
                DapBreakpoint {
                    path: PathBuf::from("src/lib.rs"),
                    line: 8,
                    column: None,
                    enabled: true,
                    condition: None,
                    hit_condition: None,
                    log_message: None,
                },
            ],
            stop_on_entry: true,
        }
    }

    #[test]
    fn builds_launch_request_json() {
        let json = launch_request_json(&profile("smoke")).unwrap();

        assert!(json.contains("\"command\": \"launch\""));
        assert!(json.contains("\"program\": \"target/debug/app\""));
        assert!(json.contains("\"RUST_LOG\": \"debug\""));
        assert!(json.contains("\"stopOnEntry\": true"));
    }

    #[test]
    fn builds_attach_request_json_with_process_id() {
        let mut profile = profile("attach");
        profile.request = "attach".to_string();
        profile.process_id = Some(4242);
        let json = launch_request_json(&profile).unwrap();

        assert!(json.contains("\"command\": \"attach\""));
        assert!(json.contains("\"processId\": 4242"));
        assert!(!json.contains("\"program\""));
        assert!(!json.contains("\"stopOnEntry\""));
        assert!(!json.contains("\"args\""));
    }

    #[test]
    fn builds_breakpoint_bundle_before_launch() {
        let requests = set_breakpoints_requests(&profile("smoke"), 1);
        let paths: BTreeSet<PathBuf> = requests
            .iter()
            .map(|request| request.arguments.source.path.clone())
            .collect();
        let bundle = request_bundle_json(&profile("smoke")).unwrap();

        assert_eq!(requests.len(), 2);
        assert!(paths.contains(&PathBuf::from("src/main.rs")));
        assert!(paths.contains(&PathBuf::from("src/lib.rs")));
        assert!(!bundle.contains("\"line\": 4"));
        assert!(bundle.contains("\"command\": \"setBreakpoints\""));
        assert!(bundle.contains("\"command\": \"launch\""));
        assert!(bundle.contains("\"condition\": \"argc > 1\""));
        assert!(bundle.contains("\"hitCondition\": \"2\""));
        assert!(bundle.contains("\"logMessage\": \"hit main\""));
    }

    #[test]
    fn persists_profiles_and_replaces_by_name() {
        let path = temp_profile_path("replace");
        let _ = fs::remove_file(&path);

        save_profile_to_path(&path, profile("smoke")).unwrap();
        let mut updated = profile("smoke");
        updated.args = vec!["--new".to_string()];
        save_profile_to_path(&path, updated).unwrap();

        let profiles = list_profiles_from_path(&path).unwrap();
        let loaded = load_profile_from_path(&path, "smoke").unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(loaded.args, vec!["--new".to_string()]);
        assert!(load_profile_from_path(&path, "missing").is_err());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn parses_env_and_location_breakpoints() {
        let env = parse_env_var("KEY=value=with-equals").unwrap();
        let breakpoint = DapBreakpoint::from_location(&Location::new("src/main.rs", Some(9), Some(2)));

        assert_eq!(env.name, "KEY");
        assert_eq!(env.value, "value=with-equals");
        assert!(parse_env_var("missing-equals").is_err());
        assert_eq!(breakpoint.line, 9);
        assert_eq!(breakpoint.column, Some(2));
    }

    #[test]
    fn dap_frame_round_trips_request() {
        let request = build_client_request(7, "threads", &json!({})).unwrap();
        let frame = encode_frame(&request).unwrap();
        let decoded: DapClientRequest = decode_frame(&frame).unwrap();

        assert!(frame.starts_with(b"Content-Length: "));
        assert_eq!(decoded, request);
    }

    #[test]
    fn dap_frame_codec_extracts_multiple_frames() {
        let first = encode_frame(&build_client_request_without_args(1, "threads")).unwrap();
        let second = encode_frame(&build_client_request_without_args(2, "configurationDone")).unwrap();
        let mut codec = DapFrameCodec::new();
        codec.push(&first[..8]);

        assert!(codec.next_frame().unwrap().is_none());

        let mut remaining = Vec::new();
        remaining.extend_from_slice(&first[8..]);
        remaining.extend_from_slice(&second);
        codec.push(&remaining);

        let decoded_first: DapClientRequest = decode_frame(&codec.next_frame().unwrap().unwrap()).unwrap();
        let decoded_second: DapClientRequest = decode_frame(&codec.next_frame().unwrap().unwrap()).unwrap();

        assert_eq!(decoded_first.command, "threads");
        assert_eq!(decoded_second.command, "configurationDone");
        assert!(codec.next_frame().unwrap().is_none());
    }

    #[test]
    fn mock_client_sends_core_debug_requests() {
        let mut client = DapClient::new(MockDapAdapter::default());

        client.initialize("mock").unwrap();
        client
            .set_breakpoints(
                PathBuf::from("src/main.rs"),
                vec![DapSourceBreakpoint {
                    line: 3,
                    column: Some(1),
                    condition: Some("ready".to_string()),
                    hit_condition: Some("3".to_string()),
                    log_message: Some("ready hit".to_string()),
                }],
            )
            .unwrap();
        client.launch(&profile("smoke")).unwrap();
        client.configuration_done().unwrap();
        client.threads().unwrap();
        client.stack_trace(1).unwrap();
        client.scopes(1).unwrap();
        client.variables(100).unwrap();
        client.next(1).unwrap();
        client.step_in(1).unwrap();
        client.step_out(1).unwrap();
        client.pause(1).unwrap();
        client.continue_thread(1).unwrap();

        let commands = client
            .sent_requests()
            .iter()
            .map(|request| request.command.clone())
            .collect::<Vec<String>>();
        let adapter = client.into_transport();

        assert_eq!(
            commands,
            vec![
                "initialize".to_string(),
                "setBreakpoints".to_string(),
                "launch".to_string(),
                "configurationDone".to_string(),
                "threads".to_string(),
                "stackTrace".to_string(),
                "scopes".to_string(),
                "variables".to_string(),
                "next".to_string(),
                "stepIn".to_string(),
                "stepOut".to_string(),
                "pause".to_string(),
                "continue".to_string()
            ]
        );
        assert!(adapter.requests().iter().any(|request| request.command == "variables"));
        assert!(adapter.requests().iter().any(|request| {
            request.command == "setBreakpoints"
                && request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("breakpoints"))
                    .and_then(Value::as_array)
                    .and_then(|breakpoints| breakpoints.first())
                    .is_some_and(|breakpoint| breakpoint.get("condition").and_then(Value::as_str) == Some("ready"))
        }));
    }

    #[test]
    fn mock_client_pages_variables() {
        let mut client = DapClient::new(MockDapAdapter::default());

        let variables = client.variables_range_data(100, Some(1), Some(1)).unwrap();

        assert_eq!(variables.len(), 1);
        assert_eq!(variables[0].name, "argv");
        assert!(client.sent_requests().iter().any(|request| {
            request.command == "variables"
                && request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("start"))
                    .and_then(Value::as_u64)
                    == Some(1)
                && request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("count"))
                    .and_then(Value::as_u64)
                    == Some(1)
        }));
    }

    #[test]
    fn mock_client_evaluates_expressions() {
        let mut client = DapClient::new(MockDapAdapter::default());

        let result = client.evaluate_data("count + 1", Some(1), "watch").unwrap();

        assert_eq!(result.result, "mock(count + 1)");
        assert_eq!(result.variables_reference, 0);
        assert!(client
            .sent_requests()
            .iter()
            .any(|request| request.command == "evaluate"));
    }

    #[test]
    fn mock_session_smoke_covers_launch_and_stepping_flow() {
        let report = run_mock_session_smoke(&profile("smoke")).unwrap();

        for command in [
            "initialize",
            "setBreakpoints",
            "launch",
            "configurationDone",
            "threads",
            "stackTrace",
            "scopes",
            "variables",
            "next",
            "stepIn",
            "stepOut",
            "pause",
            "continue",
        ] {
            assert!(
                report.commands.iter().any(|item| item == command),
                "mock smoke should send {command}"
            );
        }
        assert_eq!(report.request_count, report.response_count);
        assert!(report.events.iter().any(|event| event == "initialized"));
        assert!(report.events.iter().any(|event| event == "stopped"));
    }

    #[test]
    fn launch_session_report_tracks_state_and_last_request() {
        let mut client = DapClient::new(MockDapAdapter::default());
        let report = run_launch_session(&mut client, &profile("report")).unwrap();

        assert_eq!(report.state, DapSessionState::Initialized);
        assert_eq!(report.last_request.as_deref(), Some("configurationDone"));
        assert_eq!(report.last_error, None);
        assert!(report.initialized);
    }

    #[test]
    fn mock_session_smoke_covers_attach_flow() {
        let mut profile = profile("attach");
        profile.request = "attach".to_string();
        profile.process_id = Some(4242);
        let report = run_mock_session_smoke(&profile).unwrap();

        assert!(report.commands.iter().any(|item| item == "attach"));
        assert!(!report.commands.iter().any(|item| item == "launch"));
        assert_eq!(report.request_count, report.response_count);
        assert!(report.events.iter().any(|event| event == "initialized"));
    }

    #[test]
    fn mock_snapshot_includes_typed_debug_items() {
        let snapshot = run_mock_session_snapshot(&profile("typed")).unwrap();

        assert_eq!(snapshot.state, DapSessionState::Initialized);
        assert_eq!(snapshot.thread_items[0].id, 1);
        assert_eq!(snapshot.frame_items[0].name, "main");
        assert_eq!(snapshot.scope_items[0].name, "Locals");
        assert_eq!(snapshot.variable_items[0].name, "argc");
        assert!(snapshot.stop_reason.is_some());
        assert_eq!(snapshot.last_event.as_deref(), Some("stopped"));
        assert_eq!(snapshot.last_request.as_deref(), Some("configurationDone"));
        assert_eq!(snapshot.last_error, None);
        assert!(snapshot
            .breakpoints
            .iter()
            .any(|breakpoint| breakpoint.contains("verified")));
    }

    #[test]
    fn snapshot_request_tracks_variable_paging() {
        let profile = profile("paged");
        let mut client = DapClient::new(MockDapAdapter::default());
        let report = run_launch_session(&mut client, &profile).unwrap();
        let snapshot = refresh_session_snapshot_with_request(
            &mut client,
            DapSnapshotRequest {
                profile: &profile,
                adapter: "mock",
                status: "stopped at breakpoint",
                watch_expressions: &[],
                last_evaluation: None,
                breakpoint_results: &report.breakpoint_results,
                selected_thread_id: Some(1),
                selected_frame_id: Some(1),
                variables_reference: Some(100),
                variables_start: Some(1),
                variables_count: Some(1),
                capabilities: &[],
            },
        )
        .unwrap();

        assert_eq!(snapshot.state, DapSessionState::Stopped);
        assert_eq!(snapshot.variables_reference, Some(100));
        assert_eq!(snapshot.variables_start, Some(1));
        assert_eq!(snapshot.variables_count, Some(1));
        assert_eq!(snapshot.variable_items.len(), 1);
        assert_eq!(snapshot.variable_items[0].name, "argv");
        assert_eq!(snapshot.last_request.as_deref(), Some("variables"));
    }

    #[test]
    fn adapter_discovery_reports_known_candidates() {
        let adapters = discover_adapters();

        assert!(adapters.iter().any(|adapter| adapter.adapter == "codelldb"));
        assert!(adapters.iter().any(|adapter| adapter.adapter == "debugpy"));
        assert!(adapters.iter().any(|adapter| adapter.adapter == "node"));
    }

    #[test]
    fn adapter_templates_include_schema_and_argument_previews() {
        let templates = adapter_templates();
        let codelldb = templates
            .iter()
            .find(|template| template.adapter == "codelldb")
            .unwrap();
        let debugpy = templates.iter().find(|template| template.adapter == "debugpy").unwrap();

        assert!(codelldb.launch_fields.iter().any(|field| field == "program"));
        assert!(codelldb.attach_fields.iter().any(|field| field == "processId"));
        assert!(codelldb.notes.iter().any(|note| note.contains("processId")));
        assert!(codelldb.arguments_preview.contains_key("launch"));
        assert!(codelldb.arguments_preview.contains_key("attach"));
        assert!(debugpy.attach_fields.iter().any(|field| field == "connect/listen"));
        assert!(debugpy
            .arguments_preview
            .get("attach")
            .is_some_and(|preview| preview.to_string().contains("processId")));
    }
}
