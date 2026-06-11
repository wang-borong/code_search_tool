use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::errors::{AppError, Result};

#[derive(Debug)]
pub(super) enum DapCommand {
    MockSession(crate::dap::DapLaunchProfile),
    StartMock(crate::dap::DapLaunchProfile),
    StartReal {
        spec: crate::dap::DapAdapterProcessSpec,
        profile: crate::dap::DapLaunchProfile,
    },
    SyncBreakpoints(crate::dap::DapLaunchProfile),
    Refresh,
    Continue,
    Pause,
    Next,
    StepIn,
    StepOut,
    SelectThread(u64),
    SelectFrame(usize),
    ExpandVariables(u64),
    VariablesPage {
        start: usize,
        count: usize,
    },
    Evaluate(String),
    AddWatch(String),
    RemoveWatch(usize),
    ClearWatches,
    RefreshWatches,
    Restart,
    Terminate,
    Disconnect,
    Stop,
}

#[derive(Debug)]
struct DapRequest {
    id: u64,
    command: DapCommand,
}

#[derive(Debug)]
pub(super) struct DapResponse {
    pub(super) id: u64,
    pub(super) label: &'static str,
    pub(super) result: Result<crate::dap::DapSessionSnapshot>,
}

#[derive(Debug)]
pub(super) struct DapWorker {
    sender: Sender<DapRequest>,
    receiver: Receiver<DapResponse>,
    next_id: u64,
    latest_id: u64,
}

struct DapRuntime {
    client: DapRuntimeClient,
    profile: crate::dap::DapLaunchProfile,
    process_spec: Option<crate::dap::DapAdapterProcessSpec>,
    adapter: String,
    status: String,
    watch_expressions: Vec<String>,
    breakpoint_results: Vec<crate::dap::DapBreakpointResult>,
    capabilities: Vec<String>,
    last_evaluation: Option<String>,
    selected_thread_id: Option<u64>,
    selected_frame_id: Option<u64>,
    selected_variables_reference: Option<u64>,
    variable_page_start: Option<usize>,
    variable_page_count: Option<usize>,
    last_snapshot: crate::dap::DapSessionSnapshot,
}

enum DapRuntimeClient {
    Mock(crate::dap::DapClient<crate::dap::MockDapAdapter>),
    Process(crate::dap::DapClient<crate::dap::DapProcessTransport>),
}

impl DapWorker {
    pub(super) fn start() -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<DapRequest>();
        let (response_sender, response_receiver) = mpsc::channel::<DapResponse>();

        thread::spawn(move || {
            let mut runtime = None;
            while let Ok(request) = request_receiver.recv() {
                let (label, result) = run_dap_request(request.command, &mut runtime);
                if response_sender
                    .send(DapResponse {
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

    pub(super) fn request(&mut self, command: DapCommand) -> Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        self.latest_id = id;
        self.sender
            .send(DapRequest { id, command })
            .map_err(|err| AppError::General(err.to_string()))?;
        Ok(id)
    }

    pub(super) fn try_recv_latest(&mut self) -> Option<DapResponse> {
        let mut latest = None;
        while let Ok(response) = self.receiver.try_recv() {
            if response.id == self.latest_id {
                latest = Some(response);
            }
        }
        latest
    }
}

fn run_dap_request(
    command: DapCommand,
    runtime: &mut Option<DapRuntime>,
) -> (&'static str, Result<crate::dap::DapSessionSnapshot>) {
    match command {
        DapCommand::MockSession(profile) => ("DAP mock session", crate::dap::run_mock_session_snapshot(&profile)),
        DapCommand::StartMock(profile) => ("DAP start", start_mock_session(runtime, profile)),
        DapCommand::StartReal { spec, profile } => ("DAP real start", start_real_session(runtime, spec, profile)),
        DapCommand::SyncBreakpoints(profile) => ("DAP break sync", sync_breakpoints(runtime, profile)),
        DapCommand::Refresh => ("DAP refresh", refresh_runtime(runtime, "refreshed")),
        DapCommand::Continue => ("DAP continue", control_runtime(runtime, "continue")),
        DapCommand::Pause => ("DAP pause", control_runtime(runtime, "pause")),
        DapCommand::Next => ("DAP next", control_runtime(runtime, "next")),
        DapCommand::StepIn => ("DAP step-in", control_runtime(runtime, "step-in")),
        DapCommand::StepOut => ("DAP step-out", control_runtime(runtime, "step-out")),
        DapCommand::SelectThread(thread_id) => ("DAP thread", select_thread(runtime, thread_id)),
        DapCommand::SelectFrame(index) => ("DAP frame", select_frame(runtime, index)),
        DapCommand::ExpandVariables(reference) => ("DAP variable expand", expand_variables(runtime, reference)),
        DapCommand::VariablesPage { start, count } => ("DAP variable page", variables_page(runtime, start, count)),
        DapCommand::Evaluate(expression) => ("DAP evaluate", evaluate_runtime(runtime, expression)),
        DapCommand::AddWatch(expression) => ("DAP watch add", add_watch(runtime, expression)),
        DapCommand::RemoveWatch(index) => ("DAP watch remove", remove_watch(runtime, index)),
        DapCommand::ClearWatches => ("DAP watch clear", clear_watches(runtime)),
        DapCommand::RefreshWatches => ("DAP watch refresh", refresh_runtime(runtime, "watches refreshed")),
        DapCommand::Restart => ("DAP restart", restart_runtime(runtime)),
        DapCommand::Terminate => ("DAP terminate", terminate_runtime(runtime)),
        DapCommand::Disconnect => ("DAP disconnect", disconnect_runtime(runtime)),
        DapCommand::Stop => {
            let result = stop_runtime(runtime);
            ("DAP stop", result)
        }
    }
}

fn start_mock_session(
    runtime: &mut Option<DapRuntime>,
    profile: crate::dap::DapLaunchProfile,
) -> Result<crate::dap::DapSessionSnapshot> {
    let mut client = crate::dap::DapClient::new(crate::dap::MockDapAdapter::default());
    let launch_report = crate::dap::run_launch_session(&mut client, &profile)?;
    let mut session = DapRuntime {
        client: DapRuntimeClient::Mock(client),
        profile,
        process_spec: None,
        adapter: "mock".to_string(),
        status: "stopped".to_string(),
        watch_expressions: Vec::new(),
        breakpoint_results: launch_report.breakpoint_results,
        capabilities: launch_report
            .capabilities
            .as_ref()
            .map(|capabilities| {
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
                lines
            })
            .unwrap_or_default(),
        last_evaluation: None,
        selected_thread_id: None,
        selected_frame_id: None,
        selected_variables_reference: None,
        variable_page_start: None,
        variable_page_count: Some(50),
        last_snapshot: empty_snapshot("mock", "stopped", "mock"),
    };
    let snapshot = session.refresh("stopped")?;
    session.selected_thread_id = snapshot.selected_thread_id;
    *runtime = Some(session);
    Ok(snapshot)
}

fn start_real_session(
    runtime: &mut Option<DapRuntime>,
    spec: crate::dap::DapAdapterProcessSpec,
    profile: crate::dap::DapLaunchProfile,
) -> Result<crate::dap::DapSessionSnapshot> {
    let adapter = adapter_label(&spec);
    let transport = crate::dap::DapProcessTransport::spawn(&spec)?;
    let mut client = crate::dap::DapClient::new(transport);
    let launch_report = crate::dap::run_launch_session(&mut client, &profile)?;
    let mut session = DapRuntime {
        client: DapRuntimeClient::Process(client),
        profile,
        process_spec: Some(spec),
        adapter: adapter.clone(),
        status: "stopped".to_string(),
        watch_expressions: Vec::new(),
        breakpoint_results: launch_report.breakpoint_results,
        capabilities: launch_report
            .capabilities
            .as_ref()
            .map(|capabilities| {
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
                lines
            })
            .unwrap_or_default(),
        last_evaluation: None,
        selected_thread_id: None,
        selected_frame_id: None,
        selected_variables_reference: None,
        variable_page_start: None,
        variable_page_count: Some(50),
        last_snapshot: empty_snapshot(&adapter, "launched", "real"),
    };

    let snapshot = match session.refresh("stopped") {
        Ok(snapshot) => snapshot,
        Err(err) => {
            session.status = "running".to_string();
            let mut snapshot = session.fast_snapshot("running (launch completed; stack pending)");
            let error = err.to_string();
            snapshot.state = crate::dap::DapSessionState::Running;
            snapshot.error = Some(error.clone());
            snapshot.last_error = Some(error);
            session.last_snapshot = snapshot.clone();
            snapshot
        }
    };
    session.selected_thread_id = snapshot.selected_thread_id;
    *runtime = Some(session);
    Ok(snapshot)
}

fn refresh_runtime(runtime: &mut Option<DapRuntime>, status: &str) -> Result<crate::dap::DapSessionSnapshot> {
    active_runtime(runtime)?.refresh(status)
}

fn sync_breakpoints(
    runtime: &mut Option<DapRuntime>,
    profile: crate::dap::DapLaunchProfile,
) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    runtime.profile.breakpoints = profile.breakpoints;
    let responses = runtime.client.set_profile_breakpoints(&runtime.profile)?;
    runtime.breakpoint_results = crate::dap::breakpoint_results_from_responses(&responses);
    if runtime.is_running() {
        return Ok(runtime.fast_snapshot("breakpoints synced (running)"));
    }
    runtime.refresh("breakpoints synced")
}

fn restart_runtime(runtime: &mut Option<DapRuntime>) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime_ref = active_runtime(runtime)?;
    let profile = runtime_ref.profile.clone();
    let process_spec = runtime_ref.process_spec.clone();
    stop_runtime(runtime)?;
    match process_spec {
        Some(spec) => start_real_session(runtime, spec, profile),
        None => start_mock_session(runtime, profile),
    }
}

fn terminate_runtime(runtime: &mut Option<DapRuntime>) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime_ref = active_runtime(runtime)?;
    let response = runtime_ref.client.terminate()?;
    runtime_ref.status = "terminated".to_string();
    let mut snapshot = runtime_ref.fast_snapshot("terminated");
    snapshot.last_event = Some(format!("terminate response {}", response.request_seq));
    *runtime = None;
    Ok(snapshot)
}

fn disconnect_runtime(runtime: &mut Option<DapRuntime>) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime_ref = active_runtime(runtime)?;
    let response = runtime_ref.client.disconnect(true)?;
    runtime_ref.status = "disconnected".to_string();
    let mut snapshot = runtime_ref.fast_snapshot("disconnected");
    snapshot.last_event = Some(format!("disconnect response {}", response.request_seq));
    *runtime = None;
    Ok(snapshot)
}

fn stop_runtime(runtime: &mut Option<DapRuntime>) -> Result<crate::dap::DapSessionSnapshot> {
    if runtime.is_none() {
        return Ok(stopped_snapshot());
    }

    match disconnect_runtime(runtime) {
        Ok(snapshot) => Ok(snapshot),
        Err(err) => {
            *runtime = None;
            let mut snapshot = stopped_snapshot();
            let error = err.to_string();
            snapshot.state = crate::dap::DapSessionState::Errored;
            snapshot.error = Some(error.clone());
            snapshot.last_error = Some(error);
            Ok(snapshot)
        }
    }
}

fn control_runtime(runtime: &mut Option<DapRuntime>, command: &str) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    let thread_id = runtime.selected_thread_id.unwrap_or(1);
    match command {
        "continue" => {
            runtime.client.continue_thread(thread_id)?;
            runtime.status = "running".to_string();
            Ok(runtime.fast_snapshot("running"))
        }
        "pause" => {
            let event_start = runtime.client.event_count();
            runtime.client.pause(thread_id)?;
            runtime.client.wait_for_event_from("stopped", event_start)?;
            runtime.status = "paused".to_string();
            runtime.refresh("paused")
        }
        "next" => {
            let event_start = runtime.client.event_count();
            runtime.client.next(thread_id)?;
            runtime.client.wait_for_event_from("stopped", event_start)?;
            runtime.status = "stopped after next".to_string();
            runtime.refresh("stopped after next")
        }
        "step-in" => {
            let event_start = runtime.client.event_count();
            runtime.client.step_in(thread_id)?;
            runtime.client.wait_for_event_from("stopped", event_start)?;
            runtime.status = "stopped after step-in".to_string();
            runtime.refresh("stopped after step-in")
        }
        "step-out" => {
            let event_start = runtime.client.event_count();
            runtime.client.step_out(thread_id)?;
            runtime.client.wait_for_event_from("stopped", event_start)?;
            runtime.status = "stopped after step-out".to_string();
            runtime.refresh("stopped after step-out")
        }
        other => Err(AppError::General(format!("Unsupported DAP control command: {other}"))),
    }
}

fn select_thread(runtime: &mut Option<DapRuntime>, thread_id: u64) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    if runtime.is_running() {
        return Err(AppError::General(
            "Cannot switch DAP thread while target is running; pause first".to_string(),
        ));
    }
    runtime.selected_thread_id = Some(thread_id);
    runtime.selected_frame_id = None;
    runtime.selected_variables_reference = None;
    runtime.refresh("thread selected")
}

fn select_frame(runtime: &mut Option<DapRuntime>, index: usize) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    if runtime.is_running() {
        return Err(AppError::General(
            "Cannot switch DAP frame while target is running; pause first".to_string(),
        ));
    }
    let snapshot = runtime.refresh("selecting frame")?;
    if index == 0 || index > snapshot.frame_items.len() {
        return Err(AppError::General(format!("Frame index out of range: {index}")));
    }
    runtime.selected_frame_id = Some(snapshot.frame_items[index - 1].id);
    runtime.selected_variables_reference = None;
    runtime.refresh("frame selected")
}

fn expand_variables(
    runtime: &mut Option<DapRuntime>,
    variables_reference: u64,
) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    if runtime.is_running() {
        return Err(AppError::General(
            "Cannot expand DAP variables while target is running; pause first".to_string(),
        ));
    }
    runtime.selected_variables_reference = Some(variables_reference);
    runtime.variable_page_start = Some(0);
    runtime.refresh("variables expanded")
}

fn variables_page(
    runtime: &mut Option<DapRuntime>,
    start: usize,
    count: usize,
) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    if count == 0 {
        return Err(AppError::General(
            "Variable page count must be greater than zero".to_string(),
        ));
    }
    runtime.variable_page_start = Some(start);
    runtime.variable_page_count = Some(count);
    if runtime.is_running() {
        return Ok(runtime.fast_snapshot("variable page selected (running)"));
    }
    runtime.refresh("variable page selected")
}

fn evaluate_runtime(runtime: &mut Option<DapRuntime>, expression: String) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    if runtime.is_running() {
        return Err(AppError::General(
            "Cannot evaluate while the DAP target is running; pause or step to a stopped frame first".to_string(),
        ));
    }
    let frame_id = runtime.refresh("evaluating")?.selected_frame_id;
    let result = runtime.client.evaluate_data(&expression, frame_id, "repl")?;
    runtime.last_evaluation = Some(format!("{} = {}", expression, result.result));
    runtime.refresh("evaluated")
}

fn add_watch(runtime: &mut Option<DapRuntime>, expression: String) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    if !expression.trim().is_empty() && !runtime.watch_expressions.iter().any(|watch| watch == &expression) {
        runtime.watch_expressions.push(expression);
    }
    if runtime.is_running() {
        return Ok(runtime.fast_snapshot("watch added (running)"));
    }
    runtime.refresh("watch added")
}

fn remove_watch(runtime: &mut Option<DapRuntime>, index: usize) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    if index == 0 || index > runtime.watch_expressions.len() {
        return Err(AppError::General(format!("Watch index out of range: {index}")));
    }
    runtime.watch_expressions.remove(index - 1);
    if runtime.is_running() {
        return Ok(runtime.fast_snapshot("watch removed (running)"));
    }
    runtime.refresh("watch removed")
}

fn clear_watches(runtime: &mut Option<DapRuntime>) -> Result<crate::dap::DapSessionSnapshot> {
    let runtime = active_runtime(runtime)?;
    runtime.watch_expressions.clear();
    if runtime.is_running() {
        return Ok(runtime.fast_snapshot("watches cleared (running)"));
    }
    runtime.refresh("watches cleared")
}

fn active_runtime(runtime: &mut Option<DapRuntime>) -> Result<&mut DapRuntime> {
    runtime
        .as_mut()
        .ok_or_else(|| AppError::General("No active DAP session; run `dap start` first".to_string()))
}

impl DapRuntime {
    fn refresh(&mut self, status: &str) -> Result<crate::dap::DapSessionSnapshot> {
        if self.is_running() {
            return Ok(self.fast_snapshot("running (stack refresh skipped)"));
        }

        let snapshot = self.client.refresh_session_snapshot(crate::dap::DapSnapshotRequest {
            profile: &self.profile,
            adapter: &self.adapter,
            status,
            watch_expressions: &self.watch_expressions,
            last_evaluation: self.last_evaluation.clone(),
            breakpoint_results: &self.breakpoint_results,
            selected_thread_id: self.selected_thread_id,
            selected_frame_id: self.selected_frame_id,
            variables_reference: self.selected_variables_reference,
            variables_start: self.variable_page_start,
            variables_count: self.variable_page_count,
            capabilities: &self.capabilities,
        })?;
        self.selected_thread_id = snapshot.selected_thread_id;
        self.selected_frame_id = snapshot.selected_frame_id;
        self.last_snapshot = snapshot.clone();
        Ok(snapshot)
    }

    fn is_running(&self) -> bool {
        self.status == "running"
    }

    fn fast_snapshot(&mut self, status: &str) -> crate::dap::DapSessionSnapshot {
        let mut snapshot = self.last_snapshot.clone();
        snapshot.adapter = self.adapter.clone();
        snapshot.status = status.to_string();
        snapshot.state = dap_state_for_status(status);
        snapshot.profile = self.profile.name.clone();
        snapshot.selected_thread_id = self.selected_thread_id.or(snapshot.selected_thread_id);
        snapshot.selected_frame_id = self.selected_frame_id.or(snapshot.selected_frame_id);
        snapshot.variables_start = self.variable_page_start;
        snapshot.variables_count = self.variable_page_count;
        snapshot.request_count = self.client.request_count();
        snapshot.response_count = self.client.response_count();
        snapshot.commands = self.client.commands();
        snapshot.events = self.client.events();
        snapshot.capabilities = self.capabilities.clone();
        snapshot.last_event = snapshot.events.last().cloned();
        snapshot.last_request = snapshot.commands.last().cloned();
        snapshot.last_error = None;
        if status.starts_with("running") {
            snapshot.stop_reason = None;
            snapshot.stopped_location = None;
        }
        self.last_snapshot = snapshot.clone();
        snapshot
    }
}

impl DapRuntimeClient {
    fn refresh_session_snapshot(
        &mut self,
        request: crate::dap::DapSnapshotRequest<'_>,
    ) -> Result<crate::dap::DapSessionSnapshot> {
        match self {
            Self::Mock(client) => crate::dap::refresh_session_snapshot_with_request(client, request),
            Self::Process(client) => crate::dap::refresh_session_snapshot_with_request(client, request),
        }
    }

    fn set_profile_breakpoints(
        &mut self,
        profile: &crate::dap::DapLaunchProfile,
    ) -> Result<Vec<crate::dap::DapResponse>> {
        match self {
            Self::Mock(client) => client.set_profile_breakpoints(profile),
            Self::Process(client) => client.set_profile_breakpoints(profile),
        }
    }

    fn continue_thread(&mut self, thread_id: u64) -> Result<crate::dap::DapResponse> {
        match self {
            Self::Mock(client) => client.continue_thread(thread_id),
            Self::Process(client) => client.continue_thread(thread_id),
        }
    }

    fn pause(&mut self, thread_id: u64) -> Result<crate::dap::DapResponse> {
        match self {
            Self::Mock(client) => client.pause(thread_id),
            Self::Process(client) => client.pause(thread_id),
        }
    }

    fn next(&mut self, thread_id: u64) -> Result<crate::dap::DapResponse> {
        match self {
            Self::Mock(client) => client.next(thread_id),
            Self::Process(client) => client.next(thread_id),
        }
    }

    fn step_in(&mut self, thread_id: u64) -> Result<crate::dap::DapResponse> {
        match self {
            Self::Mock(client) => client.step_in(thread_id),
            Self::Process(client) => client.step_in(thread_id),
        }
    }

    fn step_out(&mut self, thread_id: u64) -> Result<crate::dap::DapResponse> {
        match self {
            Self::Mock(client) => client.step_out(thread_id),
            Self::Process(client) => client.step_out(thread_id),
        }
    }

    fn evaluate_data(
        &mut self,
        expression: &str,
        frame_id: Option<u64>,
        context: &str,
    ) -> Result<crate::dap::DapEvaluateBody> {
        match self {
            Self::Mock(client) => client.evaluate_data(expression, frame_id, context),
            Self::Process(client) => client.evaluate_data(expression, frame_id, context),
        }
    }

    fn terminate(&mut self) -> Result<crate::dap::DapResponse> {
        match self {
            Self::Mock(client) => client.terminate(),
            Self::Process(client) => client.terminate(),
        }
    }

    fn disconnect(&mut self, terminate_debuggee: bool) -> Result<crate::dap::DapResponse> {
        match self {
            Self::Mock(client) => client.disconnect(terminate_debuggee),
            Self::Process(client) => client.disconnect(terminate_debuggee),
        }
    }

    fn event_count(&self) -> usize {
        match self {
            Self::Mock(client) => client.event_count(),
            Self::Process(client) => client.event_count(),
        }
    }

    fn wait_for_event_from(&mut self, event_name: &str, first_event_index: usize) -> Result<crate::dap::DapEvent> {
        match self {
            Self::Mock(client) => client.wait_for_event_from(event_name, first_event_index),
            Self::Process(client) => client.wait_for_event_from(event_name, first_event_index),
        }
    }

    fn request_count(&self) -> usize {
        match self {
            Self::Mock(client) => client.sent_requests().len(),
            Self::Process(client) => client.sent_requests().len(),
        }
    }

    fn response_count(&self) -> usize {
        match self {
            Self::Mock(client) => client.received_responses().len(),
            Self::Process(client) => client.received_responses().len(),
        }
    }

    fn commands(&self) -> Vec<String> {
        match self {
            Self::Mock(client) => command_names(client.sent_requests()),
            Self::Process(client) => command_names(client.sent_requests()),
        }
    }

    fn events(&self) -> Vec<String> {
        match self {
            Self::Mock(client) => event_names(client.events()),
            Self::Process(client) => event_names(client.events()),
        }
    }
}

fn stopped_snapshot() -> crate::dap::DapSessionSnapshot {
    empty_snapshot("mock", "DAP stopped", "none")
}

fn empty_snapshot(adapter: &str, status: &str, profile: &str) -> crate::dap::DapSessionSnapshot {
    crate::dap::DapSessionSnapshot {
        adapter: adapter.to_string(),
        state: crate::dap::DapSessionState::Idle,
        status: status.to_string(),
        profile: profile.to_string(),
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

fn adapter_label(spec: &crate::dap::DapAdapterProcessSpec) -> String {
    if spec.args.is_empty() {
        return spec.command.display().to_string();
    }
    format!("{} {}", spec.command.display(), spec.args.join(" "))
}

fn command_names(requests: &[crate::dap::DapClientRequest]) -> Vec<String> {
    requests.iter().map(|request| request.command.clone()).collect()
}

fn event_names(events: &[crate::dap::DapEvent]) -> Vec<String> {
    events.iter().map(|event| event.event.clone()).collect()
}

fn dap_state_for_status(status: &str) -> crate::dap::DapSessionState {
    let normalized = status.to_ascii_lowercase();
    if normalized.contains("error") || normalized.contains("failed") {
        crate::dap::DapSessionState::Errored
    } else if normalized.contains("disconnect") {
        crate::dap::DapSessionState::Disconnected
    } else if normalized.contains("terminate") {
        crate::dap::DapSessionState::Terminated
    } else if normalized.contains("pause") || normalized.contains("stop") {
        crate::dap::DapSessionState::Stopped
    } else if normalized.contains("running") {
        crate::dap::DapSessionState::Running
    } else {
        crate::dap::DapSessionState::Idle
    }
}
