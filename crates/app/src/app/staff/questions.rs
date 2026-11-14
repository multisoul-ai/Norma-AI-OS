use super::*;
use gpui::Window;
use norma::protocol::{StaffQuestionState, TaskMessage};

impl MultisoulApp {
    pub(crate) fn staff_question_status(
        &self,
        question: &norma::protocol::StaffQuestion,
    ) -> &'static str {
        use norma::protocol::{QuestionState as Native, StaffQuestionState as Card};
        match question.state {
            Card::Answered => "staff.question_answered",
            Card::Skipped => "staff.question_skipped",
            Card::TimedOut => "staff.question_timed_out",
            Card::Pending => match question.worker.as_ref() {
                None => "staff.question_pending",
                Some(target) => {
                    match self
                        .runtime
                        .question_snapshot(target)
                        .map(|snapshot| snapshot.state)
                    {
                        Some(Native::Submitting) => "chrome.question_submitting",
                        Some(Native::Submitted) => "chrome.question_submitted",
                        _ if self.runtime.native_question_available(target) => {
                            "staff.question_pending"
                        }
                        _ => "chrome.question_native_only",
                    }
                }
            },
        }
    }

    pub(super) fn ensure_staff_question_editors(
        &mut self,
        task: &Task,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for message in &task.messages {
            if let TaskMessage::Question { question } = message {
                if question.state == StaffQuestionState::Pending {
                    if !self.staff.question_editors.contains_key(&question.id) {
                        let editor = crate::workspace::desktop::QuestionEditor::from_items(
                            &question.items,
                            window,
                            cx,
                        );
                        let subscriptions = (0..editor.item_count())
                            .map(|index| {
                                cx.subscribe(editor.input(index), |app, _, event, cx| {
                                    if matches!(event, ui::component::input::InputEvent::Change) {
                                        app.notify_staff(cx);
                                    }
                                })
                            })
                            .collect();
                        self.staff
                            .question_subscriptions
                            .insert(question.id.clone(), subscriptions);
                        self.staff
                            .question_editors
                            .insert(question.id.clone(), editor);
                    }
                } else {
                    self.staff.question_editors.remove(&question.id);
                    self.staff.question_subscriptions.remove(&question.id);
                }
            }
        }
    }

    pub(super) fn answer_staff_question(
        &mut self,
        task_id: String,
        question_id: String,
        skip: bool,
        cx: &mut Context<Self>,
    ) {
        if self.staff.busy {
            return;
        }
        let answers = if skip {
            Vec::new()
        } else {
            let Some(editor) = self.staff.question_editors.get(&question_id) else {
                return;
            };
            editor.answers(cx)
        };
        self.save_staff_answer(task_id, question_id, skip, answers, cx);
    }
}
