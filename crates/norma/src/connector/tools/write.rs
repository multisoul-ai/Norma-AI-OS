//! Bounded external requests. Internal node addressing never enters the MCP schema.
use super::super::CardSigner;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchTask {
    pub operation_id: String,
    pub card_id: String,
    pub card_revision: String,
    pub task: String,
    pub context: Option<String>,
    #[serde(default = "yes")]
    pub allow_queue: bool,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelTask {
    pub operation_id: String,
    pub task_id: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyAgent {
    pub operation_id: String,
    pub card_id: String,
    pub card_revision: String,
    pub message: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCard {
    pub operation_id: String,
    pub workspace_id: String,
    pub area_id: String,
    pub profile: String,
    pub task: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub enum WriteRequest {
    Dispatch(DispatchTask),
    Cancel(CancelTask),
    Reply(ReplyAgent),
    Create(CreateCard),
}
impl WriteRequest {
    pub fn parse(name: &str, input: Value, signer: &CardSigner) -> Result<Self, &'static str> {
        let mut request = match name {
            "dispatch_task" => {
                Self::Dispatch(serde_json::from_value(input).map_err(|_| "invalid_argument")?)
            }
            "cancel_task" => {
                Self::Cancel(serde_json::from_value(input).map_err(|_| "invalid_argument")?)
            }
            "reply_agent" => {
                Self::Reply(serde_json::from_value(input).map_err(|_| "invalid_argument")?)
            }
            "create_card" => {
                Self::Create(serde_json::from_value(input).map_err(|_| "invalid_argument")?)
            }
            _ => return Err("invalid_argument"),
        };
        id(request.operation_id(), 128)?;
        match &mut request {
            Self::Dispatch(args) => {
                signer.verify(&args.card_id)?;
                id(&args.card_revision, 256)?;
                text(&mut args.task)?;
                if let Some(context) = &mut args.context {
                    text(context)?;
                    if args.task.chars().count() + context.chars().count() + 2 > 8000 {
                        return Err("task_too_long");
                    }
                }
            }
            Self::Reply(args) => {
                signer.verify(&args.card_id)?;
                id(&args.card_revision, 256)?;
                text(&mut args.message)?;
            }
            Self::Cancel(args) => id(&args.task_id, 4096)?,
            Self::Create(args) => {
                id(&args.workspace_id, 128)?;
                id(&args.area_id, 128)?;
                if !matches!(args.profile.as_str(), "claude" | "codex") {
                    return Err("profile_unavailable");
                }
                if let Some(task) = &mut args.task {
                    text(task)?;
                }
            }
        }
        Ok(request)
    }
    pub fn operation_id(&self) -> &str {
        match self {
            Self::Dispatch(a) => &a.operation_id,
            Self::Cancel(a) => &a.operation_id,
            Self::Reply(a) => &a.operation_id,
            Self::Create(a) => &a.operation_id,
        }
    }
    pub fn tool(&self) -> &'static str {
        match self {
            Self::Dispatch(_) => "dispatch_task",
            Self::Cancel(_) => "cancel_task",
            Self::Reply(_) => "reply_agent",
            Self::Create(_) => "create_card",
        }
    }
    pub fn route(&self) -> &str {
        match self {
            Self::Dispatch(a) => &a.card_id,
            Self::Cancel(a) => &a.task_id,
            Self::Reply(a) => &a.card_id,
            Self::Create(a) => &a.area_id,
        }
    }
    pub fn fingerprint(&self) -> String {
        hex::encode(Sha256::digest(
            serde_json::to_vec(&(self.tool(), self)).expect("normalized request"),
        ))
    }
    pub fn task_text(&self) -> Option<String> {
        match self {
            Self::Dispatch(a) => Some(
                a.context
                    .as_ref()
                    .map_or_else(|| a.task.clone(), |c| format!("{}\n\n{c}", a.task)),
            ),
            Self::Create(a) => a.task.clone(),
            _ => None,
        }
    }
}
fn id(value: &str, max: usize) -> Result<(), &'static str> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        Err("invalid_argument")
    } else {
        Ok(())
    }
}
fn text(value: &mut String) -> Result<(), &'static str> {
    value.retain(|c| !c.is_control() || c == '\n' || c == '\t');
    if value.trim().is_empty() {
        Err("invalid_argument")
    } else if value.chars().count() > 8000 {
        Err("task_too_long")
    } else {
        Ok(())
    }
}
pub fn is_write(name: &str) -> bool {
    matches!(
        name,
        "dispatch_task" | "cancel_task" | "reply_agent" | "create_card"
    )
}
