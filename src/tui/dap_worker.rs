use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::errors::{AppError, Result};

#[derive(Debug)]
pub(super) enum DapCommand {
    MockSession(crate::dap::DapLaunchProfile),
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

impl DapWorker {
    pub(super) fn start() -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<DapRequest>();
        let (response_sender, response_receiver) = mpsc::channel::<DapResponse>();

        thread::spawn(move || {
            while let Ok(mut request) = request_receiver.recv() {
                while let Ok(newer_request) = request_receiver.try_recv() {
                    request = newer_request;
                }

                let (label, result) = run_dap_request(request.command);
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

fn run_dap_request(command: DapCommand) -> (&'static str, Result<crate::dap::DapSessionSnapshot>) {
    match command {
        DapCommand::MockSession(profile) => ("DAP mock session", crate::dap::run_mock_session_snapshot(&profile)),
    }
}
