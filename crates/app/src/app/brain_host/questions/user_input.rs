//! Only an actual user turn can grant this capability; model tool arguments never reach this seam.
use super::*;

const DELEGATION_PHRASES: [&str; 5] = [
    "这题你直接回答",
    "这题你来回答",
    "这题帮我回答",
    "这个问题你直接回答",
    "当前问题你直接回答",
];

fn explicit_delegation(text: &str) -> bool {
    let text = text.trim().trim_end_matches(['。', '！', '.', '!']).trim();
    DELEGATION_PHRASES.contains(&text)
}

impl MultisoulApp {
    /// Candidate detection can only reject mixed input; it never grants permission.
    pub(crate) fn contains_question_delegation(text: &str) -> bool {
        DELEGATION_PHRASES
            .iter()
            .any(|phrase| text.contains(phrase))
    }

    pub(crate) fn is_question_delegation(text: &str) -> bool {
        explicit_delegation(text)
    }

    pub(crate) fn question_delegation_from_user(
        &mut self,
        text: &str,
    ) -> Option<Result<String, String>> {
        self.question_delegation_from_voice(text, None)
    }

    pub(crate) fn question_delegation_from_voice(
        &mut self,
        text: &str,
        cancellation: Option<tokio_util::sync::CancellationToken>,
    ) -> Option<Result<String, String>> {
        if !explicit_delegation(text) {
            return None;
        }
        Some((|| {
            if cancellation
                .as_ref()
                .is_some_and(|token| token.is_cancelled())
            {
                return Err("本次语音已取消，未委托代答。".into());
            }
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
                if let Some(grant) = self
                    .brain
                    .questions
                    .delegated
                    .iter_mut()
                    .find(|grant| grant.question.target == question.target)
                {
                    grant.cancellation = cancellation;
                }
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
