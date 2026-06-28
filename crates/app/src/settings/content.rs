use super::{with_alpha, SettingsPanelState, SettingsSection};
use crate::app::MultisoulApp;
use crate::settings::voice::{voice_settings, VoiceSettingsState};
use crate::shortcuts::ShortcutSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight};
use ui::Theme;

mod appearance;
mod hooks;
mod keyboard;

use self::appearance::general_appearance;
use self::hooks::hooks_settings;
use self::keyboard::keyboard_settings;

#[derive(Clone, Copy)]
struct MockSetting {
    title: &'static str,
    hint: &'static str,
    value: &'static str,
}

pub(super) fn settings_content(
    theme: Theme,
    panel: &SettingsPanelState,
    voice_state: &VoiceSettingsState,
    shortcuts: &ShortcutSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .bg(rgba(with_alpha(theme.term_bg, 0x72)))
        .child(content_header(theme, panel.active_section, cx))
        .child(
            div()
                .id("settings-content-scroll")
                .flex_1()
                .overflow_scroll()
                .px(px(26.0))
                .pb(px(28.0))
                .child(match panel.active_section {
                    SettingsSection::Keyboard => keyboard_settings(theme, shortcuts, cx),
                    SettingsSection::Voice => voice_settings(theme, voice_state, cx),
                    SettingsSection::Hooks => hooks_settings(theme, panel, cx),
                    // [MOCK] 以下分区为占位实现，侧边栏入口已屏蔽；保留分支以防路由进入。
                    // SettingsSection::General => general_appearance(theme),
                    // SettingsSection::Runtime => mock_section(theme, runtime_rows()),
                    // SettingsSection::Agents => mock_section(theme, agent_rows()),
                    // SettingsSection::Skills => mock_section(theme, skill_rows()),
                    _ => div(), // 不可达：mock 分区已无入口
                }),
        )
}

fn content_header(theme: Theme, section: SettingsSection, cx: &mut Context<MultisoulApp>) -> Div {
    let subtitle = content_subtitle(section);
    let mut title_column = div().flex().flex_col().gap(px(3.0)).child(
        div()
            .text_size(px(21.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgba(theme.text_primary))
            .child(content_title(section)),
    );
    if !subtitle.is_empty() {
        title_column = title_column.child(
            div()
                .text_size(px(16.0))
                .text_color(rgba(theme.text_tertiary))
                .child(subtitle),
        );
    }

    div()
        .h(px(100.0))
        .flex()
        .flex_row()
        .items_start()
        .justify_between()
        .pt(px(34.0))
        .px(px(26.0))
        .child(title_column)
        .child(
            div()
                .id("settings-close")
                .w(px(34.0))
                .h(px(34.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.0))
                .text_size(px(25.0))
                .line_height(px(26.0))
                .text_color(rgba(theme.text_secondary))
                .cursor_pointer()
                .hover(move |style| style.bg(rgba(with_alpha(theme.destructive, 0x10))))
                .on_click(cx.listener(|app, _, _window, cx| {
                    app.close_settings_panel(cx);
                }))
                .child("x"),
        )
}

fn content_title(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::General => "Themes",
        SettingsSection::Keyboard => "Keyboard Shortcuts",
        SettingsSection::Runtime => "Runtime",
        SettingsSection::Voice => "Voice",
        SettingsSection::Agents => "Agents",
        SettingsSection::Skills => "Skills",
        SettingsSection::Hooks => "Agent Hooks",
    }
}

fn content_subtitle(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::General => "Choose your style or customize your theme",
        SettingsSection::Keyboard => "Customize app commands and review system input behavior",
        SettingsSection::Runtime => "Configure shell behavior and live sessions",
        SettingsSection::Voice => "Configure local speech recognition and command planning",
        SettingsSection::Agents => "Choose default agents, permissions, and profiles",
        SettingsSection::Skills => "Manage installed skills and project skill sources",
        SettingsSection::Hooks => "",
    }
}

fn mock_section(theme: Theme, rows: &'static [MockSetting]) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(mock_summary(theme, rows))
        .children(rows.iter().copied().map(|row| mock_row(theme, row)))
}

fn mock_summary(theme: Theme, rows: &'static [MockSetting]) -> Div {
    div()
        .flex()
        .flex_row()
        .gap(px(14.0))
        .child(summary_card(theme, "Primary", rows[0].value))
        .child(summary_card(theme, "Policy", rows[1].value))
        .child(summary_card(theme, "Status", rows[2].value))
}

fn summary_card(theme: Theme, label: &'static str, value: &'static str) -> Div {
    div()
        .flex_1()
        .h(px(104.0))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(8.0))
        .p(px(18.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba(card_border(theme)))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_tertiary))
                .child(label),
        )
        .child(
            div()
                .text_size(px(19.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(if value == "Ready" {
                    theme.accent_cyan
                } else {
                    theme.text_primary
                }))
                .child(value),
        )
}

fn mock_row(theme: Theme, row: MockSetting) -> Div {
    section_divider(theme).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(24.0))
            .child(section_label(theme, row.title, row.hint))
            .child(
                div()
                    .px(px(13.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.accent_cyan, 0x50)))
                    .bg(rgba(with_alpha(theme.accent_cyan, 0x12)))
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgba(theme.text_secondary))
                    .child(row.value),
            ),
    )
}

fn section_divider(theme: Theme) -> Div {
    div()
        .w_full()
        .pt(px(24.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
}

fn card_border(theme: Theme) -> u32 {
    with_alpha(theme.edge_highlight, 0x2c)
}

fn section_label(theme: Theme, title: &'static str, hint: &'static str) -> Div {
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
}

fn runtime_rows() -> &'static [MockSetting] {
    &[
        MockSetting {
            title: "Shell",
            hint: "Default shell for new terminal live nodes",
            value: "/bin/zsh",
        },
        MockSetting {
            title: "Environment",
            hint: "Pass host environment into spawned sessions",
            value: "Inherited",
        },
        MockSetting {
            title: "PTY / Session",
            hint: "Placeholder for restore and checkpoint policy",
            value: "Checkpoint",
        },
    ]
}

fn agent_rows() -> &'static [MockSetting] {
    &[
        MockSetting {
            title: "Default Agent",
            hint: "Agent used by the primary toolbar shortcut",
            value: "Codex",
        },
        MockSetting {
            title: "Permissions",
            hint: "Mock policy for side-effectful agent actions",
            value: "Ask first",
        },
        MockSetting {
            title: "Profiles",
            hint: "Available live agent card presets",
            value: "Claude, Codex",
        },
    ]
}

fn skill_rows() -> &'static [MockSetting] {
    &[
        MockSetting {
            title: "Installed Skills",
            hint: "Mock list of discovered project skills",
            value: "Local registry",
        },
        MockSetting {
            title: "Skill Sources",
            hint: "Prefer AGENTS.md and local skills before defaults",
            value: "Project first",
        },
        MockSetting {
            title: "Refresh Status",
            hint: "Placeholder for future skill sync controls",
            value: "Ready",
        },
    ]
}
