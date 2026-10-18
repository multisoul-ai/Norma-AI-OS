use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, Window};
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
    let cursor_installed = agent_hooks_installed(HookTarget::Cursor, &paths);
    let grok_installed = agent_hooks_installed(HookTarget::Grok, &paths);
    let opencode_installed = agent_hooks_installed(HookTarget::OpenCode, &paths);
    let pi_installed = agent_hooks_installed(HookTarget::Pi, &paths);

    div()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(hook_provider_row(
            theme,
            &HookProviderRowSpec {
                title: "Codex",
                hint: ui::i18n::text("settings.manage_lifecycle_hooks_in_codex_hooks_json"),
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
                hint: ui::i18n::text("settings.manage_lifecycle_hooks_in_claude_settings_json"),
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
                title: "Cursor Agent CLI",
                hint: ui::i18n::text("settings.manage_lifecycle_hooks_in_cursor_hooks_json"),
                config_path: paths.cursor_hooks.display().to_string(),
                installed: cursor_installed,
                status: &panel.cursor_hook_status,
                button_id: "settings-install-cursor-hooks",
            },
            cx,
            MultisoulApp::on_toggle_cursor_hooks,
        ))
        .child(hook_provider_row(
            theme,
            &HookProviderRowSpec {
                title: "Grok Build",
                hint: ui::i18n::text("settings.manage_lifecycle_hooks_in_grok_hooks"),
                config_path: paths.grok_hooks.display().to_string(),
                installed: grok_installed,
                status: &panel.grok_hook_status,
                button_id: "settings-install-grok-hooks",
            },
            cx,
            MultisoulApp::on_toggle_grok_hooks,
        ))
        .child(hook_provider_row(
            theme,
            &HookProviderRowSpec {
                title: "OpenCode",
                hint: ui::i18n::text("settings.manage_lifecycle_hooks_in_config_opencode_plugins"),
                config_path: paths.opencode_plugin.display().to_string(),
                installed: opencode_installed,
                status: &panel.opencode_hook_status,
                button_id: "settings-install-opencode-hooks",
            },
            cx,
            MultisoulApp::on_toggle_opencode_hooks,
        ))
        .child(hook_provider_row(
            theme,
            &HookProviderRowSpec {
                title: "Pi",
                hint: ui::i18n::text("settings.manage_lifecycle_hooks_in_pi_extensions"),
                config_path: paths.pi_extension.display().to_string(),
                installed: pi_installed,
                status: &panel.pi_hook_status,
                button_id: "settings-install-pi-hooks",
            },
            cx,
            MultisoulApp::on_toggle_pi_hooks,
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

    pub(super) fn on_toggle_pi_hooks(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_panel.pi_hook_status = toggle_status_for(HookTarget::Pi);
        cx.notify();
    }

    pub(super) fn on_toggle_cursor_hooks(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_panel.cursor_hook_status = toggle_status_for(HookTarget::Cursor);
        cx.notify();
    }

    pub(super) fn on_toggle_grok_hooks(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_panel.grok_hook_status = toggle_status_for(HookTarget::Grok);
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
            message: Some(hook_install_success_message(target, command_path.as_path())),
            is_error: false,
        },
        Err(error) => HookInstallStatus {
            message: Some(error.to_string().into()),
            is_error: true,
        },
    }
}

fn hook_install_success_message(
    target: HookTarget,
    command_path: &std::path::Path,
) -> ui::i18n::LocalizedText {
    let provider = match target {
        HookTarget::Claude => "Claude Code",
        HookTarget::Codex => "Codex",
        HookTarget::Cursor => "Cursor Agent CLI",
        HookTarget::Grok => "Grok Build",
        HookTarget::OpenCode => "OpenCode",
        HookTarget::Pi => "Pi",
        HookTarget::All => ui::i18n::text("settings.agent_providers"),
    };
    ui::i18n::message(
        "settings.installed_using_provider_reads_hooks_when_a_session_starts_restart_existing",
        &[
            ("arg0", (command_path.display()).to_string()),
            ("provider", (provider).to_string()),
        ],
    )
}

fn uninstall_status_for(target: HookTarget) -> HookInstallStatus {
    match uninstall_agent_hooks(target) {
        Ok(()) => HookInstallStatus {
            message: Some(ui::i18n::LocalizedText::new(
                "settings.norma_os_hooks_removed",
            )),
            is_error: false,
        },
        Err(error) => HookInstallStatus {
            message: Some(error.to_string().into()),
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
        (
            ui::i18n::text("settings.uninstall_hooks"),
            theme.destructive,
        )
    } else {
        (
            ui::i18n::text("settings.install_hooks"),
            theme.accent_orange,
        )
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
                                .settings_font(TextRole::Headline)
                                .text_color(rgba(theme.text_primary))
                                .child(title),
                        )
                        .child(div().settings_text(TextRole::Helper, theme).child(hint))
                        .child(
                            div()
                                .mt(px(6.0))
                                .settings_font(TextRole::Helper)
                                .text_color(rgba(if installed {
                                    theme.accent_cyan
                                } else {
                                    theme.text_secondary
                                }))
                                .child(if installed {
                                    ui::i18n::format(
                                        "settings.installed_config_path",
                                        &[("config_path", (config_path).to_string())],
                                    )
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
                        .settings_font(TextRole::Control)
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
        .settings_font(TextRole::Helper)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 安装 hook 后，Settings 必须提示用户重启既有 Agent，避免旧 Codex 进程遗漏运行态事件。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   command_path = "/tmp/Norma OS Dev.app/Contents/MacOS/multisoul-agent-hook"
    ///   该路径模拟 cargo run 开发 bundle 中刚安装的 bridge 二进制
    ///   已运行 Agent 在安装前启动，Codex 只在会话启动时读取 hooks.json，不能热加载
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 将开发 bundle 的 hook command_path 传给安装成功消息生成函数
    ///   2. 读取返回的 Settings 状态文本
    ///   3. 检查文本同时说明安装位置与既有 Agent 的重启动作
    ///
    /// 预期结果：
    ///   - 正断言：文本包含 command_path，用户可定位实际 bridge
    ///   - 正断言：文本包含 restart，用户知道重新创建 Agent 后才会加载 hook
    ///   - 负断言：文本不能只报告 Installed，否则用户会误以为旧进程已自动生效
    #[test]
    fn test_hook_install_success_message_requires_restarting_existing_agents() {
        let command_path = Path::new("/tmp/Norma OS Dev.app/Contents/MacOS/multisoul-agent-hook");

        let message = hook_install_success_message(HookTarget::Codex, command_path);

        assert!(
            message.contains(&command_path.display().to_string()),
            "安装成功消息应显示实际 bridge 路径，便于定位 cargo run 的开发 bundle"
        );
        assert!(
            message.contains("restart existing Agent cards"),
            "安装成功消息应要求重启既有 Agent，确保 Codex 在会话启动时读取 hooks"
        );
        assert_ne!(
            message,
            format!("Installed using {}", command_path.display()),
            "安装成功消息不能只报告 Installed，否则会遗漏旧 Agent 不会热加载 hooks 的关键事实"
        );
    }
}
