//! Agent 库：搜索与筛选工具栏 + Agent 卡片网格。

use super::super::components::{avatar, empty_list_message, primary_button, tag, with_alpha};
use super::super::grouping::{group_definitions, roster_summary};
use super::super::AgentDefinitionsState;
use super::toolbar::{agent_search_input, agent_status_filter};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use canvas::AgentProfile;
use gpui::{
    div, prelude::*, px, rgba, AnyElement, Context, Div, KeyDownEvent, MouseButton, MouseDownEvent,
    ScrollHandle, Window,
};
use std::collections::HashSet;
use storage::AgentDefinition;
use ui::component::switch::Switch;
use ui::component::{Sizable, Size};
use ui::Theme;

/// 网格列数随窗口宽度变化；抽屉是覆盖其上的模态侧边栏，不挤压网格。
pub(super) fn agent_library(
    theme: Theme,
    state: &AgentDefinitionsState,
    on_canvas_ids: &HashSet<i64>,
    list_scroll: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let drawer_open = state.is_drawer_open();
    // 设置面板内容区约 620px：默认 2 列；窗口足够宽（面板 ≥ 1000px）时 3 列。
    let columns = if window.viewport_size().width >= px(1500.0) {
        3
    } else {
        2
    };
    let on_canvas_total = state
        .definitions()
        .iter()
        .filter(|definition| on_canvas_ids.contains(&definition.id))
        .count();
    let summary = roster_summary(state.definitions().len(), on_canvas_total);
    let groups = group_definitions(state.visible_definitions(), on_canvas_ids);
    let no_rows = groups.on_canvas.is_empty() && groups.library.is_empty();
    let selected_id = if drawer_open {
        state.selected_definition_id()
    } else {
        None
    };

    div()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .p(px(16.0))
        .gap(px(12.0))
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(10.0))
                        .child(agent_search_input(theme, state, window, cx))
                        .child(primary_button(
                            theme,
                            "settings-agent-new",
                            ui::i18n::text("settings.new_agent_2"),
                            false,
                            cx.listener(MultisoulApp::on_agent_settings_new_definition),
                        )),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap(px(10.0))
                        .child(agent_status_filter(theme, state.status_filter(), cx))
                        .child(div().settings_text(TextRole::Helper, theme).child(summary)),
                ),
        )
        .child(
            div()
                .id("settings-agents-list")
                .debug_selector(|| "settings-agents-list".to_string())
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .track_scroll(list_scroll)
                .flex()
                .flex_col()
                .gap(px(14.0))
                .when(no_rows && !state.is_creating_new(), |this| {
                    this.child(empty_list_message(
                        theme,
                        if state.definitions().is_empty() {
                            ui::i18n::text("settings.no_project_agents_saved_yet")
                        } else {
                            ui::i18n::text("settings.no_agents_match_your_search")
                        },
                    ))
                })
                .child(
                    div()
                        .w_full()
                        .grid()
                        .grid_cols(columns)
                        .gap(px(14.0))
                        .when(state.is_creating_new(), |this| {
                            this.child(unsaved_agent_card(theme, &state.draft().name))
                        })
                        .children(
                            groups
                                .on_canvas
                                .into_iter()
                                .map(|definition| (definition, true))
                                .chain(
                                    groups
                                        .library
                                        .into_iter()
                                        .map(|definition| (definition, false)),
                                )
                                .map(|(definition, on_canvas)| {
                                    agent_card(
                                        theme,
                                        definition,
                                        on_canvas,
                                        selected_id == Some(definition.id),
                                        cx,
                                    )
                                }),
                        )
                        .child(new_agent_tile(theme, cx)),
                ),
        )
}

fn card_surface(theme: Theme, selected: bool) -> Div {
    div()
        .min_h(px(132.0))
        .min_w(px(0.0))
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .rounded(px(16.0))
        .border_2()
        .border_color(rgba(if selected {
            theme.accent_orange
        } else {
            with_alpha(theme.accent_orange, 0x00)
        }))
        .bg(rgba(if selected {
            with_alpha(theme.accent_orange, 0x1f)
        } else {
            with_alpha(theme.text_primary, 0x0a)
        }))
}

fn unsaved_agent_card(theme: Theme, name: &str) -> AnyElement {
    card_surface(theme, true)
        .id("settings-agent-list-row-unsaved")
        .debug_selector(|| "settings-agent-list-row-unsaved".to_string())
        .child(avatar(name, AgentProfile::Codex, 44.0))
        .child(
            div()
                .mt(px(8.0))
                .truncate()
                .settings_text(TextRole::Headline, theme)
                .child(name.to_string()),
        )
        .child(
            div()
                .settings_font(TextRole::Helper)
                .text_color(rgba(theme.accent_orange))
                .child(ui::i18n::text("settings.unsaved_codex")),
        )
        .into_any_element()
}

fn new_agent_tile(theme: Theme, cx: &mut Context<MultisoulApp>) -> AnyElement {
    card_surface(theme, false)
        .id("settings-agent-new-tile")
        .debug_selector(|| "settings-agent-new-tile".to_string())
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .border_dashed()
        .border_color(rgba(with_alpha(theme.text_secondary, 0x80)))
        .bg(rgba(with_alpha(theme.text_primary, 0x00)))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.text_primary, 0x0d))))
        .active(move |style| style.bg(rgba(with_alpha(theme.accent_orange, 0x33))))
        .tab_index(0)
        .focus(move |style| style.border_color(rgba(theme.accent_orange)))
        .on_click(cx.listener(MultisoulApp::on_agent_settings_new_definition))
        .on_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                app.on_agent_settings_new_definition(&Default::default(), window, cx);
                cx.stop_propagation();
            }
        }))
        .child(
            div()
                .settings_text(TextRole::Control, theme)
                .child(ui::i18n::text("settings.new_agent_2")),
        )
        .into_any_element()
}

fn agent_card(
    theme: Theme,
    definition: &AgentDefinition,
    on_canvas: bool,
    selected: bool,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let id = definition.id;
    let selector = format!("settings-agent-list-row-{id}");
    let enabled = definition.enabled;
    let dim = if enabled { 1.0 } else { 0.5 };

    card_surface(theme, selected)
        .id(("settings-agent-list-row", id as u64))
        .debug_selector(move || selector.clone())
        .cursor_pointer()
        .hover(move |style| {
            if selected {
                style
            } else {
                style.bg(rgba(with_alpha(theme.text_primary, 0x14)))
            }
        })
        .active(move |style| style.bg(rgba(with_alpha(theme.accent_orange, 0x33))))
        .tab_index(0)
        .focus(move |style| style.border_color(rgba(with_alpha(theme.text_primary, 0xe0))))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                app.on_agent_settings_select_definition(id, event, window, cx);
            }),
        )
        .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                app.agent_definitions.select_definition(id);
                cx.notify();
                cx.stop_propagation();
            }
        }))
        .child(
            div()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .child(avatar(&definition.name, definition.runtime_profile, 44.0).opacity(dim))
                .child(
                    div()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            Switch::new(("settings-agent-card-switch", id as u64))
                                .checked(enabled)
                                .with_size(Size::Small)
                                .on_click(cx.listener(move |app, _checked, _window, cx| {
                                    app.on_agent_settings_toggle_enabled_from_card(id, cx);
                                })),
                        ),
                ),
        )
        .child(
            div()
                .mt(px(8.0))
                .truncate()
                .opacity(dim)
                .settings_text(TextRole::Headline, theme)
                .child(definition.name.clone()),
        )
        .child(
            div()
                .id(("settings-agent-list-runtime-profile", id as u64))
                .debug_selector(|| "settings-agent-list-runtime-profile".to_string())
                .line_clamp(2)
                .opacity(dim)
                .settings_font(TextRole::Helper)
                // Keeps the pre-unification tone: text_secondary fails 4.5:1 on light themes.
                .text_color(rgba(with_alpha(theme.text_primary, 0xc0)))
                .child(workflow_summary(&definition.workflow_description)),
        )
        .child(
            div()
                .mt_auto()
                .pt(px(8.0))
                .flex()
                .flex_row()
                .gap(px(6.0))
                .child(tag(theme, definition.runtime_profile.display_title(), None))
                .when(!enabled, |this| {
                    this.child(tag(theme, ui::i18n::text("settings.off"), None))
                })
                .when(on_canvas, |this| {
                    this.child(
                        tag(
                            theme,
                            ui::i18n::text("settings.on_canvas_2"),
                            Some(theme.accent_cyan),
                        )
                        .id(("settings-agent-list-on-canvas-dot", id as u64)),
                    )
                }),
        )
        .into_any_element()
}

fn workflow_summary(workflow: &str) -> String {
    let trimmed = workflow.trim();
    if trimmed.is_empty() {
        ui::i18n::text("settings.no_workflow_instructions").to_string()
    } else if trimmed.chars().count() > 96 {
        format!("{}…", trimmed.chars().take(95).collect::<String>())
    } else {
        trimmed.to_string()
    }
}
