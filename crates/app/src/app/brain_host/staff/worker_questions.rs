//! Mirror the native question owner. Never independently claim or send a Worker answer.
use super::*;
use norma::protocol::{
    QuestionAnswer, QuestionRef, QuestionState, StaffQuestion, StaffQuestionState, TaskMessage,
};

impl MultisoulApp {
    pub(crate) fn start_staff_worker_answer(
        &mut self,
        target: &QuestionRef,
        answers: Vec<QuestionAnswer>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let permit = self.authorize_question_answer(target, answers.clone())?;
        self.begin_authorized_answer(permit, cx)?.detach();
        self.staff.worker_answers.retain(|(old, _)| old != target);
        self.staff.worker_answers.push((target.clone(), answers));
        Ok(())
    }

    pub(crate) fn mirror_staff_questions(&mut self, cx: &mut Context<Self>) {
        if self.staff.mirroring_questions || !self.staff.loaded {
            return;
        }
        let Some(scope) = self.brain_scope() else {
            return;
        };
        let Some(root) = config::state_dir() else {
            return;
        };
        let mut changes = Vec::new();
        let snapshots = self.runtime.questions();
        for task in self
            .staff
            .tasks
            .values()
            .filter(|task| task.workspace_id == scope.workspace_id)
        {
            for snapshot in &snapshots {
                let target = &snapshot.question.target;
                if task.phase != norma::protocol::TaskPhase::Done
                    && snapshot.state.blocks_input() && self.runtime.question_is_current(target)
                    && task.workers.iter().any(|worker| worker.card_id == Some(target.node_id) && self.runtime.staff_dispatch_id(target.node_id) == Some(worker.dispatch_id.as_str()))
                    && !task.messages.iter().any(|message| matches!(message, TaskMessage::Question {question} if question.worker.as_ref() == Some(target))) {
                    changes.push(MirrorChange::Open(task.id.clone(), StaffQuestion::new(
                        uuid::Uuid::new_v4().to_string(), snapshot.question.items.clone(), Some(target.clone()))));
                }
            }
            for message in &task.messages {
                let TaskMessage::Question { question } = message else {
                    continue;
                };
                let Some(target) = question.worker.as_ref() else {
                    continue;
                };
                if question.state != StaffQuestionState::Pending {
                    continue;
                }
                let native = self.runtime.question(target).map(|snapshot| snapshot.state);
                let Some(state) = mirror_terminal_state(native) else {
                    continue;
                };
                // Attribute answers only when the native owner confirms this host's delivery.
                let answers = if native == Some(QuestionState::Answered) {
                    self.staff
                        .worker_answers
                        .iter()
                        .find(|(old, _)| old == target)
                        .map(|(_, answers)| answers.clone())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                changes.push(MirrorChange::Settle(
                    task.id.clone(),
                    target.clone(),
                    state,
                    answers,
                ));
            }
        }
        if changes.is_empty() {
            return;
        }
        self.staff.mirroring_questions = true;
        let store = brain::staff::TaskStore::shared(root);
        let workspace = scope.workspace_id.clone();
        let work = cx.background_executor().spawn(async move {
            let mut tasks = Vec::new();
            let mut settled = Vec::new();
            for change in changes {
                let task = match change {
                    MirrorChange::Open(id, question) => {
                        store.add_question(&workspace, &id, question)
                    }
                    MirrorChange::Settle(id, target, state, answers) => {
                        let saved =
                            store.settle_worker_question(&workspace, &id, &target, state, &answers);
                        if saved.is_ok() {
                            settled.push(target);
                        }
                        saved
                    }
                };
                tasks.push(task);
            }
            (tasks, settled)
        });
        cx.spawn(async move |app, cx| {
            let (tasks, settled) = work.await;
            let _ = app.update(cx, |app, cx| {
                app.staff.mirroring_questions = false;
                if app.brain_scope().as_ref() != Some(&scope) {
                    return;
                }
                app.staff
                    .worker_answers
                    .retain(|(target, _)| !settled.contains(target));
                for task in tasks {
                    match task {
                        Ok(task) => app.receive_staff_tasks(vec![task], cx),
                        Err(error) => {
                            tracing::warn!(%error, "staff question mirror failed");
                        }
                    }
                }
            });
        })
        .detach();
    }
}

enum MirrorChange {
    Open(String, StaffQuestion),
    Settle(String, QuestionRef, StaffQuestionState, Vec<QuestionAnswer>),
}
fn mirror_terminal_state(state: Option<QuestionState>) -> Option<StaffQuestionState> {
    match state {
        Some(
            QuestionState::Answered
            | QuestionState::AnsweredElsewhere
            | QuestionState::ConsumedUnattributed,
        ) => Some(StaffQuestionState::Answered),
        Some(QuestionState::Cancelled | QuestionState::Expired) | None => {
            Some(StaffQuestionState::TimedOut)
        }
        _ => None,
    }
}
