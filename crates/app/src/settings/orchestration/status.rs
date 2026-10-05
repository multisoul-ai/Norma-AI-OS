use super::*;

pub(super) fn provider_description(
    provider: OrchestrationProvider,
    status: AggregateStatus,
) -> &'static str {
    match (provider, status) {
        (OrchestrationProvider::Claude, AggregateStatus::NotInstalled) => {
            ui::i18n::text("settings.install_skill_cli_hooks_and_authorization")
        }
        (OrchestrationProvider::Cursor, AggregateStatus::NeedsRepair)
        | (_, AggregateStatus::InstallFailed) => {
            ui::i18n::text("settings.one_component_needs_attention")
        }
        (OrchestrationProvider::Grok, _) => {
            ui::i18n::text("settings.orchestration_is_not_supported_in_the_first_release")
        }
        _ => ui::i18n::text("settings.delegate_tasks_through_norma_os"),
    }
}

pub(super) fn component_status(theme: Theme, component: &ComponentState) -> (&'static str, u32) {
    match component {
        ComponentState::Ready { .. } => (ui::i18n::text("common.ready"), theme.accent_green),
        ComponentState::Missing => (
            ui::i18n::text("settings.needs_repair"),
            warning_status_color(),
        ),
        ComponentState::Outdated { .. } => {
            (ui::i18n::text("settings.outdated"), warning_status_color())
        }
        ComponentState::Conflict { .. } => (ui::i18n::text("settings.conflict"), theme.destructive),
        ComponentState::Failed { .. } => (ui::i18n::text("common.failed"), theme.destructive),
        ComponentState::Installing => (ui::i18n::text("settings.installing"), theme.text_tertiary),
        ComponentState::Unsupported { .. } => {
            (ui::i18n::text("settings.unavailable"), theme.text_tertiary)
        }
    }
}

pub(super) fn component_error_detail(integration: &ProviderIntegration) -> Option<String> {
    [
        ("Skill", &integration.skill_component),
        ("Norma CLI", &integration.cli_component),
        ("Agent Hook", &integration.hook_component),
        (
            ui::i18n::text("settings.authorization"),
            &integration.authorization_component,
        ),
    ]
    .into_iter()
    .find_map(|(label, component)| match component {
        ComponentState::Conflict { path, reason } => Some(ui::i18n::format(
            "settings.label_reason_resolve_the_conflict_at_and_choose_repair",
            &[
                ("label", (label).to_string()),
                ("reason", (reason).to_string()),
                ("arg0", (path.display()).to_string()),
            ],
        )),
        ComponentState::Failed { message, .. } => Some(ui::i18n::format(
            "settings.label_message_choose_repair_to_try_again",
            &[
                ("label", (label).to_string()),
                ("message", (message).to_string()),
            ],
        )),
        _ => None,
    })
}
