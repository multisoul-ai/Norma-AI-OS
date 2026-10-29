//! Provider-specific mapping behind the same revocable host wait. Approval tools are excluded.
use super::{AgentHookEvent, ClaudeQuestion};
use crate::codex::questions::CodexQuestion;
use crate::opencode::questions::OpenCodeQuestion;
use crate::protocol::{
    AgentProfile, PendingQuestion, QuestionAnswer, QuestionRef, QuestionRequestId,
};
use serde_json::{json, Value};

/// Bridge response data; this is not a send permit. The owning ticket must still qualify IO.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NativeQuestionReply {
    pub delivery_id: String,
    pub answers: Vec<QuestionAnswer>,
}

impl NativeQuestionReply {
    pub fn validate(&self, question: &NativeQuestion) -> Result<(), &'static str> {
        if self.delivery_id.trim().is_empty() || self.delivery_id.len() > 256 {
            return Err("invalid_delivery_id");
        }
        question.answer_output(&self.answers)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum NativeQuestion {
    Claude(ClaudeQuestion),
    Codex {
        question: CodexQuestion,
        connection_id: String,
    },
    OpenCode {
        question: OpenCodeQuestion,
        connection_id: String,
    },
    Cursor {
        question: crate::cursor::questions::CursorQuestion,
        connection_id: String,
    },
}

impl NativeQuestion {
    pub fn from_event(event: &AgentHookEvent) -> Result<Self, &'static str> {
        let adapter = match event.profile {
            AgentProfile::Claude
                if event.provider_event == "PreToolUse"
                    && event.tool_name.as_deref() == Some("AskUserQuestion") =>
            {
                Self::Claude(ClaudeQuestion::from_hook(&event.raw_json)?)
            }
            AgentProfile::Codex
                if event.provider_event == "NormaQuestion"
                    && event.tool_name.as_deref() == Some("request_user_input") =>
            {
                let raw = &event.raw_json;
                let question = CodexQuestion::from_events(&raw["request"], &raw["source"])?;
                let connection_id = raw["connection_id"]
                    .as_str()
                    .filter(|id| id.starts_with("codex-relay:") && id.len() <= 256)
                    .ok_or("question_connection_missing")?;
                if event.turn_id.as_deref() != Some(question.turn_id()) {
                    return Err("question_turn_mismatch");
                }
                Self::Codex {
                    question,
                    connection_id: connection_id.into(),
                }
            }
            AgentProfile::OpenCode
                if event.provider_event == "NormaQuestion"
                    && event.tool_name.as_deref() == Some("question") =>
            {
                let raw = &event.raw_json;
                let question = OpenCodeQuestion::from_request(&raw["request"])?;
                let connection_id = raw["connection_id"]
                    .as_str()
                    .filter(|id| id.starts_with("opencode-native:") && id.len() <= 256)
                    .ok_or("question_connection_missing")?;
                if event.turn_id.is_some() {
                    return Err("question_turn_mismatch");
                }
                Self::OpenCode {
                    question,
                    connection_id: connection_id.into(),
                }
            }
            AgentProfile::Cursor
                if event.provider_event == "NormaQuestion"
                    && event.tool_name.as_deref() == Some("AskQuestion") =>
            {
                if event.turn_id.is_some() {
                    return Err("question_turn_mismatch");
                }
                let session = event
                    .session_id
                    .as_deref()
                    .ok_or("question_session_missing")?;
                let question = crate::cursor::questions::CursorQuestion::from_tool(
                    session,
                    &event.raw_json["source"],
                )?;
                let connection_id = event.raw_json["connection_id"]
                    .as_str()
                    .filter(|id| id.starts_with("cursor-native:") && id.len() <= 256)
                    .ok_or("question_connection_missing")?
                    .into();
                Self::Cursor {
                    question,
                    connection_id,
                }
            }
            _ => return Err("unsupported_question_provider"),
        };
        if event.session_id.as_deref() != Some(adapter.session_id()) {
            return Err("question_session_mismatch");
        }
        Ok(adapter)
    }

    pub fn session_id(&self) -> &str {
        match self {
            Self::Claude(question) => question.session_id(),
            Self::Codex { question, .. } => question.thread_id(),
            Self::OpenCode { question, .. } => question.session_id(),
            Self::Cursor { question, .. } => question.session_id(),
        }
    }

    pub fn provider(&self) -> AgentProfile {
        match self {
            Self::Claude(_) => AgentProfile::Claude,
            Self::Codex { .. } => AgentProfile::Codex,
            Self::OpenCode { .. } => AgentProfile::OpenCode,
            Self::Cursor { .. } => AgentProfile::Cursor,
        }
    }

    pub fn request_id(&self) -> QuestionRequestId {
        match self {
            Self::Claude(question) => QuestionRequestId::String(question.tool_use_id().into()),
            Self::Cursor { question, .. } => QuestionRequestId::String(question.call_id().into()),
            Self::OpenCode { question, .. } => {
                QuestionRequestId::String(question.request_id().into())
            }
            Self::Codex { question, .. } => {
                serde_json::from_value(question.rpc_id()).expect("validated RPC id")
            }
        }
    }

    pub fn tool_use_id(&self) -> &str {
        match self {
            Self::Claude(question) => question.tool_use_id(),
            Self::Codex { question, .. } => question.item_id(),
            Self::OpenCode { question, .. } => question.call_id(),
            Self::Cursor { question, .. } => question.call_id(),
        }
    }

    pub fn connection_id(&self, run_id: &str) -> String {
        match self {
            Self::Claude(_) => format!("claude-hook:{run_id}"),
            Self::Codex { connection_id, .. }
            | Self::OpenCode { connection_id, .. }
            | Self::Cursor { connection_id, .. } => connection_id.clone(),
        }
    }

    pub fn question(&self, target: QuestionRef) -> Result<PendingQuestion, &'static str> {
        match self {
            Self::Claude(question) => question.question(target),
            Self::Cursor {
                question,
                connection_id,
            } => {
                if target.connection_id != *connection_id {
                    return Err("question_connection_mismatch");
                }
                question.question(target)
            }
            Self::OpenCode {
                question,
                connection_id,
            } => {
                if target.connection_id != *connection_id {
                    return Err("question_connection_mismatch");
                }
                question.question(target)
            }
            Self::Codex {
                question,
                connection_id,
            } => {
                if target.connection_id != *connection_id {
                    return Err("question_connection_mismatch");
                }
                question.question(target)
            }
        }
    }

    pub fn answer_output(&self, answers: &[QuestionAnswer]) -> Result<Value, &'static str> {
        match self {
            Self::Claude(question) => question.answer_output(answers),
            Self::Cursor { question, .. } => {
                question.expected_result(answers)?;
                Ok(json!({"answers":answers}))
            }
            Self::OpenCode { question, .. } => {
                question.response(answers)?;
                Ok(json!({"answers":answers}))
            }
            Self::Codex { question, .. } => {
                question.response(answers)?;
                Ok(json!({"answers": answers}))
            }
        }
    }

    pub fn confirms_consumption(&self, raw: &Value) -> bool {
        match self {
            Self::Claude(question) => question.confirms_consumption(raw),
            Self::Cursor {
                question,
                connection_id,
            } => {
                raw["connection_id"] == *connection_id
                    && question.confirms_consumption(&raw["result"])
            }
            Self::OpenCode {
                question,
                connection_id,
            } => {
                raw["connection_id"] == *connection_id
                    && question.consumed_answers(&raw["part"]).is_some()
            }
            Self::Codex {
                question,
                connection_id,
            } => {
                raw["connection_id"] == *connection_id
                    && question.consumed_answers(&raw["event"]).is_some()
            }
        }
    }

    pub fn confirms_answer(&self, raw: &Value, answers: &[QuestionAnswer]) -> bool {
        match self {
            Self::Claude(question) => question.confirms_answer(raw, answers),
            Self::Cursor {
                question,
                connection_id,
            } => {
                raw["connection_id"] == *connection_id
                    && question.confirms_answer(&raw["result"], answers)
            }
            Self::OpenCode {
                question,
                connection_id,
            } => {
                raw["connection_id"] == *connection_id
                    && question.confirms_answer(&raw["part"], answers)
            }
            Self::Codex {
                question,
                connection_id,
            } => {
                raw["connection_id"] == *connection_id
                    && question.confirms_answer(&raw["event"], answers)
            }
        }
    }

    pub fn confirms_failure(&self, raw: &Value) -> bool {
        match self {
            Self::Claude(question) => question.confirms_failure(raw),
            Self::Codex { .. } | Self::OpenCode { .. } | Self::Cursor { .. } => false,
        }
    }
}
