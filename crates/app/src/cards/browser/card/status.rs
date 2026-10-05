use super::*;

pub(super) fn ego_badge(phase: EgoCardPhase) -> (&'static str, CardChromeTaskTone) {
    match phase {
        EgoCardPhase::Creating => (ui::i18n::text("cards.creating"), CardChromeTaskTone::Active),
        EgoCardPhase::ReadyForUser => (
            ui::i18n::text("cards.ready_for_you"),
            CardChromeTaskTone::Muted,
        ),
        EgoCardPhase::PreparingAgent => (
            ui::i18n::text("cards.agent_preparing"),
            CardChromeTaskTone::Active,
        ),
        EgoCardPhase::AgentWorking => (
            ui::i18n::text("cards.agent_browsing"),
            CardChromeTaskTone::Active,
        ),
        EgoCardPhase::AwaitingUser => (
            ui::i18n::text("cards.control_requested"),
            CardChromeTaskTone::Warning,
        ),
        EgoCardPhase::UserControlling => (
            ui::i18n::text("cards.you_are_in_control"),
            CardChromeTaskTone::Warning,
        ),
        EgoCardPhase::ResumeRequested => (
            ui::i18n::text("cards.waiting_for_agent"),
            CardChromeTaskTone::Warning,
        ),
        EgoCardPhase::Completed => (
            ui::i18n::text("cards.task_complete"),
            CardChromeTaskTone::Muted,
        ),
        EgoCardPhase::Failed => (
            ui::i18n::text("cards.task_failed"),
            CardChromeTaskTone::Error,
        ),
        EgoCardPhase::Disconnected => (
            ui::i18n::text("cards.ego_disconnected"),
            CardChromeTaskTone::Error,
        ),
    }
}

pub(super) fn ego_status_text(phase: EgoCardPhase) -> &'static str {
    match phase {
        EgoCardPhase::Creating => ui::i18n::text("cards.creating_ego_space"),
        EgoCardPhase::ReadyForUser => ui::i18n::text("cards.ready_for_you"),
        EgoCardPhase::PreparingAgent => ui::i18n::text("cards.agent_preparing"),
        EgoCardPhase::AgentWorking => ui::i18n::text("cards.agent_browsing"),
        EgoCardPhase::AwaitingUser => ui::i18n::text("cards.agent_requests_control"),
        EgoCardPhase::UserControlling => ui::i18n::text("cards.you_control_ego"),
        EgoCardPhase::ResumeRequested => ui::i18n::text("cards.returned_waiting_for_agent"),
        EgoCardPhase::Completed => ui::i18n::text("cards.task_complete"),
        EgoCardPhase::Failed => ui::i18n::text("cards.task_failed"),
        EgoCardPhase::Disconnected => ui::i18n::text("cards.ego_disconnected"),
    }
}
