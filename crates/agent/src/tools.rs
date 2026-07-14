use crate::{CardToolError, ProviderToolSpec};
use norma::protocol::{AgentProfile, CardCommand, CardKind, Placement};
use serde_json::{json, Value};

pub const CARD_TASK_TOOL: &str = "card_task";

pub fn dialogue_tool_specs() -> Vec<ProviderToolSpec> {
    vec![ProviderToolSpec {
        name: CARD_TASK_TOOL.to_string(),
        description: "Required for every requested Norma OS canvas/card operation. Start immediately for explicit create terminal, create agent, focus, send text, or delete requests; do not merely explain or ask for confirmation when the intent is already actionable. Also cancel or query an existing background card task."
            .to_string(),
        parameters_json_schema: json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["start", "cancel", "status"]},
                "intent": {"type": "string"},
                "task_id": {"type": "integer"}
            },
            "required": ["action"],
            "additionalProperties": false
        }),
    }]
}

pub fn card_subagent_tool_specs() -> Vec<ProviderToolSpec> {
    vec![
        tool_spec(
            "list_cards",
            "List the current cards before choosing or re-checking a target.",
            json!({"type":"object","additionalProperties":false}),
        ),
        tool_spec(
            "create_card",
            "Create a terminal or agent card.",
            json!({
                "type":"object",
                "properties": {
                    "kind":{"type":"string","enum":["terminal","agent"]},
                    "profile":{"type":"string","enum":["claude","codex","opencode"]}
                },
                "required":["kind"],"additionalProperties":false
            }),
        ),
        tool_spec(
            "focus_card",
            "Focus an existing card by node id.",
            json!({"type":"object","properties":{"node_id":{"type":"integer"}},"required":["node_id"],"additionalProperties":false}),
        ),
        tool_spec(
            "send_text",
            "Send text to a terminal or agent card. Set submit true for a command or prompt.",
            json!({"type":"object","properties":{"node_id":{"type":"integer"},"text":{"type":"string"},"submit":{"type":"boolean"}},"required":["node_id","text"],"additionalProperties":false}),
        ),
        tool_spec(
            "delete_card",
            "Delete an existing card. This side effect cannot be rolled back.",
            json!({"type":"object","properties":{"node_id":{"type":"integer"}},"required":["node_id"],"additionalProperties":false}),
        ),
    ]
}

pub fn parse_dialogue_tool_call(arguments_json: &str) -> Result<DialogueToolAction, CardToolError> {
    let value = parse_object(arguments_json)?;
    let action = required_string(&value, "action")?;
    match action {
        "start" => Ok(DialogueToolAction::Start {
            intent: required_string(&value, "intent")?.to_string(),
        }),
        "cancel" => Ok(DialogueToolAction::Cancel {
            task_id: required_u64(&value, "task_id")?,
        }),
        "status" => Ok(DialogueToolAction::Status {
            task_id: value.get("task_id").map(parse_u64).transpose()?,
        }),
        other => Err(CardToolError::InvalidArguments(format!(
            "unknown card_task action: {other}"
        ))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DialogueToolAction {
    Start { intent: String },
    Cancel { task_id: u64 },
    Status { task_id: Option<u64> },
}

pub fn parse_card_subagent_tool_call(
    name: &str,
    arguments_json: &str,
) -> Result<CardCommand, CardToolError> {
    let value = parse_object(arguments_json)?;
    match name {
        "list_cards" => Ok(CardCommand::List),
        "create_card" => parse_create(&value),
        "focus_card" => Ok(CardCommand::Focus {
            node_id: required_u64(&value, "node_id")?,
        }),
        "send_text" => Ok(CardCommand::SendText {
            node_id: required_u64(&value, "node_id")?,
            text: required_string(&value, "text")?.to_string(),
            submit: value
                .get("submit")
                .map(parse_bool)
                .transpose()?
                .unwrap_or(true),
        }),
        "delete_card" => Ok(CardCommand::Delete {
            node_id: required_u64(&value, "node_id")?,
        }),
        other => Err(CardToolError::UnknownTool(other.to_string())),
    }
}

fn parse_create(value: &Value) -> Result<CardCommand, CardToolError> {
    let kind = required_string(value, "kind")?;
    let kind = match kind {
        "terminal" => CardKind::Terminal { shell: None },
        "agent" => CardKind::Agent {
            profile: parse_profile(value.get("profile"))?,
        },
        other => {
            return Err(CardToolError::InvalidArguments(format!(
                "unsupported card kind: {other}"
            )))
        }
    };
    Ok(CardCommand::Create {
        kind,
        placement: Placement::VisibleArea,
    })
}

fn tool_spec(name: &str, description: &str, schema: Value) -> ProviderToolSpec {
    ProviderToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        parameters_json_schema: schema,
    }
}

fn parse_object(arguments_json: &str) -> Result<Value, CardToolError> {
    let value: Value = serde_json::from_str(arguments_json)
        .map_err(|error| CardToolError::InvalidArguments(error.to_string()))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(CardToolError::InvalidArguments(
            "tool arguments must be an object".to_string(),
        ))
    }
}

fn required_string<'a>(value: &'a Value, name: &str) -> Result<&'a str, CardToolError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CardToolError::InvalidArguments(format!("missing or invalid {name}")))
}

fn required_u64(value: &Value, name: &str) -> Result<u64, CardToolError> {
    value
        .get(name)
        .map(parse_u64)
        .transpose()?
        .ok_or_else(|| CardToolError::InvalidArguments(format!("missing or invalid {name}")))
}

fn parse_u64(value: &Value) -> Result<u64, CardToolError> {
    value
        .as_u64()
        .ok_or_else(|| CardToolError::InvalidArguments("expected a non-negative integer".into()))
}

fn parse_bool(value: &Value) -> Result<bool, CardToolError> {
    value
        .as_bool()
        .ok_or_else(|| CardToolError::InvalidArguments("expected a boolean".into()))
}

fn parse_profile(value: Option<&Value>) -> Result<AgentProfile, CardToolError> {
    match value.and_then(Value::as_str).unwrap_or("codex") {
        "claude" => Ok(AgentProfile::Claude),
        "codex" => Ok(AgentProfile::Codex),
        "cursor" => Ok(AgentProfile::Cursor),
        "opencode" => Ok(AgentProfile::OpenCode),
        profile => Err(CardToolError::InvalidArguments(format!(
            "unsupported agent profile: {profile}"
        ))),
    }
}
