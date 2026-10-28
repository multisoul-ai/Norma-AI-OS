//! Claude's question tool is distinct from permission requests, even though its reply uses allow.
use crate::protocol::{
    validate_question_answers, validate_question_items, AgentProfile, PendingQuestion,
    QuestionAnswer, QuestionItem, QuestionOption, QuestionRef, QuestionRequestId,
};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct ClaudeQuestion {
    session_id: String,
    tool_use_id: String,
    input: Value,
    items: Vec<QuestionItem>,
}

impl ClaudeQuestion {
    pub fn from_hook(raw: &Value) -> Result<Self, &'static str> {
        if raw["hook_event_name"] != "PreToolUse" || raw["tool_name"] != "AskUserQuestion" {
            return Err("not_claude_question");
        }
        let session_id = string(&raw["session_id"])?;
        let tool_use_id = string(&raw["tool_use_id"])?;
        let input = raw["tool_input"].clone();
        if input
            .get("answers")
            .is_some_and(|answers| !answers.as_object().is_some_and(Map::is_empty))
        {
            return Err("question_already_has_answers");
        }
        let questions = input["questions"].as_array().ok_or("invalid_questions")?;
        let mut prompts = HashSet::new();
        let mut items = Vec::new();
        for (index, question) in questions.iter().enumerate() {
            let prompt = string(&question["question"])?;
            if !prompts.insert(prompt.clone()) {
                return Err("ambiguous_question_text");
            }
            let options = question["options"].as_array().ok_or("invalid_options")?;
            let mut labels = HashSet::new();
            let mut normalized = Vec::new();
            for (index, option) in options.iter().enumerate() {
                let label = string(&option["label"])?;
                if !labels.insert(label.clone()) {
                    return Err("ambiguous_option_label");
                }
                normalized.push(QuestionOption {
                    key: format!("option-{index}"),
                    label,
                    description: match option.get("description") {
                        None | Some(Value::Null) => None,
                        Some(Value::String(text)) => Some(text.clone()),
                        _ => return Err("invalid_option_description"),
                    },
                });
            }
            let multiple = match question.get("multiSelect") {
                Some(Value::Bool(value)) => *value,
                None => false,
                _ => return Err("invalid_multi_select"),
            };
            items.push(QuestionItem {
                key: format!("question-{index}"),
                provider_id: None,
                prompt,
                options: normalized,
                multiple,
                allow_custom: true,
                secret: false,
            });
        }
        validate_question_items(&items)?;
        Ok(Self {
            session_id,
            tool_use_id,
            input,
            items,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn tool_use_id(&self) -> &str {
        &self.tool_use_id
    }

    pub fn question(&self, target: QuestionRef) -> Result<PendingQuestion, &'static str> {
        if target.provider != AgentProfile::Claude
            || target.session_id != self.session_id
            || target.request_id != QuestionRequestId::String(self.tool_use_id.clone())
        {
            return Err("question_hook_identity_mismatch");
        }
        let question = PendingQuestion {
            target,
            items: self.items.clone(),
        };
        question.validate()?;
        Ok(question)
    }

    fn answer_map(&self, answers: &[QuestionAnswer]) -> Result<Value, &'static str> {
        validate_question_answers(&self.items, answers)?;
        let mut mapped = Map::new();
        for item in &self.items {
            let answer = answers
                .iter()
                .find(|answer| answer.key == item.key)
                .ok_or("incomplete_answers")?;
            let mut labels: Vec<_> = answer
                .selected
                .iter()
                .map(|key| {
                    item.options
                        .iter()
                        .find(|option| option.key == *key)
                        .expect("validated option")
                        .label
                        .clone()
                })
                .collect();
            if let Some(text) = &answer.text {
                labels.push(text.clone());
            }
            mapped.insert(item.prompt.clone(), Value::String(labels.join(", ")));
        }
        Ok(Value::Object(mapped))
    }

    pub fn answer_output(&self, answers: &[QuestionAnswer]) -> Result<Value, &'static str> {
        let mut updated = self.input.clone();
        updated["answers"] = self.answer_map(answers)?;
        Ok(json!({"hookSpecificOutput":{"hookEventName":"PreToolUse",
            "permissionDecision":"allow","updatedInput":updated}}))
    }

    pub fn confirms_answer(&self, raw: &Value, answers: &[QuestionAnswer]) -> bool {
        self.confirms_consumption(raw)
            && self
                .answer_map(answers)
                .is_ok_and(|expected| raw["tool_response"]["answers"] == expected)
    }

    /// Tool failure ends this invocation; a cancelled HTTP wait merely falls back to the TUI.
    pub fn confirms_failure(&self, raw: &Value) -> bool {
        raw["hook_event_name"] == "PostToolUseFailure"
            && raw["tool_name"] == "AskUserQuestion"
            && raw["session_id"] == self.session_id
            && raw["tool_use_id"] == self.tool_use_id
            && raw["tool_input"]["questions"] == self.input["questions"]
            && raw["error"].as_str().is_some_and(|error| !error.is_empty())
    }

    /// A correlated native user answer closes the same question without attributing it to Norma.
    pub fn confirms_consumption(&self, raw: &Value) -> bool {
        raw["hook_event_name"] == "PostToolUse"
            && raw["tool_name"] == "AskUserQuestion"
            && raw["session_id"] == self.session_id
            && raw["tool_use_id"] == self.tool_use_id
            && raw["tool_response"]["questions"] == self.input["questions"]
            && raw["tool_response"]["answers"]
                .as_object()
                .is_some_and(|answers| {
                    answers.len() == self.items.len()
                        && self.items.iter().all(|item| {
                            answers
                                .get(&item.prompt)
                                .and_then(Value::as_str)
                                .is_some_and(|answer| {
                                    !answer.trim().is_empty() && answer.len() <= 32768
                                })
                        })
                })
    }
}

fn string(value: &Value) -> Result<String, &'static str> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 32768)
        .map(str::to_owned)
        .ok_or("invalid_question_string")
}
