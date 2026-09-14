use super::{execution, Session, Shared};
use agent::{AgentInputSource, ToolContext};
use futures::future::BoxFuture;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub(super) type Work = BoxFuture<'static, (String, Vec<Value>)>;

struct Turn {
    id: String,
    input: Mutex<Option<String>>,
    input_ready: Notify,
    cancelled: CancellationToken,
    serial: tokio::sync::Mutex<()>,
}
impl Turn {
    fn new(id: String, session: &Session) -> Self {
        Self {
            id,
            input: Mutex::new(None),
            input_ready: Notify::new(),
            cancelled: session.cancelled.child_token(),
            serial: tokio::sync::Mutex::new(()),
        }
    }
    async fn input(&self) -> String {
        let ready = self.input_ready.notified();
        if let Some(input) = self.input.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return input;
        }
        tokio::select! {
            _ = ready => {},
            _ = self.cancelled.cancelled() => {},
            // Transcription is asynchronous and may trail response.done. Do not infer authorization
            // from tool arguments while waiting; a 3s timeout leaves the operation rejected.
            _ = tokio::time::sleep(Duration::from_secs(3)) => {},
        }
        self.input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_default()
    }
}
struct Pending {
    turn: Arc<Turn>,
    local_calls: HashSet<String>,
    mac_done: bool,
}

pub(super) struct Events {
    session: Arc<Session>,
    shared: Arc<Shared>,
    turn: Arc<Turn>,
    responses: HashMap<String, Arc<Turn>>,
    pending: HashMap<String, Pending>,
    local_outputs: HashSet<String>,
    active_response: String,
    speaking: bool,
}
impl Events {
    pub(super) fn new(shared: Arc<Shared>, session: Arc<Session>) -> Self {
        Self {
            turn: Arc::new(Turn::new(String::new(), &session)),
            session,
            shared,
            responses: HashMap::new(),
            pending: HashMap::new(),
            local_outputs: HashSet::new(),
            active_response: String::new(),
            speaking: false,
        }
    }

    pub(super) fn observe(&mut self, event: &Value) -> Result<Option<Work>, &'static str> {
        match event["type"].as_str() {
            Some("input_audio_buffer.speech_started") => {
                self.turn.cancelled.cancel();
                self.turn = Arc::new(Turn::new(id(event, "item_id")?, &self.session));
                self.speaking = true;
            }
            Some("input_audio_buffer.speech_stopped") => self.speaking = false,
            Some("conversation.item.input_audio_transcription.completed") => {
                if event["item_id"].as_str() == Some(&self.turn.id) {
                    let text = event["transcript"]
                        .as_str()
                        .filter(|text| text.len() <= 32 * 1024)
                        .ok_or("invalid_transcript")?;
                    *self.turn.input.lock().map_err(|_| "unavailable")? = Some(text.into());
                    self.turn.input_ready.notify_waiters();
                }
            }
            Some("response.created") => {
                let response = id(&event["response"], "id")?;
                if self.responses.len() >= 32 {
                    return Err("too_many_responses");
                }
                self.active_response = response.clone();
                self.responses.insert(response, Arc::clone(&self.turn));
            }
            Some("conversation.item.added" | "conversation.item.created")
                if event["item"]["type"] == "function_call_output" =>
            {
                let call = id(&event["item"], "call_id")?;
                if self.local_outputs.len() >= 64 {
                    self.local_outputs.clear();
                }
                self.local_outputs.insert(call.clone());
                for pending in self.pending.values_mut() {
                    pending.local_calls.remove(&call);
                }
            }
            Some("response.done") => return self.completed(&event["response"]),
            _ => {}
        }
        Ok(None)
    }

    fn completed(&mut self, response: &Value) -> Result<Option<Work>, &'static str> {
        let response_id = id(response, "id")?;
        let Some(turn) = self.responses.remove(&response_id) else {
            return Ok(None);
        };
        if response["status"] != "completed" {
            return Ok(None);
        }
        let calls: Vec<Value> = response["output"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["type"] == "function_call")
            .cloned()
            .collect();
        // 20 shared task calls plus at most eight local avatar calls in one model response.
        if calls.len() > agent::MAX_TOOL_CALLS_PER_TURN + 8 || self.pending.len() >= 8 {
            return Err("too_many_tool_calls");
        }
        let mut mac = Vec::new();
        let mut local_calls = HashSet::new();
        for call in calls {
            let call_id = id(&call, "call_id")?;
            if call_id.starts_with("rtc_") {
                return Err("invalid_function_identity");
            }
            if call["name"] == "set_behavior" {
                if !self.local_outputs.contains(&call_id) {
                    local_calls.insert(call_id);
                }
            } else {
                mac.push(call);
            }
        }
        // iOS retains sole ownership of avatar-only continuations. For mixed responses Mac
        // waits for the actual avatar function outputs before creating one continuation.
        if mac.is_empty() {
            return Ok(None);
        }
        self.pending.insert(
            response_id.clone(),
            Pending {
                turn: Arc::clone(&turn),
                local_calls,
                mac_done: false,
            },
        );
        let shared = Arc::clone(&self.shared);
        let session = Arc::clone(&self.session);
        Ok(Some(Box::pin(async move {
            let _serial = turn.serial.lock().await;
            let user_input = turn.input().await;
            let mut output = Vec::new();
            for call in mac {
                let function = call["call_id"].as_str().expect("validated function id");
                let operation = execution::operation_id(&session, function);
                let context = ToolContext {
                    project_id: session.binding.workspace_id.clone(),
                    turn_id: turn.id.clone(),
                    user_input: user_input.clone(),
                    input_source: AgentInputSource::Voice,
                    contextual_delete_targets: vec![],
                    tool_call_id: function.into(),
                    operation_id: Some(operation.clone()),
                    cancellation: turn.cancelled.clone(),
                };
                let mut result = super::policy::execute(
                    Arc::clone(&shared),
                    Arc::clone(&session),
                    call.clone(),
                    context,
                )
                .await;
                result["operation_id"] = json!(operation);
                output.push(json!({"type":"conversation.item.create","item":{"type":"function_call_output","call_id":function,"output":result.to_string()}}));
            }
            (response_id, output)
        })))
    }

    pub(super) fn finished(&mut self, response: &str) {
        if let Some(pending) = self.pending.get_mut(response) {
            pending.mac_done = true;
        }
    }
    pub(super) fn continuations(&mut self) -> Vec<Value> {
        let done: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.mac_done && pending.local_calls.is_empty())
            .map(|(id, _)| id.clone())
            .collect();
        let mut events = Vec::new();
        for id in done {
            let pending = self.pending.remove(&id).expect("pending response exists");
            if !self.speaking
                && id == self.active_response
                && !pending.turn.cancelled.is_cancelled()
            {
                events.push(json!({"type":"response.create"}));
            }
        }
        events
    }
}

fn id(value: &Value, key: &str) -> Result<String, &'static str> {
    value[key]
        .as_str()
        .filter(|id| crate::mobile::command::valid_id(id))
        .map(str::to_owned)
        .ok_or("invalid_sideband_event")
}
