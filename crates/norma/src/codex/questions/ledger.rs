//! A bounded, connection-local ledger. No RPC is answerable without its original tool source.
use super::CodexQuestion;
use crate::protocol::{QuestionAnswer, QuestionState};
use serde_json::{json, Value};

const CAPACITY: usize = 128;
const MAX_EVENT_BYTES: usize = 1_048_576;
const MAX_LEDGER_BYTES: usize = 8 * MAX_EVENT_BYTES;

#[derive(Clone, Debug)]
pub struct CodexQuestionChange {
    pub question: CodexQuestion,
    pub state: QuestionState,
    pub evidence: Option<String>,
    pub delivery_id: Option<String>,
}

struct Request {
    raw: Value,
    question: Option<CodexQuestion>,
    state: QuestionState,
    answers: Option<Vec<QuestionAnswer>>,
    delivery_id: Option<String>,
    completed_turn: bool,
}

pub struct CodexQuestions {
    thread_id: String,
    sources: Vec<Value>,
    requests: Vec<Request>,
    changes: Vec<CodexQuestionChange>,
    bytes: usize,
    connected: bool,
    retired_turns: Vec<String>,
    current_turn: Option<String>,
    turn_finished: bool,
}

impl CodexQuestions {
    pub fn new(thread_id: String) -> Self {
        Self {
            thread_id,
            sources: Vec::new(),
            requests: Vec::new(),
            changes: Vec::new(),
            bytes: 0,
            connected: true,
            retired_turns: Vec::new(),
            current_turn: None,
            turn_finished: false,
        }
    }

    pub fn take_changes(&mut self) -> Vec<CodexQuestionChange> {
        std::mem::take(&mut self.changes)
    }

    pub fn observe(&mut self, event: &Value) -> Result<(), &'static str> {
        let result = self.observe_inner(event);
        if result.is_err() {
            self.disconnect();
        }
        result
    }

    fn observe_inner(&mut self, event: &Value) -> Result<(), &'static str> {
        if event["params"]["threadId"] != self.thread_id {
            return Ok(());
        }
        match event["method"].as_str() {
            Some("turn/started") => {
                if let Some(turn) = event["params"]["turn"]["id"]
                    .as_str()
                    .filter(|id| !id.is_empty() && id.len() <= 4096)
                {
                    if !self.retired_turns.iter().any(|retired| retired == turn) {
                        self.current_turn = Some(turn.into());
                        self.turn_finished = false;
                    }
                }
            }
            Some("item/tool/requestUserInput") => {
                tracing::debug!("received native Codex question request");
                if !self.accepts_new_item(event) {
                    return Ok(());
                }
                if self
                    .retired_turns
                    .iter()
                    .any(|turn| event["params"]["turnId"] == *turn)
                {
                    return Ok(());
                }
                if let Some(previous) = self
                    .requests
                    .iter()
                    .find(|entry| entry.raw["id"] == event["id"])
                {
                    return if same_event(&previous.raw, event) {
                        Ok(())
                    } else {
                        Err("codex_question_rpc_reused")
                    };
                }
                self.reserve(event, self.requests.len())?;
                self.requests.push(Request {
                    raw: event.clone(),
                    question: None,
                    state: QuestionState::Pending,
                    answers: None,
                    delivery_id: None,
                    completed_turn: false,
                });
                self.match_sources()?;
            }
            Some("rawResponseItem/completed")
                if event["params"]["item"]["type"] == "function_call" =>
            {
                tracing::debug!(
                    is_question = event["params"]["item"]["name"] == "request_user_input",
                    "received native Codex tool source"
                );
                if !self.accepts_new_item(event) {
                    return Ok(());
                }
                if self.sources.iter().any(|source| {
                    source["params"]["turnId"] == event["params"]["turnId"]
                        && source["params"]["item"]["call_id"] == event["params"]["item"]["call_id"]
                        && !same_event(source, event)
                }) {
                    return Err("codex_question_source_changed");
                }
                if self
                    .retired_turns
                    .iter()
                    .any(|turn| event["params"]["turnId"] == *turn)
                {
                    return Ok(());
                }
                if !self.sources.iter().any(|source| same_event(source, event)) {
                    self.reserve(event, self.sources.len())?;
                    self.sources.push(event.clone());
                    self.match_sources()?;
                }
            }
            Some("rawResponseItem/completed")
                if event["params"]["item"]["type"] == "function_call_output" =>
            {
                for entry in &mut self.requests {
                    if !entry.state.blocks_input() && entry.state != QuestionState::Expired {
                        continue;
                    }
                    let Some(question) = &entry.question else {
                        continue;
                    };
                    if question.consumed_answers(event).is_some() {
                        entry.state = if entry
                            .answers
                            .as_ref()
                            .is_some_and(|answers| question.confirms_answer(event, answers))
                        {
                            QuestionState::Answered
                        } else {
                            QuestionState::AnsweredElsewhere
                        };
                        self.changes.push(CodexQuestionChange {
                            question: question.clone(),
                            state: entry.state,
                            evidence: Some(event.to_string()),
                            delivery_id: if entry.state == QuestionState::Answered {
                                entry.delivery_id.clone()
                            } else {
                                None
                            },
                        });
                    }
                }
                self.reclaim_history();
            }
            Some("serverRequest/resolved") => {
                for entry in &mut self.requests {
                    if entry.raw["id"] == event["params"]["requestId"]
                        && entry.state == QuestionState::Pending
                    {
                        entry.state = QuestionState::Unknown;
                        if let Some(question) = &entry.question {
                            self.changes.push(CodexQuestionChange {
                                question: question.clone(),
                                state: entry.state,
                                evidence: None,
                                delivery_id: entry.delivery_id.clone(),
                            });
                        }
                    }
                }
            }
            Some("turn/completed") => {
                let turn = event["params"]["turn"]["id"].as_str();
                if let Some(turn) = turn {
                    if !self.retired_turns.iter().any(|retired| retired == turn) {
                        if self.retired_turns.len() >= CAPACITY {
                            self.retired_turns.remove(0);
                        }
                        self.retired_turns.push(turn.into());
                    }
                    if self
                        .current_turn
                        .as_deref()
                        .is_none_or(|current| current == turn)
                    {
                        self.current_turn = Some(turn.into());
                        self.turn_finished = true;
                    }
                    for entry in &mut self.requests {
                        if entry.raw["params"]["turnId"] == turn {
                            entry.completed_turn = true;
                        }
                        if entry.raw["params"]["turnId"] == turn
                            && (entry.state.blocks_input() || entry.state == QuestionState::Expired)
                        {
                            entry.state = if entry.answers.is_some() {
                                QuestionState::Unknown
                            } else {
                                QuestionState::Cancelled
                            };
                            if let Some(question) = &entry.question {
                                self.changes.push(CodexQuestionChange {
                                    question: question.clone(),
                                    state: entry.state,
                                    evidence: None,
                                    delivery_id: entry.delivery_id.clone(),
                                });
                            }
                        }
                    }
                    // Unknown sends retain their question for late exact consumption. Completed
                    // history and unrelated tool payloads cannot exhaust a long live session.
                    self.sources
                        .retain(|source| source["params"]["turnId"] != turn);
                    self.reclaim_history();
                }
            }
            _ => {}
        }
        if self.changes.len() > CAPACITY {
            return Err("codex_question_consumer_overflow");
        }
        Ok(())
    }

    /// Called by the transport immediately before writing. The caller must not replay on IO failure.
    pub fn reply(
        &mut self,
        question: &CodexQuestion,
        answers: &[QuestionAnswer],
    ) -> Result<Value, &'static str> {
        self.reply_delivery(question, answers, None)
    }

    pub fn reply_delivery(
        &mut self,
        question: &CodexQuestion,
        answers: &[QuestionAnswer],
        delivery_id: Option<&str>,
    ) -> Result<Value, &'static str> {
        if !self.connected {
            return Err("codex_question_disconnected");
        }
        let entry = self
            .requests
            .iter_mut()
            .find(|entry| entry.question.as_ref() == Some(question))
            .ok_or("codex_question_not_found")?;
        if entry.state != QuestionState::Pending {
            return Err("codex_question_not_pending");
        }
        let result = question.response(answers)?;
        entry.answers = Some(answers.to_vec());
        entry.state = QuestionState::Submitting;
        entry.delivery_id = delivery_id.map(str::to_owned);
        self.changes.push(CodexQuestionChange {
            question: question.clone(),
            state: entry.state,
            evidence: None,
            delivery_id: entry.delivery_id.clone(),
        });
        Ok(json!({"id":question.rpc_id(), "result":result}))
    }

    pub fn sent(&mut self, question: &CodexQuestion) {
        if let Some(entry) = self
            .requests
            .iter_mut()
            .find(|entry| entry.question.as_ref() == Some(question))
        {
            if entry.state == QuestionState::Submitting {
                entry.state = QuestionState::Submitted;
                self.changes.push(CodexQuestionChange {
                    question: question.clone(),
                    state: entry.state,
                    evidence: None,
                    delivery_id: entry.delivery_id.clone(),
                });
            }
        }
    }

    /// Observe the native client's response before forwarding it, without rewriting its decision.
    pub fn observe_native_response(&mut self, frame: &Value) -> Result<(), &'static str> {
        if frame.get("method").is_some()
            || (frame.get("result").is_none() && frame.get("error").is_none())
        {
            return Ok(());
        }
        if let Some(entry) = self
            .requests
            .iter_mut()
            .find(|entry| entry.raw["id"] == frame["id"] && entry.state == QuestionState::Pending)
        {
            entry.state = QuestionState::Unknown;
            if let Some(question) = &entry.question {
                self.changes.push(CodexQuestionChange {
                    question: question.clone(),
                    state: entry.state,
                    evidence: None,
                    delivery_id: entry.delivery_id.clone(),
                });
            }
        }
        Ok(())
    }

    /// A transport gap revokes all unsent requests. Sent answers await evidence and are never replayed.
    pub fn disconnect(&mut self) {
        self.connected = false;
        for entry in &mut self.requests {
            if !entry.state.blocks_input() || entry.state == QuestionState::Unknown {
                continue;
            }
            entry.state = if entry.answers.is_some() {
                QuestionState::Unknown
            } else {
                QuestionState::Expired
            };
            if let Some(question) = &entry.question {
                self.changes.push(CodexQuestionChange {
                    question: question.clone(),
                    state: entry.state,
                    evidence: None,
                    delivery_id: entry.delivery_id.clone(),
                });
            }
        }
    }

    fn reserve(&mut self, event: &Value, count: usize) -> Result<(), &'static str> {
        let size = event.to_string().len();
        if size > MAX_EVENT_BYTES || count >= CAPACITY || self.bytes + size > MAX_LEDGER_BYTES {
            return Err("codex_question_ledger_capacity");
        }
        self.bytes += size;
        Ok(())
    }

    fn accepts_new_item(&self, event: &Value) -> bool {
        self.connected
            && self
                .current_turn
                .as_ref()
                .is_none_or(|turn| !self.turn_finished && event["params"]["turnId"] == *turn)
    }

    fn reclaim_history(&mut self) {
        self.requests
            .retain(|entry| !entry.completed_turn || entry.state == QuestionState::Unknown);
        // A retired unknown request still owns its source for exact late reconciliation.
        // Account each retained source once, whether indexed or held by the parsed question.
        self.bytes = self
            .sources
            .iter()
            .map(|value| value.to_string().len())
            .sum::<usize>()
            + self
                .requests
                .iter()
                .map(|entry| {
                    let source = entry.question.as_ref().map_or(0, |question| {
                        if self
                            .sources
                            .iter()
                            .any(|source| same_event(source, question.original_source()))
                        {
                            0
                        } else {
                            question.original_source().to_string().len()
                        }
                    });
                    entry.raw.to_string().len() + source
                })
                .sum::<usize>();
    }

    fn match_sources(&mut self) -> Result<(), &'static str> {
        if !self.connected {
            return Ok(());
        }
        for entry in &mut self.requests {
            if entry.question.is_some() || entry.state != QuestionState::Pending {
                continue;
            }
            if let Some(question) = self.sources.iter().find_map(|source| {
                match CodexQuestion::from_events(&entry.raw, source) {
                    Ok(question) => Some(question),
                    Err(reason) => {
                        tracing::debug!(reason, "native Codex question source not matched");
                        None
                    }
                }
            }) {
                self.changes.push(CodexQuestionChange {
                    question: question.clone(),
                    state: entry.state,
                    evidence: None,
                    delivery_id: entry.delivery_id.clone(),
                });
                entry.question = Some(question);
            }
        }
        if self.changes.len() > CAPACITY {
            return Err("codex_question_consumer_overflow");
        }
        Ok(())
    }
}

// Notification transport timestamps do not belong to the RPC or tool-call identity.
fn same_event(a: &Value, b: &Value) -> bool {
    a["method"] == b["method"] && a["id"] == b["id"] && a["params"] == b["params"]
}
