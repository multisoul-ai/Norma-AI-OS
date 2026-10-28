//! Exact current-user delegation through the same scope and timeline gate as Live tools.
use super::{
    bridge::DelegatedWork,
    report::{DelegatedReport, PlannerFailure},
};
use crate::app::{agent_runtime::LiveAdapterScope, MultisoulApp};

pub(super) fn delegate(
    app: &mut MultisoulApp,
    work: &DelegatedWork,
    scope: &LiveAdapterScope,
) -> Option<DelegatedReport> {
    // A client delegation is a semantic candidate, not a final transcript fence.
    // Never discard earlier unconsumed user words using an Assistant backchannel.
    let text = &work.snapshot.user_input;
    if !MultisoulApp::is_question_delegation(text) {
        if MultisoulApp::contains_question_delegation(text) {
            let receipt = agent::AdapterResult { status: "rejected".into(), ..agent::AdapterResult::succeeded(
                "本次原话包含其他内容，尚未授权。请重新完整说出“这题你直接回答”，或在问答卡片确认。") };
            return Some(DelegatedReport {
                receipts: vec![Ok(receipt)],
                ..Default::default()
            });
        }
        return None;
    }
    let rejected = || DelegatedReport {
        planner: Err(PlannerFailure::Cancelled),
        ..Default::default()
    };
    let timeline = scope.timeline.lock().expect("Live authorization lock");
    let project = app
        .runtime
        .workspace_root()
        .map(|root| root.display().to_string());
    if work.cancellation.is_cancelled()
        || scope.project.is_empty()
        || project.as_deref() != Some(scope.project.as_str())
        || !app.gpt_live.matches_generation(scope.generation)
        || !app.gpt_live.input_complete()
        || scope.snapshot != work.snapshot
        || !timeline.is_current(&work.snapshot)
    {
        return Some(rejected());
    }
    let expected = app.gpt_live.context_scope.clone();
    if !app
        .shared_context_service()
        .is_ok_and(|service| Some(service.scope()) == expected.as_ref())
    {
        return Some(rejected());
    }
    let result = app.question_delegation_from_voice(text, Some(work.cancellation.clone()))?;
    let accepted = result.is_ok();
    let receipt = match result {
        Ok(message) => agent::AdapterResult::accepted(message),
        Err(message) => agent::AdapterResult {
            status: "rejected".into(),
            ..agent::AdapterResult::succeeded(message)
        },
    };
    Some(DelegatedReport {
        receipts: vec![Ok(receipt)],
        effects: accepted,
        ..Default::default()
    })
}
