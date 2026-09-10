use super::server::Shared;
use norma::mobile::{TerminalPage, TerminalRequest};
use std::time::{Duration, Instant};

pub struct LiveTerminalRequest {
    pub request: TerminalRequest,
    pub deadline: Instant,
    pub reply: crossbeam_channel::Sender<Result<TerminalPage, &'static str>>,
}

pub(super) fn read(
    shared: &Shared,
    request: TerminalRequest,
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
            deadline,
            reply,
        })
        .map_err(|_| "unavailable")?;
    response
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "unavailable")?
}
