use crate::{CardToolError, ProviderToolSpec};
use norma::protocol::{AgentProfile, AgentTaskAssignment, CardCommand, CardKind, Placement};
use serde_json::{json, Value};

pub const CARD_TASK_TOOL: &str = "card_task";

pub fn dialogue_tool_specs() -> Vec<ProviderToolSpec> {
    vec![ProviderToolSpec {
        name: CARD_TASK_TOOL.to_string(),
        description: "Required for every requested Norma OS canvas/card operation. Start immediately for explicit create terminal, create agent, focus, send text, delete, or Agent delegation requests; do not merely explain or ask for confirmation when the target Agent and requested action are identifiable. A sentence-initial exact Agent address followed by an actionable request is delegation even without words such as assign or delegate. For multiple Agents, targets come only from the sentence-initial addressed set or explicit per-Agent assignments: an explicit shared request may relay the same task to each target, while Agent names mentioned inside task text are not additional targets. For delegation, only polish and relay: pass a lightly polished intent that removes only addressing and conversational filler while preserving every goal, scope, order, constraint, deliverable, and explicit command; do not summarize, invent requirements, or analyze the task on the target Agent's behalf; do not ask for implementation details, background, preferences, or acceptance criteria. Clarify only when the target Agent, requested action, or a multi-candidate reference makes the core intent unidentifiable; ask one minimal question and do not repeatedly confirm. A clarification answer may contain only an Agent name, action, or profile; combine it with the prior task and dispatch without asking the user to restate it. Also cancel or query an existing background card task."
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
            "list_agents",
            "List persistent project Agents, including canonical name, speech alias, runtime profile, enabled state, active card, tracking, and busy state. Use this instead of list_cards for Agent routing.",
            json!({"type":"object","additionalProperties":false}),
        ),
        tool_spec(
            "create_agent",
            "Create a persistent named Agent definition. Profile is required; name is optional and uses the deterministic built-in pool when omitted. Voice and quick-create requests normally set activate=true.",
            json!({
                "type":"object",
                "properties": {
                    "name":{"type":"string"},
                    "profile":{"type":"string","enum":["claude","codex","cursor","opencode"]},
                    "activate":{"type":"boolean"}
                },
                "required":["profile"],"additionalProperties":false
            }),
        ),
        tool_spec(
            "activate_agent",
            "Open or focus the single active card for an existing enabled Agent, addressed by exact canonical name or preset speech alias.",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":false}),
        ),
        tool_spec(
            "dispatch_agent_tasks",
            "Atomically validate and dispatch one or more explicit Agent assignments. An explicit shared request may send the same task to multiple Agents as independent assignments. Do not add Agents merely mentioned inside task text. Different Agents may run in parallel; the same managed Agent queues FIFO. Set force_untracked only when the user explicitly says to send anyway.",
            json!({
                "type":"object",
                "properties": {
                    "assignments": {
                        "type":"array","minItems":1,
                        "items": {
                            "type":"object",
                            "properties":{"name":{"type":"string"},"task":{"type":"string"}},
                            "required":["name","task"],"additionalProperties":false
                        }
                    },
                    "force_untracked":{"type":"boolean"}
                },
                "required":["assignments"],"additionalProperties":false
            }),
        ),
        tool_spec(
            "list_cards",
            "List non-Agent cards before choosing or re-checking a target.",
            json!({"type":"object","additionalProperties":false}),
        ),
        tool_spec(
            "create_card",
            "Create a terminal card. Agent cards must use create_agent or activate_agent.",
            json!({
                "type":"object",
                "properties": {
                    "kind":{"type":"string","enum":["terminal"]}
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
            "Send text to a non-Agent terminal card. Agent tasks must use dispatch_agent_tasks.",
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
        "list_agents" => Ok(CardCommand::ListAgents),
        "create_agent" => Ok(CardCommand::CreateAgent {
            name: optional_string(&value, "name")?.map(str::to_string),
            profile: parse_required_profile(&value)?,
            activate: value
                .get("activate")
                .map(parse_bool)
                .transpose()?
                .unwrap_or(true),
        }),
        "activate_agent" => Ok(CardCommand::ActivateAgent {
            name: required_string(&value, "name")?.to_string(),
        }),
        "dispatch_agent_tasks" => parse_agent_dispatch(&value),
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
        "agent" => {
            return Err(CardToolError::InvalidArguments(
                "Agent cards must use create_agent".to_string(),
            ))
        }
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

fn parse_agent_dispatch(value: &Value) -> Result<CardCommand, CardToolError> {
    let assignments = value
        .get("assignments")
        .and_then(Value::as_array)
        .filter(|assignments| !assignments.is_empty())
        .ok_or_else(|| CardToolError::InvalidArguments("missing or invalid assignments".into()))?
        .iter()
        .map(|assignment| {
            Ok(AgentTaskAssignment {
                name: required_string(assignment, "name")?.to_string(),
                task: required_string(assignment, "task")?.to_string(),
            })
        })
        .collect::<Result<Vec<_>, CardToolError>>()?;
    Ok(CardCommand::DispatchAgentTasks {
        assignments,
        force_untracked: value
            .get("force_untracked")
            .map(parse_bool)
            .transpose()?
            .unwrap_or(false),
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

fn optional_string<'a>(value: &'a Value, name: &str) -> Result<Option<&'a str>, CardToolError> {
    match value.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .map(Some)
            .ok_or_else(|| CardToolError::InvalidArguments(format!("invalid {name}"))),
    }
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

fn parse_required_profile(value: &Value) -> Result<AgentProfile, CardToolError> {
    match value.get("profile").and_then(Value::as_str) {
        None => Err(CardToolError::InvalidArguments(
            "missing or invalid profile".to_string(),
        )),
        Some("claude") => Ok(AgentProfile::Claude),
        Some("codex") => Ok(AgentProfile::Codex),
        Some("cursor") => Ok(AgentProfile::Cursor),
        Some("opencode") => Ok(AgentProfile::OpenCode),
        Some(profile) => Err(CardToolError::InvalidArguments(format!(
            "unsupported agent profile: {profile}"
        ))),
    }
}
