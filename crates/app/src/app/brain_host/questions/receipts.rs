//! Provider consumption is already correlated by Runtime. Persist it without repeating IO.
use super::*;

impl MultisoulApp {
    pub(crate) fn flush_question_receipts(&mut self, cx: &mut Context<Self>) {
        let mut waiting = Vec::new();
        for (submission, lease) in self.brain.questions.receipts.drain(..) {
            let Some(snapshot) = self.runtime.question(submission.target()) else {
                // Closed project/node: the committed intent remains Unknown, never reusable.
                continue;
            };
            if snapshot
                .delivery_id
                .as_deref()
                .is_some_and(|id| id != submission.delivery_id())
            {
                continue;
            }
            let outcome = match snapshot.state {
                QuestionState::Answered
                    if snapshot.delivery_id.as_deref() == Some(submission.delivery_id()) =>
                {
                    Some(QuestionDeliveryState::Answered)
                }
                QuestionState::AnsweredElsewhere => Some(QuestionDeliveryState::AnsweredElsewhere),
                QuestionState::ConsumedUnattributed => {
                    Some(QuestionDeliveryState::ConsumedUnattributed)
                }
                QuestionState::Cancelled => Some(QuestionDeliveryState::Cancelled),
                QuestionState::Expired => Some(QuestionDeliveryState::Expired),
                _ => None,
            };
            if let Some(outcome) = outcome {
                let evidence = snapshot.evidence.clone().or_else(|| {
                    matches!(
                        snapshot.state,
                        QuestionState::Cancelled | QuestionState::Expired
                    )
                    .then(|| {
                        snapshot
                            .error
                            .clone()
                            .unwrap_or_else(|| "original_question_closed".into())
                    })
                });
                if let Some(evidence) = evidence {
                    self.brain
                        .questions
                        .ready_receipts
                        .push((lease, outcome, evidence));
                    continue;
                }
            }
            waiting.push((submission, lease));
        }
        self.brain.questions.receipts = waiting;
        if self.brain.questions.receipt_write_pending
            || self
                .brain
                .questions
                .receipt_retry_at
                .is_some_and(|at| Instant::now() < at)
        {
            return;
        }
        if self.brain.questions.ready_receipts.is_empty() {
            return;
        }
        let ready = std::mem::take(&mut self.brain.questions.ready_receipts);
        self.brain.questions.receipt_write_pending = true;
        let store = QuestionDeliveryStore::new(self.database_paths.app_db_path().to_path_buf());
        let work = cx.background_executor().spawn(async move {
            let mut failed = Vec::new();
            for (lease, outcome, evidence) in ready {
                if let Err(error) = store.settle(&lease, outcome, &evidence) {
                    tracing::warn!(%error, "question receipt persistence failed; provider is not retried");
                    failed.push((lease, outcome, evidence));
                }
            }
            failed
        });
        cx.spawn(async move |this, cx| {
            let failed = work.await;
            let _ = this.update(cx, |app, cx| {
                app.brain.questions.receipt_write_pending = false;
                app.brain.questions.receipt_retry_at =
                    (!failed.is_empty()).then(|| Instant::now() + Duration::from_secs(5));
                app.brain.questions.ready_receipts.extend(failed);
                cx.notify();
            });
        })
        .detach();
    }
}
