//! Convert only a verified request_user_input call, never another tool's similarly shaped approval.
use crate::protocol::{
    validate_question_answers, validate_question_items, AgentProfile, PendingQuestion,
    QuestionAnswer, QuestionItem, QuestionOption, QuestionRef, QuestionRequestId,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;

mod ledger;
pub use ledger::{CodexQuestionChange, CodexQuestions};

#[derive(Clone, Debug, PartialEq)]
pub struct CodexQuestion {
    rpc_id: QuestionRequestId,
    thread_id: String,
    turn_id: String,
    item_id: String,
    items: Vec<QuestionItem>,
    request: Arc<Value>,
    source: Arc<Value>,
}

impl CodexQuestion {
    /// Both events must come from the owning live connection's source ledger.
    pub fn from_events(request: &Value, source: &Value) -> Result<Self, &'static str> {
        if request.to_string().len() > 1_048_576 || source.to_string().len() > 1_048_576 {
            return Err("codex_question_event_too_large");
        }
        if request["method"] != "item/tool/requestUserInput"
            || source["method"] != "rawResponseItem/completed"
            || source["params"]["item"]["type"] != "function_call"
            || source["params"]["item"]["name"] != "request_user_input"
        {
            return Err("codex_question_unverified_source");
        }
        let params = &request["params"];
        let thread_id = identifier(&params["threadId"])?;
        let turn_id = identifier(&params["turnId"])?;
        let item_id = identifier(&params["itemId"])?;
        if params["isBlocking"] != true
            || source["params"]["threadId"] != thread_id
            || source["params"]["turnId"] != turn_id
            || source["params"]["item"]["call_id"] != item_id
        {
            return Err("codex_question_source_mismatch");
        }
        let rpc_id = serde_json::from_value::<QuestionRequestId>(request["id"].clone())
            .map_err(|_| "codex_question_invalid_rpc_id")?;
        if matches!(&rpc_id, QuestionRequestId::String(id) if id.trim().is_empty() || id.len() > 4096)
        {
            return Err("codex_question_invalid_rpc_id");
        }
        let arguments = source["params"]["item"]["arguments"]
            .as_str()
            .filter(|text| text.len() <= 1_048_576)
            .ok_or("codex_question_invalid_source")?;
        let arguments: Value =
            serde_json::from_str(arguments).map_err(|_| "codex_question_invalid_source")?;
        let original = arguments["questions"]
            .as_array()
            .ok_or("codex_question_invalid_source")?;
        let questions = params["questions"]
            .as_array()
            .filter(|questions| !questions.is_empty() && questions.len() <= 32)
            .ok_or("codex_question_invalid_shape")?;
        if original.len() != questions.len() {
            return Err("codex_question_source_mismatch");
        }
        let mut ids = HashSet::new();
        let mut items = Vec::with_capacity(questions.len());
        for (index, (question, original)) in questions.iter().zip(original).enumerate() {
            let id = identifier(&question["id"])?;
            if !ids.insert(id) {
                return Err("codex_question_duplicate_id");
            }
            for field in ["id", "question", "options"] {
                if question[field] != original[field] {
                    return Err("codex_question_source_mismatch");
                }
            }
            for field in ["isOther", "isSecret"] {
                if original.get(field).is_some() && question[field] != original[field] {
                    return Err("codex_question_source_mismatch");
                }
            }
            // The pinned Codex schema supports one selection or custom text, not multiple values.
            if [question, original].iter().any(|value| {
                ["multiSelect", "multiple", "isMultiSelect"]
                    .iter()
                    .any(|field| {
                        value
                            .get(*field)
                            .is_some_and(|value| value != &Value::Bool(false))
                    })
            }) {
                return Err("codex_question_unsupported_cardinality");
            }
            let options = match question.get("options") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(options)) if options.len() <= 128 => {
                    let mut labels = HashSet::new();
                    options
                        .iter()
                        .enumerate()
                        .map(|(index, option)| {
                            let label = identifier(&option["label"])?;
                            if !labels.insert(label) {
                                return Err("codex_question_ambiguous_label");
                            }
                            Ok(QuestionOption {
                                key: format!("option-{index}"),
                                label: label.into(),
                                description: match option.get("description") {
                                    None | Some(Value::Null) => None,
                                    Some(Value::String(text)) => Some(text.clone()),
                                    _ => return Err("codex_question_invalid_description"),
                                },
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?
                }
                _ => return Err("codex_question_invalid_options"),
            };
            items.push(QuestionItem {
                key: format!("question-{index}"),
                provider_id: Some(id.into()),
                prompt: question["question"]
                    .as_str()
                    .ok_or("codex_question_missing_prompt")?
                    .into(),
                options,
                multiple: false,
                allow_custom: question["isOther"]
                    .as_bool()
                    .ok_or("codex_question_missing_custom_capability")?,
                secret: question["isSecret"]
                    .as_bool()
                    .ok_or("codex_question_missing_secret_capability")?,
            });
        }
        validate_question_items(&items)?;
        Ok(Self {
            rpc_id,
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            item_id: item_id.into(),
            items,
            request: Arc::new(request.clone()),
            source: Arc::new(source.clone()),
        })
    }

    pub fn rpc_id(&self) -> Value {
        serde_json::to_value(&self.rpc_id).expect("RPC identity serialization")
    }

    pub fn original_request(&self) -> &Value {
        &self.request
    }
    pub fn original_source(&self) -> &Value {
        &self.source
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }
    pub fn item_id(&self) -> &str {
        &self.item_id
    }

    pub fn question(&self, target: QuestionRef) -> Result<PendingQuestion, &'static str> {
        target.validate()?;
        if target.provider != AgentProfile::Codex
            || target.session_id != self.thread_id
            || target.turn_id.as_deref() != Some(self.turn_id.as_str())
            || target.request_id != self.rpc_id
        {
            return Err("codex_question_target_mismatch");
        }
        Ok(PendingQuestion {
            target,
            items: self.items.clone(),
        })
    }

    pub fn response(&self, answers: &[QuestionAnswer]) -> Result<Value, &'static str> {
        validate_question_answers(&self.items, answers)?;
        let mut mapped = serde_json::Map::new();
        for item in &self.items {
            let answer = answers
                .iter()
                .find(|answer| answer.key == item.key)
                .ok_or("incomplete_answers")?;
            let mut values = answer
                .selected
                .iter()
                .map(|key| {
                    item.options
                        .iter()
                        .find(|option| option.key == *key)
                        .map(|option| option.label.clone())
                        .ok_or("invalid_answer_option")
                })
                .collect::<Result<Vec<_>, _>>()?;
            values.extend(answer.text.iter().cloned());
            mapped.insert(
                item.provider_id
                    .clone()
                    .ok_or("codex_question_missing_id")?,
                json!({"answers":values}),
            );
        }
        Ok(json!({"answers":mapped}))
    }

    /// A resolved RPC, idle state, or another tool's result is never consumption evidence.
    pub fn confirms_answer(&self, event: &Value, answers: &[QuestionAnswer]) -> bool {
        self.response(answers)
            .ok()
            .is_some_and(|expected| self.consumed_answers(event) == Some(expected))
    }

    pub(crate) fn consumed_answers(&self, event: &Value) -> Option<Value> {
        if event["method"] != "rawResponseItem/completed"
            || event["params"]["threadId"] != self.thread_id
            || event["params"]["turnId"] != self.turn_id
            || event["params"]["item"]["type"] != "function_call_output"
            || event["params"]["item"]["call_id"] != self.item_id
        {
            return None;
        }
        let output = event["params"]["item"]["output"]
            .as_str()
            .filter(|text| text.len() <= 1_048_576)
            .and_then(|text| serde_json::from_str::<Value>(text).ok())?;
        let answers = output["answers"].as_object()?;
        (answers.len() == self.items.len()
            && self.items.iter().all(|item| {
                item.provider_id
                    .as_ref()
                    .and_then(|id| answers.get(id))
                    .and_then(|value| value["answers"].as_array())
                    .is_some_and(|values| {
                        values.len() == 1
                            && values[0]
                                .as_str()
                                .is_some_and(|text| !text.trim().is_empty() && text.len() <= 32768)
                    })
            }))
        .then_some(output)
    }
}

fn identifier(value: &Value) -> Result<&str, &'static str> {
    value
        .as_str()
        .filter(|text| !text.trim().is_empty() && text.len() <= 4096)
        .ok_or("codex_question_invalid_identity_or_label")
}
