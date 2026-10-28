//! Native RPCs own terminal input even when their tool source cannot authorize an answer.
use serde_json::Value;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

const CAPACITY: usize = 128;
#[derive(Default)]
pub(super) struct NativeInputs {
    requests: Vec<Request>,
    retired: Vec<(String, String)>,
    saturated: bool,
    thread: Option<String>,
    current_turn: Option<(String, bool)>,
    used: bool,
    announced: bool,
    pub(super) blocked: Arc<AtomicBool>,
}
struct Request {
    rpc: Value,
    thread: String,
    turn: String,
    item: String,
    pending: bool,
}

impl NativeInputs {
    pub(super) fn attach(&mut self, thread: &str) {
        if self.thread.as_deref() != Some(thread) {
            self.requests.clear();
            self.retired.clear();
            self.saturated = false;
            self.thread = Some(thread.into());
            self.current_turn = None;
            self.used = false;
            self.announced = false;
            self.publish();
        }
    }

    pub(super) fn observe(&mut self, event: &Value) {
        let params = &event["params"];
        if self
            .thread
            .as_deref()
            .is_none_or(|thread| params["threadId"] != thread)
        {
            return;
        }
        match event["method"].as_str() {
            Some("item/tool/requestUserInput") => {
                let Some(thread) = identity(&params["threadId"]) else {
                    self.saturate();
                    return;
                };
                let Some(turn) = identity(&params["turnId"]) else {
                    self.saturate();
                    return;
                };
                if self
                    .current_turn
                    .as_ref()
                    .is_some_and(|(current, finished)| current != turn || *finished)
                {
                    return;
                }
                let Some(item) = identity(&params["itemId"]) else {
                    self.saturate();
                    return;
                };
                let rpc = &event["id"];
                if !(rpc.is_number() || identity(rpc).is_some()) {
                    self.saturate();
                    return;
                }
                self.used = true;
                if self.retired.iter().any(|(t, r)| t == thread && r == turn) {
                    return;
                }
                if let Some(existing) = self
                    .requests
                    .iter()
                    .find(|r| r.thread == thread && r.rpc == *rpc)
                {
                    if existing.turn != turn || existing.item != item {
                        self.saturate();
                    }
                    return;
                }
                if self.requests.len() >= CAPACITY {
                    self.saturate();
                    return;
                }
                self.requests.push(Request {
                    rpc: rpc.clone(),
                    thread: thread.into(),
                    turn: turn.into(),
                    item: item.into(),
                    pending: true,
                });
            }
            Some("rawResponseItem/completed")
                if params["item"]["type"] == "function_call_output" =>
            {
                for request in &mut self.requests {
                    if params["threadId"] == request.thread
                        && params["turnId"] == request.turn
                        && params["item"]["call_id"] == request.item
                    {
                        request.pending = false;
                    }
                }
            }
            Some("turn/started") if identity(&params["turn"]["id"]).is_some() => {
                self.used = true;
                let turn = params["turn"]["id"]
                    .as_str()
                    .expect("validated turn identity");
                if !self.retired.iter().any(|(_, retired)| retired == turn) {
                    self.current_turn = Some((turn.into(), false));
                }
            }
            Some("turn/completed") => {
                let Some(thread) = identity(&params["threadId"]) else {
                    return;
                };
                let Some(turn) = identity(&params["turn"]["id"]) else {
                    return;
                };
                if let Some((current, finished)) = &mut self.current_turn {
                    if current == turn {
                        *finished = true;
                    }
                }
                if !self.retired.iter().any(|(t, r)| t == thread && r == turn) {
                    // Completed turns are a bounded replay window, never evidence of open input.
                    if self.retired.len() >= CAPACITY {
                        self.retired.remove(0);
                    }
                    self.retired.push((thread.into(), turn.into()));
                }
                // The retired turn rejects replay; completed requests no longer consume
                // capacity reserved for live questions in a long-running native session.
                self.requests
                    .retain(|request| request.thread != thread || request.turn != turn);
            }
            _ => {}
        }
        self.publish();
    }

    pub(super) fn take_used_session(&mut self) -> Option<String> {
        if !self.used || self.announced {
            return None;
        }
        self.announced = true;
        self.thread.clone()
    }

    fn saturate(&mut self) {
        self.saturated = true;
        self.publish();
    }
    fn publish(&self) {
        self.blocked.store(
            self.saturated || self.requests.iter().any(|r| r.pending),
            Ordering::Release,
        );
    }
}

fn identity(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty() && s.len() <= 256)
}

#[cfg(test)]
#[path = "../../../tests/unit/codex_input_tests.rs"]
mod tests;
