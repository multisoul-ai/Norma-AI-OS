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
        _ => ui::i18n::text("settings.delegate_tasks_through_norma_os"),
    }
}

/// 委派链路未验证的 provider 在设置行上显示的实验性标注；已验证的 provider 返回 None。
pub(super) fn delegation_notice(provider: OrchestrationProvider) -> Option<&'static str> {
    provider
        .delegation_unverified()
        .then(|| ui::i18n::text("settings.delegation_unverified_experimental"))
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
    component_error_detail_for(integration, ui::i18n::Locale::current())
}

fn component_error_detail_for(
    integration: &ProviderIntegration,
    locale: ui::i18n::Locale,
) -> Option<String> {
    [
        ("Skill", &integration.skill_component),
        ("Norma CLI", &integration.cli_component),
        ("Agent Hook", &integration.hook_component),
        (
            ui::i18n::text_for(locale, "settings.authorization"),
            &integration.authorization_component,
        ),
    ]
    .into_iter()
    .find_map(|(label, component)| match component {
        ComponentState::Conflict { path, reason } => Some(
            ui::i18n::message(
                "settings.label_reason_resolve_the_conflict_at_and_choose_repair",
                &[
                    ("label", (label).to_string()),
                    ("reason", (reason).to_string()),
                    ("arg0", (path.display()).to_string()),
                ],
            )
            .resolve_for(locale),
        ),
        ComponentState::Failed {
            message,
            error_code,
        } => {
            let message = if error_code == "credential_permissions_insecure" {
                ui::i18n::text_for(locale, "settings.credential_permissions_insecure")
            } else {
                message
            };
            Some(
                ui::i18n::message(
                    "settings.label_message_choose_repair_to_try_again",
                    &[
                        ("label", (label).to_string()),
                        ("message", (message).to_string()),
                    ],
                )
                .resolve_for(locale),
            )
        }
        _ => None,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/orchestration_status_tests.rs"]
mod tests;
