//! Only an actual user turn can grant this capability; model tool arguments never reach this seam.
use super::*;

fn explicit_delegation(text: &str) -> bool {
    let text = text.trim().trim_end_matches(['。', '！', '.', '!']).trim();
    matches!(
        text,
        "这题你直接回答"
            | "这题你来回答"
            | "这题帮我回答"
            | "这个问题你直接回答"
            | "当前问题你直接回答"
    )
}

impl MultisoulApp {
    pub(crate) fn question_delegation_from_user(
        &mut self,
        text: &str,
    ) -> Option<Result<String, String>> {
        if !explicit_delegation(text) {
            return None;
        }
        Some((|| {
            if self.brain.host.is_none() {
                return Err("幕僚没有运行，暂时无法代答；可以在原卡片回答。".into());
            }
            let chosen = match self.runtime.focus.mode() {
                runtime::InputMode::NodeFocused(node) => Some(node),
                _ if self.runtime.selection.len() == 1 => self.runtime.selection.iter().next(),
                _ if self.runtime.selection.len() > 1 => {
                    return Err("请先选中要回答的卡片，再委托这道题。".into())
                }
                _ => None,
            };
            let mut questions = self.runtime.questions().into_iter().filter(|snapshot| {
                snapshot.state.blocks_input()
                    && self.runtime.question_is_current(&snapshot.question.target)
                    && chosen.is_none_or(|node| node == snapshot.question.target.node_id)
            });
            let question = questions
                .next()
                .ok_or("当前卡片没有可代答的问题。")?
                .question;
            if questions.next().is_some() {
                return Err("当前有多道待答问题，请先选中要回答的卡片。".into());
            }
            if self.authorize_question_delegation(&question.target)? {
                self.push_brain(brain::BrainEvent::QuestionRequested {
                    agent: self
                        .agent_announcement_name(question.target.node_id)
                        .unwrap_or_else(|| format!("{:?}", question.target.provider)),
                    question,
                    delegated: true,
                });
            }
            Ok("已委托幕僚回答当前这道题，提交后会核对原 Agent 的消费结果。".into())
        })())
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/brain_question_user_input_tests.rs"]
mod tests;
