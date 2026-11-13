//! Durable question cards. Live delivery remains owned by ACP / the native question runtime.
use super::{QuestionAnswer, QuestionItem, QuestionRef, Task, TaskMessage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum StaffQuestionState {
    Pending,
    Answered,
    Skipped,
    #[default]
    TimedOut,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct StaffQuestion {
    pub id: String,
    pub items: Vec<QuestionItem>,
    pub state: StaffQuestionState,
    /// Secret values are consumed by the live owner and never copied to this snapshot.
    pub answers: Vec<QuestionAnswer>,
    pub worker: Option<QuestionRef>,
    pub expires_at: u64,
}

impl StaffQuestion {
    pub fn new(id: String, items: Vec<QuestionItem>, worker: Option<QuestionRef>) -> Self {
        Self {
            id,
            items,
            worker,
            state: StaffQuestionState::Pending,
            expires_at: super::timestamp() + crate::agent_hook::QUESTION_TIMEOUT.as_secs(),
            answers: Vec::new(),
        }
    }

    pub fn public_answers(&self, answers: &[QuestionAnswer]) -> Vec<QuestionAnswer> {
        answers
            .iter()
            .cloned()
            .map(|mut answer| {
                if self
                    .items
                    .iter()
                    .any(|item| item.key == answer.key && item.secret)
                {
                    answer.selected.clear();
                    answer.text = None;
                }
                answer
            })
            .collect()
    }

    /// User-provided prompts and labels stay in their original language; secrets stay off the transcript.
    pub fn answer_text(&self, answers: &[QuestionAnswer]) -> String {
        self.items
            .iter()
            .filter_map(|item| {
                let answer = answers.iter().find(|answer| answer.key == item.key)?;
                let value = if item.secret {
                    "••••".to_owned()
                } else {
                    let mut values: Vec<_> = answer
                        .selected
                        .iter()
                        .filter_map(|key| {
                            item.options
                                .iter()
                                .find(|option| option.key == *key)
                                .map(|option| option.label.clone())
                        })
                        .collect();
                    values.extend(answer.text.iter().cloned());
                    values.join(", ")
                };
                Some(format!("{}\n{value}", item.prompt))
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl Task {
    pub fn pending_question_count(&self) -> usize {
        self.messages.iter().filter(|message| {
            matches!(message, TaskMessage::Question { question } if question.state == StaffQuestionState::Pending)
        }).count()
    }

    pub fn expire_pending_questions(&mut self) -> bool {
        let mut changed = false;
        for message in &mut self.messages {
            if let TaskMessage::Question { question } = message {
                if question.state == StaffQuestionState::Pending {
                    question.state = StaffQuestionState::TimedOut;
                    changed = true;
                }
            }
        }
        changed
    }
}
