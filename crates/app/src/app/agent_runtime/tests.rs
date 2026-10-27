use super::*;

/// Direct executor tests bypass the async model adapter, so perform its real
/// persisted-state preparation outside the GPUI entity update before acting.
fn prepare_shared_context(app: &gpui::Entity<MultisoulApp>, cx: &mut gpui::VisualTestContext) {
    let service = app
        .update(cx, |app, _| app.shared_context_service())
        .expect("test source scope");
    shared_tokio_runtime()
        .unwrap()
        .block_on(service.snapshot())
        .expect("load persisted constraints before executing");
}

#[path = "../../../tests/unit/agent_runtime_tests.rs"]
mod agent_runtime_tests;

#[path = "../../../tests/unit/norma_card_adapter_tests.rs"]
mod norma_card_adapter_tests;

#[path = "../../../tests/unit/norma_agent_card_close_tests.rs"]
mod norma_agent_card_close_tests;

#[path = "../../../tests/unit/agent_dispatch_submit_tests.rs"]
mod agent_dispatch_submit_tests;

#[path = "../../../tests/unit/agent_dispatch_visual_effect_tests.rs"]
mod agent_dispatch_visual_effect_tests;

#[path = "../../../tests/unit/agent_to_agent_dispatch_visual_effect_tests.rs"]
mod agent_to_agent_dispatch_visual_effect_tests;

#[path = "../../../tests/unit/agent_dispatch_inactive_window_visual_effect_tests.rs"]
mod agent_dispatch_inactive_window_visual_effect_tests;

#[path = "../../../tests/unit/agent_dispatch_workspace_tests.rs"]
mod agent_dispatch_workspace_tests;

#[path = "../../../tests/unit/agent_dispatch_queue_failure_tests.rs"]
mod agent_dispatch_queue_failure_tests;

#[path = "../../../tests/unit/agent_dispatch_unsafe_failure_tests.rs"]
mod agent_dispatch_unsafe_failure_tests;

#[path = "../../../tests/unit/agent_force_untracked_replacement_tests.rs"]
mod agent_force_untracked_replacement_tests;

#[path = "../../../tests/unit/agent_session_termination_queue_tests.rs"]
mod agent_session_termination_queue_tests;

#[path = "../../../tests/unit/agent_context_command_tests.rs"]
mod agent_context_command_tests;

#[path = "../../../tests/unit/agent_self_identity_tests.rs"]
mod agent_self_identity_tests;
