//! Inspect the original connection without creating an extra turn or attachment.
use super::*;

impl Relay {
    pub(super) fn native_frame(&mut self, frame: Message) -> Message {
        let Message::Text(text) = &frame else {
            return frame;
        };
        let Ok(mut value) = serde_json::from_str::<Value>(text) else {
            self.disable("codex_question_invalid_native_json");
            return frame;
        };
        match value["method"].as_str() {
            Some("initialize") if !self.version_verified => {
                if self.initialize.is_some()
                    || (!value["id"].is_string() && !value["id"].is_number())
                {
                    self.disable("codex_question_initialize_invalid");
                } else {
                    self.initialize = Some(value["id"].clone());
                }
            }
            Some("thread/start" | "thread/resume" | "thread/fork") => {
                // Independent of question observation: a dropped injection would downgrade YOLO.
                if self.full_access && value["params"].is_object() {
                    let params = &mut value["params"];
                    params["approvalPolicy"] = "never".into();
                    params["sandbox"] = "danger-full-access".into();
                    params["permissions"] = Value::Null;
                }
                // The native TUI also starts temporary-structured helper threads on this
                // connection. They are not a user attachment and must not revoke/hold its RPC.
                if value["method"] == "thread/start" && value["params"]["ephemeral"] == true {
                    return self.rewritten(frame, value);
                }
                if !self.version_verified {
                    self.disable("codex_question_version_unverified");
                }
                if self.starts.len() >= CAPACITY
                    || (!value["id"].is_string() && !value["id"].is_number())
                {
                    self.disable("codex_question_session_request_overflow");
                    return self.rewritten(frame, value);
                }
                self.starts.push((
                    value["id"].clone(),
                    if value["method"] == "thread/resume" {
                        value["params"]["threadId"].as_str().map(str::to_owned)
                    } else {
                        None
                    },
                ));
                if self.observing.load(Ordering::Acquire)
                    && value["method"] == "thread/start"
                    && value["params"].is_object()
                {
                    value["params"]["experimentalRawEvents"] = true.into();
                    return Message::Text(value.to_string().into());
                }
                return self.rewritten(frame, value);
            }
            _ => {
                if let Some(ledger) = &mut self.ledger {
                    if let Err(error) = ledger.observe_native_response(&value) {
                        self.disable(error);
                    }
                    self.publish_changes();
                }
            }
        }
        frame
    }

    fn rewritten(&self, frame: Message, value: Value) -> Message {
        if self.full_access {
            Message::Text(value.to_string().into())
        } else {
            frame
        }
    }

    pub(super) fn server_frame(&mut self, frame: &Message) {
        let Message::Text(text) = frame else { return };
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            self.disable("codex_question_invalid_server_json");
            return;
        };
        self.inputs.observe(&value);
        if let Some(turn) = self.turns.observe(&value) {
            if self.events.try_send(CodexRelayEvent::Turn(turn)).is_err() {
                self.disable("codex_question_event_gap");
            }
        }
        if let Some(thread_id) = self.inputs.take_used_session() {
            let attachment = CodexAttachment {
                connection_id: format!("codex-relay:{}:{}", self.owner, self.generation),
                thread_id,
                resumed: false,
            };
            if self
                .events
                .try_send(CodexRelayEvent::SessionUsed(attachment))
                .is_err()
            {
                self.disable("codex_question_event_gap");
            }
        }
        if value.get("method").is_none() {
            if self
                .initialize
                .as_ref()
                .is_some_and(|id| *id == value["id"])
            {
                self.initialize = None;
                self.version_verified = value.get("error").is_none()
                    && value["result"]["userAgent"].as_str().is_some_and(|agent| {
                        ["codex-tui/0.162.1 ", "codex-tui/0.166.0 "]
                            .iter()
                            .any(|version| agent.starts_with(version))
                    });
                if !self.version_verified {
                    self.disable("codex_question_version_unsupported");
                }
            }
            if let Some(index) = self.starts.iter().position(|(id, _)| *id == value["id"]) {
                let (_, expected) = self.starts.remove(index);
                if value.get("error").is_some() {
                    return;
                }
                let Some(thread) = value["result"]["thread"]["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                else {
                    self.disable("codex_question_missing_thread");
                    return;
                };
                let resumed = expected.is_some();
                if expected.is_some_and(|expected| expected != thread) {
                    self.disable("codex_question_wrong_resumed_thread");
                    return;
                }
                self.inputs.attach(thread);
                if let Some(ledger) = &mut self.ledger {
                    ledger.disconnect();
                }
                self.publish_changes();
                let Some(next) = self.generation.checked_add(1) else {
                    self.disable("codex_question_generation_exhausted");
                    return;
                };
                self.generation = next;
                self.turns.attach(CodexAttachment {
                    connection_id: format!("codex-relay:{}:{}", self.owner, self.generation),
                    thread_id: thread.into(),
                    resumed,
                });
                let mut ledger = CodexQuestions::new(thread.into());
                if !self.observing.load(Ordering::Acquire) {
                    ledger.disconnect();
                }
                self.ledger = Some(ledger);
                if self
                    .events
                    .try_send(CodexRelayEvent::Session(CodexAttachment {
                        connection_id: format!("codex-relay:{}:{}", self.owner, self.generation),
                        thread_id: thread.into(),
                        resumed,
                    }))
                    .is_err()
                {
                    self.disable("codex_question_event_gap");
                }
            }
        } else if let Some(ledger) = &mut self.ledger {
            if let Err(error) = ledger.observe(&value) {
                self.disable(error);
            }
            self.publish_changes();
        }
    }
}
