use super::{AdapterCommand, AdapterError, IdeaTarget};
use norma::protocol::AgentProfile;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureIdeaArgs {
    body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

/// 目标字段平铺：OpenAI 兼容端不接受根上的 oneOf，互斥在这里校验。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignIdeaArgs {
    idea_id: String,
    workspace_id: String,
    area_id: String,
    node_id: Option<u64>,
    new_agent_profile: Option<String>,
}

pub(super) fn parse_idea_command(
    name: &str,
    arguments: Value,
) -> Option<Result<AdapterCommand, AdapterError>> {
    Some(match name {
        "capture_idea" => capture(arguments),
        "list_ideas" => parse::<EmptyArgs>(arguments).map(|_| AdapterCommand::ListIdeas),
        "assign_idea" => assign(arguments),
        _ => return None,
    })
}

fn capture(arguments: Value) -> Result<AdapterCommand, AdapterError> {
    let args: CaptureIdeaArgs = parse(arguments)?;
    Ok(AdapterCommand::CaptureIdea {
        body: text("body", args.body)?,
    })
}

fn assign(arguments: Value) -> Result<AdapterCommand, AdapterError> {
    let args: AssignIdeaArgs = parse(arguments)?;
    let workspace_id = text("workspace_id", args.workspace_id)?;
    let area_id = text("area_id", args.area_id)?;
    let target = match (args.node_id, args.new_agent_profile) {
        (Some(node_id), None) => {
            if node_id == 0 {
                return Err(invalid("node_id must be greater than zero"));
            }
            IdeaTarget::Agent {
                workspace_id,
                area_id,
                node_id,
            }
        }
        (None, Some(profile)) => IdeaTarget::NewAgent {
            workspace_id,
            area_id,
            profile: profile_from(&profile)?,
        },
        _ => {
            return Err(invalid(
                "target must be exactly one of node_id or new_agent_profile",
            ))
        }
    };
    Ok(AdapterCommand::AssignIdea {
        idea_id: text("idea_id", args.idea_id)?,
        target,
    })
}

fn profile_from(value: &str) -> Result<AgentProfile, AdapterError> {
    Ok(match value {
        "codex" => AgentProfile::Codex,
        "claude" => AgentProfile::Claude,
        "opencode" => AgentProfile::OpenCode,
        "infcode" => AgentProfile::InfCode,
        "cursor" => AgentProfile::Cursor,
        "grok" => AgentProfile::Grok,
        "kimi" => AgentProfile::Kimi,
        "zcode" => AgentProfile::ZCode,
        "pi" => AgentProfile::Pi,
        "hermes" => AgentProfile::Hermes,
        other => return Err(invalid(&format!("unsupported Agent profile: {other}"))),
    })
}

fn parse<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, AdapterError> {
    serde_json::from_value(value).map_err(|error| invalid(&error.to_string()))
}

fn text(field: &str, value: String) -> Result<String, AdapterError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(invalid(&format!("{field} must not be empty")));
    }
    Ok(trimmed.to_string())
}

fn invalid(message: &str) -> AdapterError {
    AdapterError::InvalidArguments(message.to_string())
}
