use super::{missing, TaskStore};
use norma::protocol::{
    validate_question_answers, validate_question_items, QuestionAnswer, StaffQuestion,
    StaffQuestionState, Task, TaskMessage, TaskPhase,
};
use std::io;

impl TaskStore {
    pub fn add_question(
        &self,
        workspace: &str,
        id: &str,
        question: StaffQuestion,
    ) -> io::Result<Task> {
        validate_question_items(&question.items).map_err(invalid)?;
        if question.id.trim().is_empty() || question.state != StaffQuestionState::Pending {
            return Err(invalid("staff.question_invalid"));
        }
        if let Some(target) = &question.worker {
            target.validate().map_err(invalid)?;
        }
        self.update(workspace, id, |task| {
            if let Some(old) = task.messages.iter().find_map(|message| match message {
                TaskMessage::Question { question: old } if old.id == question.id => Some(old),
                _ => None,
            }) {
                return if old.items == question.items && old.worker == question.worker {
                    Ok(())
                } else {
                    Err(invalid("staff.question_identity_changed"))
                };
            }
            if task.phase == TaskPhase::Done
                || question.worker.as_ref().is_some_and(|target| {
                    !task
                        .workers
                        .iter()
                        .any(|worker| worker.card_id == Some(target.node_id))
                })
            {
                return Err(invalid("staff.question_unavailable"));
            }
            task.messages.push(TaskMessage::Question {
                question: Box::new(question),
            });
            task.updated_at = norma::protocol::timestamp();
            Ok(())
        })
    }

    /// Atomic first-writer claim. The live owner receives secrets before this durable projection.
    pub fn resolve_question(
        &self,
        workspace: &str,
        id: &str,
        question_id: &str,
        state: StaffQuestionState,
        answers: &[QuestionAnswer],
    ) -> io::Result<(Task, bool)> {
        if state == StaffQuestionState::Pending {
            return Err(invalid("staff.question_invalid"));
        }
        let mut resolved = false;
        let task = self.update(workspace, id, |task| {
            let question = task
                .messages
                .iter_mut()
                .find_map(|message| match message {
                    TaskMessage::Question { question } if question.id == question_id => {
                        Some(question)
                    }
                    _ => None,
                })
                .ok_or_else(missing)?;
            if question.state != StaffQuestionState::Pending {
                return Ok(());
            }
            if state == StaffQuestionState::Answered
                && question.expires_at <= norma::protocol::timestamp()
            {
                question.state = StaffQuestionState::TimedOut;
                return Ok(());
            }
            let text = if state == StaffQuestionState::Answered {
                validate_question_answers(&question.items, answers).map_err(invalid)?;
                let text = question.answer_text(answers);
                question.answers = question.public_answers(answers);
                Some(text)
            } else {
                None
            };
            question.state = state;
            resolved = true;
            if let Some(text) = text {
                task.user_message(&text);
            }
            task.updated_at = norma::protocol::timestamp();
            Ok(())
        })?;
        Ok((task, resolved))
    }

    /// Native delivery has its own exact-question claim and receipt; mirror its terminal state only.
    pub fn settle_worker_question(
        &self,
        workspace: &str,
        id: &str,
        target: &norma::protocol::QuestionRef,
        state: StaffQuestionState,
        answers: &[QuestionAnswer],
    ) -> io::Result<Task> {
        if state == StaffQuestionState::Pending {
            return Err(invalid("staff.question_invalid"));
        }
        self.update(workspace, id, |task| {
            let mut summary = None;
            for message in &mut task.messages {
                if let TaskMessage::Question { question } = message {
                    if question.worker.as_ref() == Some(target)
                        && question.state == StaffQuestionState::Pending
                    {
                        if state == StaffQuestionState::Answered && !answers.is_empty() {
                            validate_question_answers(&question.items, answers).map_err(invalid)?;
                            summary = Some(question.answer_text(answers));
                            question.answers = question.public_answers(answers);
                        }
                        question.state = state;
                        task.updated_at = norma::protocol::timestamp();
                    }
                }
            }
            if let Some(text) = summary {
                task.user_message(&text);
            }
            Ok(())
        })
    }
}

fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, reason)
}
