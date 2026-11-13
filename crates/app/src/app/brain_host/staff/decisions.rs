use super::*;
use crate::app::staff::error_message;
use norma::protocol::{QuestionAnswer, StaffQuestionState, Task};

impl MultisoulApp {
    pub(crate) fn set_staff_proposal_ignored(
        &mut self,
        task_id: String,
        proposal_id: u64,
        ignored: bool,
        cx: &mut Context<Self>,
    ) {
        if self.staff.busy {
            return;
        }
        let result = (|| {
            let store = self.staff_store()?;
            let scope = self.brain_scope().ok_or("island.staff_no_workspace")?;
            Ok(cx.background_executor().spawn(async move {
                store
                    .set_proposal_ignored(&scope.workspace_id, &task_id, proposal_id, ignored)
                    .map(|task| (task, false))
                    .map_err(|e| e.to_string())
            }))
        })();
        self.complete_staff_decision(result, cx);
    }

    pub(crate) fn complete_staff_decision(
        &mut self,
        result: Result<gpui::Task<Result<(Task, bool), String>>, String>,
        cx: &mut Context<Self>,
    ) {
        let work = match result {
            Ok(work) => work,
            Err(error) => {
                self.staff.error = Some(error_message(error));
                self.notify_staff(cx);
                return;
            }
        };
        let scope = self.brain_scope();
        self.staff.busy = true;
        self.staff.error = None;
        self.notify_staff(cx);
        cx.spawn(async move |app, cx| {
            let result = work.await;
            let _ = app.update(cx, |app, cx| {
                if app.brain_scope() != scope {
                    return;
                }
                app.staff.busy = false;
                match result {
                    Ok((task, wake)) => {
                        let task_id = task.id.clone();
                        app.receive_staff_tasks(vec![task], cx);
                        if wake {
                            app.push_brain(brain::BrainEventKind::TaskRequest { task_id });
                        }
                    }
                    Err(error) => app.staff.error = Some(error_message(error)),
                }
                app.notify_staff(cx);
            });
        })
        .detach();
    }
    pub(crate) fn save_staff_answer(
        &mut self,
        task_id: String,
        question_id: String,
        skip: bool,
        answers: Vec<QuestionAnswer>,
        cx: &mut Context<Self>,
    ) {
        let target = self.staff.tasks.get(&task_id).and_then(|task| {
            task.messages.iter().find_map(|message| match message {
                norma::protocol::TaskMessage::Question { question }
                    if question.id == question_id =>
                {
                    question.worker.clone()
                }
                _ => None,
            })
        });
        if let Some(target) = target {
            if !skip {
                match self.start_staff_worker_answer(&target, answers, cx) {
                    Ok(()) => self.notify_staff(cx),
                    Err(error) => {
                        self.staff.error = Some(error_message(error));
                        self.notify_staff(cx);
                    }
                }
                return;
            }
            if let Err(error) = self.runtime.release_question_to_native(&target) {
                self.staff.error = Some(error_message(error.to_owned()));
                self.notify_staff(cx);
                return;
            }
        }
        let result = (|| {
            let store = self.staff_store()?;
            let scope = self.brain_scope().ok_or("island.staff_no_workspace")?;
            let work = cx.background_executor().spawn(async move {
                store
                    .resolve_question(
                        &scope.workspace_id,
                        &task_id,
                        &question_id,
                        if skip {
                            StaffQuestionState::Skipped
                        } else {
                            StaffQuestionState::Answered
                        },
                        &answers,
                    )
                    .map(|(task, claimed)| (task, claimed && !skip))
                    .map_err(|e| e.to_string())
            });
            Ok(work)
        })();
        self.complete_staff_decision(result, cx);
    }
}
