//! Host-owned question authorization and asynchronous durable delivery.
use crate::app::{shared_context::SharedContext, MultisoulApp};
use gpui::{Context, Task};
use norma::protocol::{
    ContextOperation, ContextProject, ContextScope, PendingQuestion, QuestionAnswer, QuestionRef,
    QuestionSnapshot, QuestionState,
};
use runtime::runtime::QuestionSubmission;
use std::time::{Duration, Instant};
use storage::question_delivery_store::{
    QuestionDeliveryDraft, QuestionDeliveryLease, QuestionDeliveryReservation,
    QuestionDeliveryState, QuestionDeliveryStore,
};
mod delegation;
mod overview;
mod receipts;
mod user_input;

#[cfg(test)]
#[path = "../../../tests/unit/brain_question_send_guard_tests.rs"]
mod send_guard_tests;

#[derive(Default)]
pub(super) struct QuestionHost {
    suggestions: Vec<AnswerSuggestion>,
    delegated: Vec<delegation::AnswerDelegation>,
    announced: Vec<QuestionRef>,
    announced_token: String,
    receipts: Vec<(QuestionSubmission, QuestionDeliveryLease)>,
    ready_receipts: Vec<(QuestionDeliveryLease, QuestionDeliveryState, String)>,
    prepared: Vec<(QuestionSubmission, QuestionDeliveryLease)>,
    ui_errors: Vec<(QuestionRef, String)>,
    receipt_write_pending: bool,
    receipt_retry_at: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct AnswerSuggestion {
    pub target: QuestionRef,
    pub answers: Vec<QuestionAnswer>,
    pub reason: String,
}

/// Only the owning UI/user-input handler can construct this; it is never deserialized from CLI.
pub(crate) struct AuthorizedAnswer {
    question: PendingQuestion,
    answers: Vec<QuestionAnswer>,
    scope: ContextScope,
    deadline: Instant,
    cancellation: Option<tokio_util::sync::CancellationToken>,
}

impl MultisoulApp {
    fn question_context(&mut self, target: &QuestionRef) -> Result<SharedContext, String> {
        let service = self
            .shared_context_service()
            .map_err(|error| error.to_string())?;
        let project = match &service.scope().project {
            ContextProject::Project { project_id, .. } => Some(project_id.as_str()),
            ContextProject::Personal => self.workspace_id.as_deref(),
        };
        if project != Some(target.project_id.as_str())
            || self
                .runtime
                .agent_hook_config()
                .is_none_or(|config| config.workspace_id != target.workspace_id)
        {
            return Err("question_scope_changed".into());
        }
        Ok(service)
    }

    pub(crate) fn propose_question_answer(
        &mut self,
        target: QuestionRef,
        answers: Vec<QuestionAnswer>,
        reason: String,
    ) -> Result<(), String> {
        self.question_context(&target)?;
        let current = self.runtime.question(&target).ok_or("question_not_found")?;
        if current.state != QuestionState::Pending {
            return Err("question_already_handled".into());
        }
        if current.question.items.iter().any(|item| item.secret) {
            return Err("question_requires_private_answer".into());
        }
        current.question.validate_answers(&answers)?;
        if reason.trim().is_empty() || reason.len() > 4096 {
            return Err("invalid_answer_reason".into());
        }
        self.brain.questions.suggestions.retain(|suggestion| {
            suggestion.target != target
                && self
                    .runtime
                    .question(&suggestion.target)
                    .is_some_and(|question| question.state == QuestionState::Pending)
        });
        self.brain.questions.suggestions.push(AnswerSuggestion {
            target,
            answers,
            reason,
        });
        Ok(())
    }

    /// Called by the user's adopt/edit action, never by a model-supplied origin flag.
    pub(crate) fn authorize_question_answer(
        &mut self,
        target: &QuestionRef,
        answers: Vec<QuestionAnswer>,
    ) -> Result<AuthorizedAnswer, String> {
        let service = self.question_context(target)?;
        if !self.runtime.native_question_available(target) {
            return Err("question_not_available".into());
        }
        let question = self
            .runtime
            .question(target)
            .ok_or("question_not_found")?
            .question
            .clone();
        question.validate_answers(&answers)?;
        Ok(AuthorizedAnswer {
            question,
            answers,
            scope: service.scope().clone(),
            deadline: Instant::now() + Duration::from_secs(120),
            cancellation: None,
        })
    }

    pub(crate) fn answer_question_from_brain(
        &mut self,
        target: QuestionRef,
        answers: Vec<QuestionAnswer>,
        cx: &mut Context<Self>,
    ) -> Result<QuestionSnapshot, String> {
        let permit = self.consume_question_delegation(&target, answers)?;
        self.begin_authorized_answer(permit, cx)?.detach();
        self.runtime
            .question_snapshot(&target)
            .ok_or("question_not_found".into())
    }

    pub(crate) fn begin_authorized_answer(
        &mut self,
        permit: AuthorizedAnswer,
        cx: &mut Context<Self>,
    ) -> Result<Task<()>, String> {
        let target = &permit.question.target;
        let service = self.question_context(target)?;
        if service.scope() != &permit.scope
            || Instant::now() >= permit.deadline
            || permit
                .cancellation
                .as_ref()
                .is_some_and(|token| token.is_cancelled())
            || self
                .runtime
                .question(target)
                .is_none_or(|snapshot| snapshot.question != permit.question)
        {
            return Err("question_authorization_expired".into());
        }
        let retained = self
            .brain
            .questions
            .prepared
            .iter()
            .find(|(submission, _)| submission.target() == target)
            .cloned();
        let (submission, retained_lease) = if let Some((submission, lease)) = retained {
            if submission.answers() != permit.answers {
                return Err(
                    "question_prepared_answer_changed：此答案已准备，请采用原答案或在原卡片处理"
                        .into(),
                );
            }
            self.runtime
                .retry_native_question_preparation(&submission)?;
            (submission, Some(lease))
        } else {
            (
                self.runtime
                    .claim_question_answer(
                        target,
                        permit.answers.clone(),
                        &uuid::Uuid::new_v4().to_string(),
                    )?
                    .ok_or("question_already_handled")?,
                None,
            )
        };
        let draft = QuestionDeliveryDraft {
            delivery_id: submission.delivery_id().into(),
            target: serde_json::to_value(target).map_err(|error| error.to_string())?,
            question: serde_json::to_value(&permit.question).map_err(|error| error.to_string())?,
            answers: serde_json::to_value(&permit.answers).map_err(|error| error.to_string())?,
        };
        let store = QuestionDeliveryStore::new(self.database_paths.app_db_path().to_path_buf());
        cx.notify();
        let (completed, observer) = async_channel::bounded(1);
        // The host owns the authorized operation. Dropping the returned observer cannot cancel it.
        cx.spawn(async move |this, cx| {
            let prepared = match service.snapshot().await {
                Ok(_) if retained_lease.is_some() => Ok(QuestionDeliveryReservation::Claimed(
                    retained_lease.expect("checked retained lease"),
                )),
                Ok(_) => {
                    cx.background_executor()
                        .spawn(
                            async move { store.reserve(&draft).map_err(|error| error.to_string()) },
                        )
                        .await
                }
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update(cx, |app, cx| {
                let result =
                    app.finish_question_preparation(&permit, &submission, &service, prepared);
                if let Err(error) = result {
                    let _ = app.runtime.fail_question_preparation(&submission, &error);
                }
                cx.notify();
            });
            let _ = completed.send(()).await;
        })
        .detach();
        Ok(cx.spawn(async move |_, _| {
            let _ = observer.recv().await;
        }))
    }

    fn finish_question_preparation(
        &mut self,
        permit: &AuthorizedAnswer,
        submission: &QuestionSubmission,
        service: &SharedContext,
        prepared: Result<QuestionDeliveryReservation, String>,
    ) -> Result<(), String> {
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                if !self
                    .brain
                    .questions
                    .receipts
                    .iter()
                    .any(|(pending, _)| pending == submission)
                {
                    self.runtime.acknowledge_question_receipt(submission);
                }
                return Err(error);
            }
        };
        let lease = match prepared {
            QuestionDeliveryReservation::Claimed(lease) => lease,
            QuestionDeliveryReservation::Existing(receipt) => {
                self.runtime.acknowledge_question_receipt(submission);
                self.runtime
                    .hold_prior_question_delivery(submission, &receipt.delivery_id)?;
                return Ok(());
            }
        };
        // Every durable claim needs reconciliation, even if the original question ended
        // while reserve was running or the final authorization check prevents sending.
        if !self.brain.questions.receipts.iter().any(|(prior, _)| {
            prior.target() == submission.target() && prior.delivery_id() == submission.delivery_id()
        }) {
            self.brain
                .questions
                .receipts
                .push((submission.clone(), lease.clone()));
        }
        let sent = (|| {
            if Instant::now() >= permit.deadline
                || permit
                    .cancellation
                    .as_ref()
                    .is_some_and(|token| token.is_cancelled())
                || self.question_context(submission.target())?.scope() != &permit.scope
            {
                return Err("question_authorization_expired".into());
            }
            service
                .execute_with_state(
                    ContextOperation::AnswerQuestion,
                    Some(&submission.target().node_id.to_string()),
                    |state| {
                        if submission.target().provider == norma::protocol::AgentProfile::Cursor {
                            agent::context::check_context_operation(
                                state,
                                ContextOperation::SendTerminalInput,
                                Some(&submission.target().node_id.to_string()),
                            )
                            .map_err(|_| {
                                agent::AdapterError::Rejected("context_operation_restricted".into())
                            })?;
                            self.runtime
                                .set_question_terminal_guard(
                                    submission,
                                    terminal_answer_guard(
                                        service.clone(),
                                        submission.target().node_id,
                                        permit.deadline,
                                        permit.cancellation.clone(),
                                    ),
                                )
                                .map_err(|reason| agent::AdapterError::Rejected(reason.into()))?;
                        } else {
                            self.runtime
                                .set_question_send_guard(
                                    submission,
                                    send_guard(
                                        service.clone(),
                                        submission.target().node_id,
                                        permit.deadline,
                                        permit.cancellation.clone(),
                                    ),
                                )
                                .map_err(|reason| agent::AdapterError::Rejected(reason.into()))?;
                        }
                        self.runtime
                            .deliver_native_question(submission)
                            .map_err(|error| agent::AdapterError::Rejected(error.into()))
                    },
                )
                .map_err(|error| error.to_string())?;
            Ok::<(), String>(())
        })();
        if let Err(error) = sent {
            if self
                .runtime
                .fail_question_preparation(submission, &error)
                .is_ok()
            {
                self.brain
                    .questions
                    .prepared
                    .retain(|(prior, _)| prior.target() != submission.target());
                self.brain
                    .questions
                    .prepared
                    .push((submission.clone(), lease));
            }
            return Err(error);
        }
        self.brain
            .questions
            .prepared
            .retain(|(prior, _)| prior.target() != submission.target());
        Ok(())
    }
}

fn terminal_answer_guard(
    service: SharedContext,
    node: u64,
    deadline: Instant,
    cancellation: Option<tokio_util::sync::CancellationToken>,
) -> runtime::runtime::QuestionTerminalGuard {
    std::sync::Arc::new(move |write| {
        if Instant::now() >= deadline
            || cancellation
                .as_ref()
                .is_some_and(|token| token.is_cancelled())
        {
            return Err("question_authorization_expired");
        }
        service
            .execute_with_state(
                ContextOperation::AnswerQuestion,
                Some(&node.to_string()),
                |state| {
                    agent::context::check_context_operation(
                        state,
                        ContextOperation::SendTerminalInput,
                        Some(&node.to_string()),
                    )
                    .map_err(|_| {
                        agent::AdapterError::Rejected("context_operation_restricted".into())
                    })?;
                    Ok(write())
                },
            )
            .map_err(|_| "question_context_restricted")?
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_question_actions_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_question_overview_tests.rs"]
mod overview_tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_question_delegation_tests.rs"]
mod delegation_tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_question_receipt_tests.rs"]
mod receipt_tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_question_retry_tests.rs"]
mod retry_tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_question_privacy_tests.rs"]
mod privacy_tests;

#[cfg(test)]
#[path = "../../../tests/unit/brain_question_claude_real_tests.rs"]
mod claude_real_tests;

fn send_guard(
    service: SharedContext,
    node: u64,
    deadline: Instant,
    cancellation: Option<tokio_util::sync::CancellationToken>,
) -> norma::agent_hook::QuestionSendGuard {
    std::sync::Arc::new(move |send| {
        service
            .execute_with_state(
                ContextOperation::AnswerQuestion,
                Some(&node.to_string()),
                |_| {
                    if Instant::now() >= deadline
                        || cancellation
                            .as_ref()
                            .is_some_and(|token| token.is_cancelled())
                    {
                        return Err(agent::AdapterError::Rejected(
                            "question_authorization_expired".into(),
                        ));
                    }
                    send();
                    Ok(())
                },
            )
            .map_err(|_| {
                if Instant::now() >= deadline
                    || cancellation
                        .as_ref()
                        .is_some_and(|token| token.is_cancelled())
                {
                    "question_authorization_expired"
                } else {
                    "question_context_restricted"
                }
            })
    })
}
