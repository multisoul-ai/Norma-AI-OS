//! 一条 ACP 连接上的多会话分发：响应按请求 id，通知和权限按 sessionId。
use super::wire::Incoming;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

pub(crate) struct Switchboard {
    boxes: HashMap<String, Sender<Incoming>>,
    requests: HashMap<u64, String>,
}

impl Switchboard {
    pub(crate) fn new() -> Self {
        Self {
            boxes: HashMap::new(),
            requests: HashMap::new(),
        }
    }

    pub(crate) fn attach(&mut self, session: &str) -> Receiver<Incoming> {
        let (tx, rx) = mpsc::channel();
        self.boxes.insert(session.to_owned(), tx);
        rx
    }

    pub(crate) fn detach(&mut self, session: &str) {
        self.boxes.remove(session);
        self.requests.retain(|_, owner| owner != session);
    }

    pub(crate) fn bind(&mut self, id: u64, session: &str) {
        self.requests.insert(id, session.to_owned());
    }

    /// 送进对应会话的信箱。没有 sessionId、或响应对不上已登记的请求，就丢掉。
    pub(crate) fn deliver(&mut self, incoming: Incoming) -> bool {
        let key = match &incoming {
            Incoming::Response { id, .. } => self.requests.remove(id),
            Incoming::Notification { params, .. } | Incoming::Request { params, .. } => {
                session_id(params)
            }
        };
        let Some(key) = key else {
            return false;
        };
        self.boxes
            .get(&key)
            .is_some_and(|tx| tx.send(incoming).is_ok())
    }
}

fn session_id(params: &Value) -> Option<String> {
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
#[path = "../../tests/unit/acp_route_tests.rs"]
mod tests;
