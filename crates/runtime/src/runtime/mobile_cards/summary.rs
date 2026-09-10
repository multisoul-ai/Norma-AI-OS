use norma::agent_hook::{observe_conversation, AgentHookEvent, ConversationObservationKind};
use serde_json::Value;

/// One bounded excerpt of observed work, not an invented completion or a second LLM request.
#[derive(Default)]
pub(super) struct TaskSummary {
    turn: Option<String>,
    text: Option<String>,
}

impl TaskSummary {
    pub(super) fn snapshot(&self) -> Option<String> {
        self.text.clone()
    }

    fn start(&mut self, turn: Option<&str>) {
        if turn.is_none() || self.turn.as_deref() != turn {
            self.turn = turn.map(str::to_owned);
            self.text = None;
        }
    }

    // Called only after the Runtime accepts the original node/run/session/turn.
    pub(super) fn observe_hook(&mut self, event: &AgentHookEvent) {
        match event.provider_event.as_str() {
            "SessionStart" => *self = Self::default(),
            "UserPromptSubmit" => self.start(event.turn_id.as_deref()),
            _ if self.turn.is_some() && self.turn == event.turn_id => {
                match event.provider_event.as_str() {
                    "PreToolUse" | "PermissionRequest" => {
                        // Provider-supplied human description only; never copy command bodies,
                        // file contents, tool results or a subagent's output into the task card.
                        self.text = event.raw_json["tool_input"]["description"]
                            .as_str()
                            .and_then(bounded_excerpt);
                    }
                    "Stop" => {
                        self.text = observe_conversation(event)
                            .filter(|o| o.kind == ConversationObservationKind::CompleteAgentReply)
                            .and_then(|o| o.text)
                            .as_deref()
                            .and_then(bounded_excerpt);
                    }
                    "StopFailure" => self.text = event.message.as_deref().and_then(bounded_excerpt),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Controlled Codex already receives these records. Native Codex continues using its Hooks.
    pub(super) fn observe_codex(&mut self, method: &str, params: &Value) -> bool {
        if !matches!(method, "turn/started" | "turn/completed" | "item/completed") {
            return false;
        }
        let previous = self.text.clone();
        if method == "turn/started" {
            self.start(params["turn"]["id"].as_str());
        } else if method == "turn/completed"
            && self.turn.is_some()
            && params["turn"]["id"].as_str() == self.turn.as_deref()
        {
            if params["turn"]["status"] == "failed" {
                self.text = params["turn"]["error"]["message"]
                    .as_str()
                    .and_then(bounded_excerpt);
            }
            self.turn = None;
        } else if method == "item/completed"
            && self.turn.is_some()
            && params["turnId"].as_str() == self.turn.as_deref()
            && params["item"]["type"] == "agentMessage"
        {
            self.text = params["item"]["text"].as_str().and_then(bounded_excerpt);
        }
        previous != self.text
    }
}

fn bounded_excerpt(raw: &str) -> Option<String> {
    // ponytail: at most 4096 source scalars inspected and 240 retained (<=960 UTF-8 bytes).
    // This is a preview; full text remains in the existing conversation, with no new polling/IO.
    let mut text = String::new();
    let mut count = 0;
    let mut spacing = false;
    for c in raw.chars().take(4096) {
        if c.is_whitespace() {
            spacing = !text.is_empty();
            continue;
        }
        if c.is_control() {
            continue;
        }
        if spacing {
            text.push(' ');
            count += 1;
            spacing = false;
        }
        text.push(c);
        count += 1;
        if count > 240 {
            // Retain 239 scalars + one ellipsis; an exact 240-scalar reply remains intact.
            while count > 239 {
                text.pop();
                count -= 1;
            }
            text.push('…');
            break;
        }
    }
    (!text.is_empty()).then_some(text)
}
