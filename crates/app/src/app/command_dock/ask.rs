//! `ask <agent|codex|claude…> <message>`：从 Dock 直接给 Agent 发消息，或新建卡片后发送。
use crate::app::MultisoulApp;
use crate::shortcuts::idea_assign::profile_from_key;
use gpui::{Context, Window};
use norma::protocol::{
    AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Ask<'a> {
    pub target: &'a str,
    pub message: &'a str,
}

/// 纯解析：`ask ` 前缀（不分大小写）+ 目标 + 非空消息；其余输入都不是 ask。
pub(super) fn parse(input: &str) -> Option<Ask<'_>> {
    let input = input.trim();
    let input = input.strip_prefix('/').unwrap_or(input);
    let head = input
        .get(..4)
        .filter(|head| head.eq_ignore_ascii_case("ask "))?;
    let rest = input[head.len()..].trim_start();
    let (target, message) = rest.split_once(char::is_whitespace)?;
    let message = message.trim();
    (!message.is_empty()).then_some(Ask { target, message })
}

impl MultisoulApp {
    pub(super) fn submit_dock_ask(
        &mut self,
        ask: Ask<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let area = self.mobile_canvas_area(Some(window));
        let zoom = self.runtime.document.viewport.zoom;
        let outcome = self.ask_agent(ask.target, ask.message, area, zoom);
        let Some(state) = self.command_dock.as_mut() else {
            return;
        };
        match outcome {
            Ok(name) => {
                state.reply = Some(ui::i18n::message("chrome.ask_sent", &[("agent", name)]));
                state.error = None;
                state.draft.clear();
                state.set_visible(false);
                let input = state.input.clone();
                input.update(cx, |input, cx| input.set_value(String::new(), window, cx));
            }
            Err(error) => state.error = Some(error),
        }
        cx.notify();
    }

    fn ask_agent(
        &mut self,
        target: &str,
        message: &str,
        area: canvas::CanvasRect,
        zoom: f32,
    ) -> Result<String, ui::i18n::LocalizedText> {
        let failed = |error: String| ui::i18n::message("chrome.ask_failed", &[("error", error)]);
        let source = agent::AgentDispatchSource::norma();
        let name = if let Some(profile) = profile_from_key(&target.to_lowercase()) {
            let created = crate::app::execute_high_level_agent_command(
                self,
                CardCommand::CreateAgent {
                    name: None,
                    profile,
                    activate: true,
                },
                area,
                zoom,
                source,
                None,
            )
            .map_err(|error| failed(error.to_string()))?;
            let CardCommandResult::AgentCreated { name, .. } = created else {
                return Err(failed("unexpected_result".into()));
            };
            name
        } else {
            self.agent_definitions
                .definition_by_address(target)
                .map(|definition| definition.name.clone())
                .ok_or_else(|| {
                    ui::i18n::message("chrome.ask_unknown_agent", &[("agent", target.to_owned())])
                })?
        };
        let result = crate::app::execute_high_level_agent_command(
            self,
            CardCommand::DispatchAgentTasks {
                assignments: vec![AgentTaskAssignment {
                    name: name.clone(),
                    task: message.to_owned(),
                }],
                force_untracked: false,
            },
            area,
            zoom,
            source,
            None,
        )
        .map_err(|error| failed(error.to_string()))?;
        tracing::info!(agent = %name, ?result, "dock ask dispatched");
        match result {
            CardCommandResult::AgentTasksDispatched { receipts }
                if receipts
                    .iter()
                    .all(|r| r.status != AgentTaskReceiptStatus::Failed) =>
            {
                self.push_brain(brain::BrainEvent::Goal {
                    agent: name.clone(),
                    text: message.to_owned(),
                });
                Ok(name)
            }
            _ => Err(failed(name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 场景：Dock 输入是否被识别为 ask。预期：只有 `ask 目标 非空消息` 命中。
    #[test]
    fn parses_only_complete_ask_commands() {
        assert_eq!(
            parse("ask codex fix bug"),
            Some(Ask {
                target: "codex",
                message: "fix bug"
            })
        );
        assert_eq!(
            parse("  ASK Simon  hi "),
            Some(Ask {
                target: "Simon",
                message: "hi"
            })
        );
        assert_eq!(parse("ask codex"), None);
        assert_eq!(parse("ask codex   "), None);
        assert_eq!(parse("asked codex hi"), None);
        assert_eq!(parse("请 ask codex hi"), None);
        assert_eq!(
            parse("/ask codex hi"),
            Some(Ask {
                target: "codex",
                message: "hi"
            })
        );
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/command_dock_ask_tests.rs"]
mod ask_tests;
