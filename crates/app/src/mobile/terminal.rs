use super::server::Shared;
use norma::mobile::{TerminalPage, TerminalRequest};
use std::time::{Duration, Instant};

pub struct LiveTerminalRequest {
    pub request: TerminalRequest,
    pub task: Option<LiveTerminalTask>,
    pub control: Option<std::sync::Arc<runtime::TerminalInputLease>>,
    pub deadline: Instant,
    pub reply: crossbeam_channel::Sender<Result<TerminalPage, &'static str>>,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct LiveTerminalTask {
    pub task_ref: String,
    pub agent_definition_id: i64,
    pub key: storage::MobileTurnKey,
}

pub(super) fn read(
    shared: &Shared,
    request: TerminalRequest,
) -> Result<TerminalPage, &'static str> {
    read_task(shared, request, None)
}

pub(super) fn read_task(
    shared: &Shared,
    request: TerminalRequest,
    task: Option<LiveTerminalTask>,
) -> Result<TerminalPage, &'static str> {
    read_controlled(shared, request, task, None)
}

pub(super) fn read_controlled(
    shared: &Shared,
    request: TerminalRequest,
    task: Option<LiveTerminalTask>,
    control: Option<std::sync::Arc<runtime::TerminalInputLease>>,
) -> Result<TerminalPage, &'static str> {
    if !request.valid() {
        return Err("invalid_request");
    }
    let (reply, response) = crossbeam_channel::bounded(1);
    //4s leaves1s for wire delivery within the existing5s read budget.
    let deadline = Instant::now() + Duration::from_secs(4);
    shared
        .terminals
        .as_ref()
        .ok_or("terminal_unavailable")?
        .try_send(LiveTerminalRequest {
            request,
            task,
            control,
            deadline,
            reply,
        })
        .map_err(|_| "unavailable")?;
    response
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "unavailable")?
}
