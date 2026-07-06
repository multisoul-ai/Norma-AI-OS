use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, FontWeight, Window};
use norma::agent_hook::{
    agent_hooks_installed, default_install_paths, install_agent_hooks, uninstall_agent_hooks,
    HookTarget,
};
use ui::Theme;

use super::super::{HookInstallStatus, SettingsPanelState};

pub(super) fn hooks_settings(
    theme: Theme,
    panel: &SettingsPanelState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let paths = default_install_paths();
    let codex_installed = agent_hooks_installed(HookTarget::Codex, &paths);
    let claude_installed = agent_hooks_installed(HookTarget::Claude, &paths);
    let opencode_installed = agent_hooks_installed(HookTarget::OpenCode, &paths);

    div()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(hook_provider_row(
            theme,
            &HookProviderRowSpec {
                title: "Codex",
                hint: "Manage lifecycle hooks in ~/.codex/hooks.json",
                config_path: paths.codex_hooks.display().to_string(),
                installed: codex_installed,
                status: &panel.codex_hook_status,
                button_id: "settings-install-codex-hooks",
            },
            cx,
            MultisoulApp::on_toggle_codex_hooks,
        ))
        .child(hook_provider_row(
            theme,
            &HookProviderRowSpec {
                title: "Claude Code",
                hint: "Manage lifecycle hooks in ~/.claude/settings.json",
                config_path: paths.claude_settings.display().to_string(),
                installed: claude_installed,
                status: &panel.claude_hook_status,
                button_id: "settings-install-claude-hooks",
            },
            cx,
            MultisoulApp::on_toggle_claude_hooks,
        ))
        .child(hook_provider_row(
            theme,
            &HookProviderRowSpec {
                title: "OpenCode",
                hint: "Manage lifecycle hooks in ~/.config/opencode/plugins",
                config_path: paths.opencode_plugin.display().to_string(),
                installed: opencode_installed,
                status: &panel.opencode_hook_status,
                button_id: "settings-install-opencode-hooks",
            },
            cx,
            MultisoulApp::on_toggle_opencode_hooks,
        ))
}

/// Content bundle for one [`hook_provider_row`] (provider labels, paths, and
/// install state). Grouped so the row builder stays under clippy's
/// `too_many_arguments` threshold without losing call-site clarity.
struct HookProviderRowSpec<'a> {
    title: &'static str,
    hint: &'static str,
    config_path: String,
    installed: bool,
    status: &'a HookInstallStatus,
    button_id: &'static str,
}

impl MultisoulApp {
    pub(super) fn on_toggle_codex_hooks(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_panel.codex_hook_status = toggle_status_for(HookTarget::Codex);
        cx.notify();
    }

    pub(super) fn on_toggle_claude_hooks(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_panel.claude_hook_status = toggle_status_for(HookTarget::Claude);
        cx.notify();
    }

    pub(super) fn on_toggle_opencode_hooks(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_panel.opencode_hook_status = toggle_status_for(HookTarget::OpenCode);
        cx.notify();
    }
}

fn toggle_status_for(target: HookTarget) -> HookInstallStatus {
    let paths = default_install_paths();
    if agent_hooks_installed(target, &paths) {
        uninstall_status_for(target)
    } else {
        install_status_for(target)
    }
}

fn install_status_for(target: HookTarget) -> HookInstallStatus {
    match install_agent_hooks(target) {
        Ok(command_path) => HookInstallStatus {
            message: Some(format!("Installed using {}", command_path.display())),
            is_error: false,
        },
        Err(error) => HookInstallStatus {
            message: Some(error.to_string()),
            is_error: true,
        },
    }
}

fn uninstall_status_for(target: HookTarget) -> HookInstallStatus {
    match uninstall_agent_hooks(target) {
        Ok(()) => HookInstallStatus {
            message: Some("Norma OS hooks removed".to_string()),
            is_error: false,
        },
        Err(error) => HookInstallStatus {
            message: Some(error.to_string()),
            is_error: true,
        },
    }
}

fn hook_provider_row(
    theme: Theme,
    spec: &HookProviderRowSpec<'_>,
    cx: &mut Context<MultisoulApp>,
    handler: fn(&mut MultisoulApp, &ClickEvent, &mut Window, &mut Context<MultisoulApp>),
) -> Div {
    let HookProviderRowSpec {
        title,
        hint,
        ref config_path,
        installed,
        status,
        button_id,
    } = *spec;
    let (button_label, accent) = if installed {
        ("Uninstall hooks", theme.destructive)
    } else {
        ("Install hooks", theme.accent_orange)
    };

    div()
        .w_full()
        .pt(px(24.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap(px(24.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(18.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(theme.text_primary))
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(16.0))
                                .text_color(rgba(theme.text_tertiary))
                                .child(hint),
                        )
                        .child(
                            div()
                                .mt(px(6.0))
                                .text_size(px(13.0))
                                .text_color(rgba(if installed {
                                    theme.accent_cyan
                                } else {
                                    theme.text_secondary
                                }))
                                .child(if installed {
                                    format!("Installed · {config_path}")
                                } else {
                                    config_path.as_str().to_owned()
                                }),
                        )
                        .children(status_message(theme, status)),
                )
                .child(
                    div()
                        .id(button_id)
                        .px(px(16.0))
                        .py(px(10.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgba(with_alpha(accent, 0x70)))
                        .bg(rgba(with_alpha(accent, 0x18)))
                        .text_size(px(14.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(accent))
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgba(with_alpha(accent, 0x28))))
                        .on_click(cx.listener(handler))
                        .child(button_label),
                ),
        )
}

fn status_message(theme: Theme, status: &HookInstallStatus) -> Vec<Div> {
    let Some(message) = status.message.as_deref() else {
        return Vec::new();
    };
    vec![div()
        .mt(px(8.0))
        .text_size(px(13.0))
        .text_color(rgba(if status.is_error {
            theme.destructive
        } else {
            theme.accent_cyan
        }))
        .child(message.to_string())]
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
