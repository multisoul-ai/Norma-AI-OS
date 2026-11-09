//! Hook-shaped AskUserQuestion replies. Each provider keeps the shape its spike accepted.
use crate::protocol::{AgentProfile, QuestionItem};
use serde_json::{json, Map, Value};

/// How a blocking PreToolUse hook returns the island's answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookAskReply {
    /// Claude and ZCode execute the tool with the original questions plus `answers`.
    AllowUpdatedInput,
    /// Grok ignores `updatedInput.answers`. Deny text is the model-visible answer.
    TopLevelDeny,
    /// Kimi's verified stdout is only `hookSpecificOutput` deny plus a reason.
    KimiDeny,
    /// InfCode normalizes either deny form into `infcodeHookBlocked`. Emit both.
    InfCodeDeny,
}

pub fn hook_ask_reply(profile: AgentProfile) -> Option<HookAskReply> {
    match profile {
        AgentProfile::Claude | AgentProfile::ZCode => Some(HookAskReply::AllowUpdatedInput),
        AgentProfile::Grok => Some(HookAskReply::TopLevelDeny),
        AgentProfile::Kimi => Some(HookAskReply::KimiDeny),
        AgentProfile::InfCode => Some(HookAskReply::InfCodeDeny),
        _ => None,
    }
}

pub fn is_hook_ask_tool(profile: AgentProfile, tool_name: Option<&str>) -> bool {
    let Some(name) = tool_name else {
        return false;
    };
    match profile {
        AgentProfile::Grok => name == "ask_user_question" || name == "AskUserQuestion",
        AgentProfile::InfCode => name == "question",
        AgentProfile::Claude | AgentProfile::Kimi | AgentProfile::ZCode => {
            name == "AskUserQuestion"
        }
        _ => false,
    }
}

/// Deny replies are themselves the tool result. Allow replies still need PostToolUse.
pub fn answer_is_hook_terminal(reply: HookAskReply) -> bool {
    !matches!(reply, HookAskReply::AllowUpdatedInput)
}

pub fn hook_connection_prefix(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Grok => "grok",
        AgentProfile::Kimi => "kimi",
        AgentProfile::ZCode => "zcode",
        AgentProfile::InfCode => "infcode",
        _ => "hook",
    }
}

pub fn hook_invocation_id(raw: &Value) -> Option<&str> {
    [
        "tool_use_id",
        "toolUseId",
        "tool_call_id",
        "toolCallId",
        "call_id",
        "callId",
    ]
    .iter()
    .find_map(|key| {
        raw.get(*key)
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty() && id.len() <= 32768)
    })
}

pub fn canonical_hook_event(raw: &Value) -> Option<&'static str> {
    let name = ["hook_event_name", "hookEventName", "event_name"]
        .iter()
        .find_map(|key| raw.get(*key).and_then(Value::as_str))?;
    match name {
        "PreToolUse" | "pre_tool_use" | "preToolUse" => Some("PreToolUse"),
        "PostToolUse" | "post_tool_use" | "postToolUse" => Some("PostToolUse"),
        "PostToolUseFailure" | "post_tool_use_failure" | "postToolUseFailure" => {
            Some("PostToolUseFailure")
        }
        _ => None,
    }
}

pub fn tool_input(raw: &Value) -> Option<&Value> {
    ["tool_input", "toolInput"]
        .iter()
        .find_map(|key| raw.get(*key).filter(|value| !value.is_null()))
}

pub fn tool_response(raw: &Value) -> Option<&Value> {
    ["tool_response", "toolResponse"]
        .iter()
        .find_map(|key| raw.get(*key).filter(|value| !value.is_null()))
}

pub fn error_text(raw: &Value) -> Option<&str> {
    match raw.get("error") {
        Some(Value::String(text)) if !text.is_empty() => Some(text.as_str()),
        Some(Value::Object(map)) => map
            .get("message")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty()),
        _ => None,
    }
}

pub fn reason_text(
    items: &[QuestionItem],
    mapped: &Map<String, Value>,
) -> Result<String, &'static str> {
    let mut parts = Vec::with_capacity(items.len());
    for item in items {
        let value = mapped
            .get(&item.prompt)
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty() && text.len() <= 32768)
            .ok_or("incomplete_answers")?;
        parts.push(format!(
            "The user answered {prompt} with {value}.",
            prompt = item.prompt
        ));
    }
    Ok(parts.join(" "))
}

/// Bridge payload. Deny forms keep `answers` so the CLI can rebuild a safe stdout.
pub fn bridge_stdout(
    reply: HookAskReply,
    input: &Value,
    mapped: Value,
    items: &[QuestionItem],
) -> Result<Value, &'static str> {
    match reply {
        HookAskReply::AllowUpdatedInput => {
            let mut updated = input.clone();
            updated["answers"] = mapped;
            Ok(json!({"hookSpecificOutput":{"hookEventName":"PreToolUse",
                "permissionDecision":"allow","updatedInput":updated}}))
        }
        HookAskReply::TopLevelDeny => {
            let reason = reason_text(items, mapped.as_object().ok_or("invalid_question_answers")?)?;
            Ok(json!({"decision":"deny","reason":reason,"answers":mapped}))
        }
        HookAskReply::KimiDeny => {
            let reason = reason_text(items, mapped.as_object().ok_or("invalid_question_answers")?)?;
            Ok(json!({"hookSpecificOutput":{"permissionDecision":"deny",
                "permissionDecisionReason":reason},"answers":mapped}))
        }
        HookAskReply::InfCodeDeny => {
            let reason = reason_text(items, mapped.as_object().ok_or("invalid_question_answers")?)?;
            Ok(json!({
                "decision":"deny",
                "reason":reason,
                "hookSpecificOutput":{
                    "hookEventName":"PreToolUse",
                    "permissionDecision":"deny",
                    "permissionDecisionReason":reason
                },
                "answers":mapped
            }))
        }
    }
}

pub fn strip_bridge_answers(mut output: Value) -> Value {
    if let Some(object) = output.as_object_mut() {
        object.remove("answers");
    }
    output
}
