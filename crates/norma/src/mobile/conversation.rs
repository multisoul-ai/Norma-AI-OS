use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConversationRequest {
    pub workspace_id: String,
    pub route_id: String,
    pub node_id: u64,
    pub run_id: String,
    pub session_id: String,
    pub cursor: Option<String>,
}

impl ConversationRequest {
    pub fn valid(&self) -> bool {
        self.node_id > 0
            && [
                &self.workspace_id,
                &self.route_id,
                &self.run_id,
                &self.session_id,
            ]
            .into_iter()
            .all(|s| {
                !s.is_empty()
                    && s.len() <= 128
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            })
            && self.cursor.as_ref().is_none_or(|value| value.len() <= 4096)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConversationItem {
    pub id: String,
    pub role: String,
    pub text: String,
    /// Full tool/attachment record, explicitly expandable rather than silently discarded.
    pub details: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConversationPage {
    pub items: Vec<ConversationItem>,
    pub cursor: Option<String>,
    pub has_more: bool,
    pub pending_tail: bool,
    /// Codex paginates older turns; Claude continues forward through complete JSONL records.
    pub direction: String,
}

impl ConversationPage {
    pub fn from_codex(raw: Value) -> Result<Self, String> {
        let turns = raw["data"]
            .as_array()
            .ok_or("Codex history missing turns")?;
        let mut items = Vec::new();
        for turn in turns.iter().rev() {
            if turn.get("itemsView").is_some_and(|view| view != "full") {
                return Err("Codex returned a summary instead of full history".into());
            }
            let turn_id = turn["id"].as_str().ok_or("Codex turn missing identity")?;
            for item in turn["items"]
                .as_array()
                .ok_or("Codex history missing items")?
            {
                let id = item["id"].as_str().ok_or("Codex item missing identity")?;
                let (role, text, details) = match item["type"].as_str() {
                    Some("agentMessage") => (
                        "assistant",
                        item["text"]
                            .as_str()
                            .ok_or("Codex reply missing body")?
                            .into(),
                        None,
                    ),
                    Some("userMessage") => {
                        let (text, details) = content(&item["content"])?;
                        ("user", text, details)
                    }
                    Some(_) => ("tool", String::new(), Some(item.to_string())),
                    None => return Err("Codex item missing type".into()),
                };
                items.push(ConversationItem {
                    id: format!("{turn_id}:{id}"),
                    role: role.into(),
                    text,
                    details,
                });
            }
        }
        let cursor = match raw.get("nextCursor") {
            Some(Value::String(value)) => {
                Some(serde_json::to_string(value).map_err(|e| e.to_string())?)
            }
            Some(Value::Null) => None,
            _ => return Err("Codex history missing cursor".into()),
        };
        let page = Self {
            items,
            has_more: cursor.is_some(),
            cursor,
            pending_tail: false,
            direction: "older".into(),
        };
        page.validate()?;
        Ok(page)
    }

    pub fn from_claude(raw: crate::claude::TranscriptPage) -> Result<Self, String> {
        let mut items = Vec::new();
        for message in raw.messages {
            if !matches!(message.role.as_str(), "user" | "assistant") {
                return Err("Claude history contains an unknown message role".into());
            }
            let (text, details) = content(&message.content)?;
            items.push(ConversationItem {
                id: message.id,
                role: if text.is_empty() && details.is_some() {
                    "tool".into()
                } else {
                    message.role
                },
                text,
                details,
            });
        }
        let page = Self {
            items,
            cursor: Some(serde_json::to_string(&raw.cursor).map_err(|e| e.to_string())?),
            has_more: raw.has_more,
            pending_tail: raw.pending_tail,
            direction: "newer".into(),
        };
        page.validate()?;
        Ok(page)
    }

    pub fn from_cursor(raw: crate::cursor::TranscriptPage) -> Result<Self, String> {
        let mut items = Vec::new();
        for message in raw.messages {
            if !matches!(message.role.as_str(), "user" | "assistant") {
                return Err("Cursor history contains an unknown message role".into());
            }
            let (text, details) = content(&message.content)?;
            items.push(ConversationItem {
                id: message.id,
                role: if text.is_empty() && details.is_some() {
                    "tool".into()
                } else {
                    message.role
                },
                text,
                details,
            });
        }
        let page = Self {
            items,
            cursor: Some(serde_json::to_string(&raw.cursor).map_err(|error| error.to_string())?),
            has_more: raw.has_more,
            pending_tail: raw.pending_tail,
            direction: "newer".into(),
        };
        page.validate()?;
        Ok(page)
    }

    fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        if self
            .items
            .iter()
            .any(|item| item.id.is_empty() || item.id.len() > 512 || !ids.insert(&item.id))
        {
            return Err("Conversation contains invalid or repeated message identities".into());
        }
        Ok(())
    }
}

fn content(value: &Value) -> Result<(String, Option<String>), String> {
    if let Some(text) = value.as_str() {
        return Ok((text.into(), None));
    }
    let blocks = value
        .as_array()
        .ok_or("Conversation body is neither text nor blocks")?;
    let mut text = Vec::new();
    let mut other = Vec::new();
    for block in blocks {
        if block["type"] == "text" {
            text.push(block["text"].as_str().ok_or("Text block missing content")?);
        } else {
            other.push(block);
        }
    }
    Ok((
        text.join("\n\n"),
        (!other.is_empty()).then(|| serde_json::json!(other).to_string()),
    ))
}
