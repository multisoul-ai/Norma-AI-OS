//! Claude-shaped question tools. Reply JSON follows the provider spike, not a copied Claude allow.
use super::question_reply::{
    self, bridge_stdout, canonical_hook_event, error_text, hook_ask_reply, hook_connection_prefix,
    hook_invocation_id, is_hook_ask_tool, strip_bridge_answers, tool_input, tool_response,
    HookAskReply,
};
use crate::protocol::{
    validate_question_answers, validate_question_items, AgentProfile, PendingQuestion,
    QuestionAnswer, QuestionItem, QuestionOption, QuestionRef, QuestionRequestId,
};
use serde_json::{Map, Value};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct ClaudeQuestion {
    provider: AgentProfile,
    reply: HookAskReply,
    session_id: String,
    tool_use_id: String,
    input: Value,
    items: Vec<QuestionItem>,
}

impl ClaudeQuestion {
    pub fn from_hook(raw: &Value) -> Result<Self, &'static str> {
        Self::from_profile(AgentProfile::Claude, raw)
    }

    pub fn from_profile(provider: AgentProfile, raw: &Value) -> Result<Self, &'static str> {
        let reply = hook_ask_reply(provider).ok_or("not_hook_question")?;
        if canonical_hook_event(raw) != Some("PreToolUse") {
            return Err("not_hook_question");
        }
        let tool_name = field_str(raw, &["tool_name", "toolName"]).ok_or("not_hook_question")?;
        if !is_hook_ask_tool(provider, Some(tool_name)) {
            return Err("not_hook_question");
        }
        let session_id = bounded(
            field_str(raw, &["session_id", "sessionId"]).ok_or("invalid_question_string")?,
        )?;
        let tool_use_id = bounded(hook_invocation_id(raw).ok_or("invalid_question_string")?)?;
        let input = tool_input(raw).cloned().ok_or("invalid_questions")?;
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
            let prompt = bounded(
                question["question"]
                    .as_str()
                    .ok_or("invalid_question_string")?,
            )?;
            if !prompts.insert(prompt.clone()) {
                return Err("ambiguous_question_text");
            }
            let options = question["options"].as_array().ok_or("invalid_options")?;
            let mut labels = HashSet::new();
            let mut normalized = Vec::new();
            for (index, option) in options.iter().enumerate() {
                let label = bounded(option["label"].as_str().ok_or("invalid_question_string")?)?;
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
            provider,
            reply,
            session_id,
            tool_use_id,
            input,
            items,
        })
    }

    pub fn provider(&self) -> AgentProfile {
        self.provider
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn tool_use_id(&self) -> &str {
        &self.tool_use_id
    }
    pub fn answer_is_hook_terminal(&self) -> bool {
        question_reply::answer_is_hook_terminal(self.reply)
    }
    pub fn connection_id(&self, run_id: &str) -> String {
        format!("{}-hook:{run_id}", hook_connection_prefix(self.provider))
    }

    pub fn question(&self, target: QuestionRef) -> Result<PendingQuestion, &'static str> {
        if target.provider != self.provider
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
        bridge_stdout(
            self.reply,
            &self.input,
            self.answer_map(answers)?,
            &self.items,
        )
    }

    /// Drop server-supplied permission fields. Deny stdout is rebuilt from the answer map.
    pub fn sanitize_hook_response(&self, output: Value) -> Result<Value, &'static str> {
        match self.reply {
            HookAskReply::AllowUpdatedInput => self.sanitize_allow(output),
            _ => {
                let answers = output
                    .get("answers")
                    .cloned()
                    .ok_or("missing question answers")?;
                self.accept_answer_map(&answers)?;
                Ok(strip_bridge_answers(bridge_stdout(
                    self.reply,
                    &self.input,
                    answers,
                    &self.items,
                )?))
            }
        }
    }

    fn sanitize_allow(&self, output: Value) -> Result<Value, &'static str> {
        let specific = &output["hookSpecificOutput"];
        if specific["hookEventName"] != "PreToolUse" || specific["permissionDecision"] != "allow" {
            return Err("invalid question response");
        }
        let mut updated = specific["updatedInput"].clone();
        let answers = updated
            .as_object_mut()
            .and_then(|input| input.remove("answers"))
            .ok_or("missing question answers")?;
        self.accept_answer_map(&answers)?;
        let mut original = self.input.clone();
        original
            .as_object_mut()
            .ok_or("invalid original input")?
            .remove("answers");
        if original != updated {
            return Err("question input changed");
        }
        original["answers"] = answers;
        Ok(
            serde_json::json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":original}}),
        )
    }

    fn accept_answer_map(&self, answers: &Value) -> Result<(), &'static str> {
        let mapped = answers.as_object().ok_or("invalid question answers")?;
        if mapped.len() != self.items.len()
            || !self.items.iter().all(|item| {
                mapped
                    .get(&item.prompt)
                    .and_then(Value::as_str)
                    .is_some_and(|answer| !answer.trim().is_empty() && answer.len() <= 32768)
            })
        {
            return Err("incomplete question answers");
        }
        Ok(())
    }

    pub fn confirms_answer(&self, raw: &Value, answers: &[QuestionAnswer]) -> bool {
        self.confirms_consumption(raw)
            && self.answer_map(answers).is_ok_and(|expected| {
                tool_response(raw).is_some_and(|body| body["answers"] == expected)
            })
    }

    pub fn confirms_failure(&self, raw: &Value) -> bool {
        canonical_hook_event(raw) == Some("PostToolUseFailure")
            && self.same_call(raw)
            && input_questions(raw).is_some_and(|questions| questions == &self.input["questions"])
            && error_text(raw).is_some()
    }

    pub fn confirms_consumption(&self, raw: &Value) -> bool {
        if canonical_hook_event(raw) != Some("PostToolUse") || !self.same_call(raw) {
            return false;
        }
        match self.reply {
            HookAskReply::AllowUpdatedInput => tool_response(raw).is_some_and(|body| {
                body["questions"] == self.input["questions"]
                    && body["answers"].as_object().is_some_and(|answers| {
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
            }),
            HookAskReply::TopLevelDeny => tool_response(raw)
                .and_then(|body| body.pointer("/UserAnswered/message"))
                .and_then(Value::as_str)
                .is_some_and(|message| !message.trim().is_empty() && message.len() <= 32768),
            // Terminal success payloads were not sampled. Same call plus the original questions
            // is the only close signal that cannot attach to a different tool.
            HookAskReply::KimiDeny | HookAskReply::InfCodeDeny => {
                input_questions(raw).is_some_and(|questions| questions == &self.input["questions"])
            }
        }
    }

    fn same_call(&self, raw: &Value) -> bool {
        is_hook_ask_tool(self.provider, field_str(raw, &["tool_name", "toolName"]))
            && field_str(raw, &["session_id", "sessionId"]) == Some(self.session_id.as_str())
            && hook_invocation_id(raw) == Some(self.tool_use_id.as_str())
    }
}

fn input_questions(raw: &Value) -> Option<&Value> {
    tool_input(raw).and_then(|input| input.get("questions"))
}

fn field_str<'a>(raw: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| raw.get(*key).and_then(Value::as_str))
}

fn bounded(value: &str) -> Result<String, &'static str> {
    if value.trim().is_empty() || value.len() > 32768 {
        Err("invalid_question_string")
    } else {
        Ok(value.to_owned())
    }
}
