//! Native ACP questions are durable cards; the original tool receives no user answers.
use super::{client::AcpError, wire::Connection};
use crate::{
    event::{BrainStatus, BrainStatusKind},
    staff::TaskStore,
};
use norma::protocol::{
    BrainScope, QuestionItem, QuestionOption, StaffQuestion, StaffQuestionState, TaskMessage,
};
use serde_json::{json, Value};
use std::{
    sync::{mpsc::Sender, Arc},
    time::Instant,
};

pub struct QuestionHost {
    store: Arc<TaskStore>,
    scope: BrainScope,
    task_id: String,
    turn: u64,
    status: Sender<BrainStatus>,
    pending: Vec<Waiting>,
}
struct Waiting {
    request: Value,
    question_id: String,
    deadline: Instant,
}

impl QuestionHost {
    pub fn new(
        store: Arc<TaskStore>,
        scope: BrainScope,
        task_id: String,
        turn: u64,
        status: Sender<BrainStatus>,
    ) -> Self {
        Self {
            store,
            scope,
            task_id,
            turn,
            status,
            pending: Vec::new(),
        }
    }

    pub(super) fn open(&mut self, request: &Value, params: &Value) -> Result<bool, AcpError> {
        let Some(items) = parse_form(params) else {
            return Ok(false);
        };
        let id = format!(
            "acp-{}-{}-{}",
            norma::protocol::timestamp(),
            self.turn,
            request
        );
        let question = StaffQuestion::new(id.clone(), items, None);
        let task = self
            .store
            .add_question(&self.scope.workspace_id, &self.task_id, question)
            .map_err(|error| AcpError::Remote(error.to_string()))?;
        self.publish(task);
        self.pending.push(Waiting {
            request: request.clone(),
            question_id: id,
            deadline: Instant::now() + norma::agent_hook::QUESTION_TIMEOUT,
        });
        Ok(true)
    }

    pub(super) fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(super) fn poll(&mut self, conn: &mut Connection, expire: bool) -> Result<(), AcpError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let task = self
            .store
            .get(&self.scope.workspace_id, &self.task_id)
            .map_err(|error| AcpError::Remote(error.to_string()))?;
        let mut index = 0;
        while index < self.pending.len() {
            let waiting = &self.pending[index];
            let is_pending = task.messages.iter().any(|message| matches!(message,
                TaskMessage::Question { question } if question.id == waiting.question_id && question.state == StaffQuestionState::Pending));
            if is_pending && !expire && Instant::now() < waiting.deadline {
                index += 1;
                continue;
            }
            if is_pending {
                let (task, _) = self
                    .store
                    .resolve_question(
                        &self.scope.workspace_id,
                        &self.task_id,
                        &waiting.question_id,
                        StaffQuestionState::TimedOut,
                        &[],
                    )
                    .map_err(|error| AcpError::Remote(error.to_string()))?;
                self.publish(task);
            }
            let waiting = self.pending.remove(index);
            // Cancellation still settles every card when the adapter has already exited.
            let _ = conn.respond(&waiting.request, json!({"action":"decline"}));
        }
        Ok(())
    }

    fn publish(&self, task: norma::protocol::Task) {
        let _ = self.status.send(BrainStatus {
            scope: self.scope.clone(),
            kind: BrainStatusKind::TaskChanged(task),
        });
    }
}

/// Recognize only adapter-native AskUserQuestion/request_user_input forms, not generic MCP forms.
fn parse_form(params: &Value) -> Option<Vec<QuestionItem>> {
    if params["mode"] != "form" || params["toolCallId"].as_str()?.is_empty() {
        return None;
    }
    let properties = params["requestedSchema"]["properties"].as_object()?;
    let mut fields: Vec<_> = properties
        .iter()
        .filter(|(key, value)| {
            value["_meta"]["codex"]["isOther"].is_boolean()
                || key
                    .strip_prefix("question_")
                    .is_some_and(|index| index.parse::<usize>().is_ok())
        })
        .collect();
    fields.sort_by_key(|(key, _)| {
        key.strip_prefix("question_")
            .and_then(|index| index.parse::<usize>().ok())
            .unwrap_or(0)
    });
    let count = fields.len();
    let items: Vec<_> = fields
        .into_iter()
        .map(|(key, field)| {
            let codex = field["_meta"]["codex"].is_object();
            let multiple = field["type"] == "array";
            let choices = if multiple {
                &field["items"]["anyOf"]
            } else {
                &field["oneOf"]
            };
            let allow_custom = if codex {
                field["_meta"]["codex"]["isOther"]
                    .as_bool()
                    .unwrap_or(false)
            } else {
                properties.contains_key(&format!("{key}_custom"))
            };
            let options = choices
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|choice| {
                    let value = choice["const"].as_str()?;
                    // Codex supplies this synthetic option only as a pointer to its note field.
                    if codex && allow_custom && value == "None of the above" {
                        return None;
                    }
                    Some(QuestionOption {
                        key: value.into(),
                        label: choice["title"].as_str().unwrap_or(value).into(),
                        description: choice["description"].as_str().map(str::to_owned),
                    })
                })
                .collect();
            let prompt = if codex {
                field["title"].as_str()
            } else if count == 1 {
                params["message"].as_str()
            } else {
                field["description"]
                    .as_str()
                    .or_else(|| field["title"].as_str())
            };
            QuestionItem {
                key: key.clone(),
                provider_id: Some(key.clone()),
                prompt: prompt.unwrap_or_default().into(),
                options,
                multiple,
                allow_custom,
                secret: field["_meta"]["codex"]["isSecret"]
                    .as_bool()
                    .unwrap_or(false),
            }
        })
        .collect();
    norma::protocol::validate_question_items(&items).ok()?;
    Some(items)
}

#[cfg(test)]
#[path = "../../tests/unit/acp_questions_tests.rs"]
mod tests;
