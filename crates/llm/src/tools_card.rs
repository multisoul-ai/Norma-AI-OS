use norma::protocol::{CardCommand, CardCommandResult, CardKind, Placement};
use serde_json::{json, Value};

use crate::{LlmError, ToolCall, ToolSpec};

pub fn card_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "Create".into(),
            description: "Create a new terminal card on the canvas.".into(),
            parameters_json_schema: json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["terminal"],
                        "description": "Card kind to create."
                    }
                },
                "required": ["kind"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "Focus".into(),
            description: "Focus an existing card by node id.".into(),
            parameters_json_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Target card node id."
                    }
                },
                "required": ["node_id"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "SendText".into(),
            description: "Send text input to a terminal card.".into(),
            parameters_json_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Target card node id."
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to send."
                    },
                    "submit": {
                        "type": "boolean",
                        "description": "Whether to submit after sending. Defaults to true."
                    }
                },
                "required": ["node_id", "text"],
                "additionalProperties": false
            }),
        },
    ]
}

pub fn parse_card_tool_call(call: &ToolCall) -> Result<CardCommand, LlmError> {
    let args = parse_arguments_json(&call.arguments_json)?;
    match call.name.as_str() {
        "Create" => parse_create(&args),
        "Focus" => parse_focus(&args),
        "SendText" => parse_send_text(&args),
        other => Err(LlmError::Message(format!("unknown tool: {other}"))),
    }
}

pub fn tool_result_json_for_command_result(result: &CardCommandResult) -> String {
    let value = match result {
        CardCommandResult::Created { node_id } => json!({
            "status": "created",
            "node_id": node_id,
        }),
        CardCommandResult::Deleted { node_id } => json!({
            "status": "deleted",
            "node_id": node_id,
        }),
        CardCommandResult::Sent { node_id } => json!({
            "status": "sent",
            "node_id": node_id,
        }),
        CardCommandResult::Focused { node_id } => json!({
            "status": "focused",
            "node_id": node_id,
        }),
        CardCommandResult::Listed { cards } => json!({
            "status": "listed",
            "cards": cards,
        }),
        CardCommandResult::AgentsListed { agents } => json!({
            "status": "agents_listed",
            "agents": agents,
        }),
        CardCommandResult::AgentCreated {
            definition_id,
            name,
            node_id,
        } => json!({
            "status": "agent_created",
            "definition_id": definition_id,
            "name": name,
            "node_id": node_id,
        }),
        CardCommandResult::AgentActivated {
            definition_id,
            name,
            node_id,
            reused,
        } => json!({
            "status": "agent_activated",
            "definition_id": definition_id,
            "name": name,
            "node_id": node_id,
            "reused": reused,
        }),
        CardCommandResult::AgentTasksDispatched { receipts } => json!({
            "status": "agent_tasks_dispatched",
            "receipts": receipts,
        }),
    };
    value.to_string()
}

fn parse_arguments_json(arguments_json: &str) -> Result<Value, LlmError> {
    serde_json::from_str(arguments_json)
        .map_err(|err| LlmError::Message(format!("invalid tool arguments JSON: {err}")))
}

fn parse_create(args: &Value) -> Result<CardCommand, LlmError> {
    let kind = required_str(args, "kind")?;
    let card_kind = match kind {
        "terminal" => CardKind::Terminal { shell: None },
        "agent" => {
            return Err(LlmError::Message(
                "generic Create cannot create Agent cards; use the Agent definition workflow"
                    .into(),
            ))
        }
        other => {
            return Err(LlmError::Message(format!("unknown create kind: {other}")));
        }
    };
    Ok(CardCommand::Create {
        kind: card_kind,
        placement: Placement::VisibleArea,
    })
}

fn parse_focus(args: &Value) -> Result<CardCommand, LlmError> {
    let node_id = required_u64(args, "node_id")?;
    Ok(CardCommand::Focus { node_id })
}

fn parse_send_text(args: &Value) -> Result<CardCommand, LlmError> {
    let node_id = required_u64(args, "node_id")?;
    let text = required_str(args, "text")?.to_string();
    let submit = args
        .get("submit")
        .map(parse_bool)
        .transpose()?
        .unwrap_or(true);
    Ok(CardCommand::SendText {
        node_id,
        text,
        submit,
    })
}

fn required_str<'a>(args: &'a Value, field: &str) -> Result<&'a str, LlmError> {
    args.get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| LlmError::Message(format!("missing or invalid {field}")))
}

fn required_u64(args: &Value, field: &str) -> Result<u64, LlmError> {
    args.get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| LlmError::Message(format!("missing or invalid {field}")))
}

fn parse_bool(value: &Value) -> Result<bool, LlmError> {
    value
        .as_bool()
        .ok_or_else(|| LlmError::Message("submit must be a boolean".into()))
}
