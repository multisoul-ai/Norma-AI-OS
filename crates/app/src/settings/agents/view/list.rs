use super::super::components::{avatar, empty_list_message, primary_button, with_alpha};
use super::super::grouping::{group_definitions, roster_summary};
use super::super::{AgentDefinitionsState, AgentStatusFilter};
use crate::app::MultisoulApp;
use canvas::AgentProfile;
use gpui::{
    div, prelude::*, px, rgba, AnyElement, AppContext, Context, Div, Entity, Focusable, FontWeight,
    KeyDownEvent, MouseButton, MouseDownEvent, ScrollHandle, Subscription, Window,
};
use std::collections::HashSet;
use storage::AgentDefinition;
use ui::component::input::{Input, InputEvent, InputState};
use ui::component::{Icon, IconName};
use ui::Theme;

struct AgentSearchInputState {
    input: Entity<InputState>,
    _subscription: Subscription,
}

pub(super) fn agent_list(
    theme: Theme,
    state: &AgentDefinitionsState,
    on_canvas_ids: &HashSet<i64>,
    list_scroll: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let definitions = state.visible_definitions();
    let on_canvas_total = state
        .definitions()
        .iter()
        .filter(|definition| on_canvas_ids.contains(&definition.id))
        .count();
    let summary = roster_summary(state.definitions().len(), on_canvas_total);
    let groups = group_definitions(definitions, on_canvas_ids);
    let no_rows = groups.on_canvas.is_empty() && groups.library.is_empty();

    div()
        .w(px(264.0))
        .h_full()
        .min_h(px(0.0))
        .flex_none()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded_l(px(10.0))
        .border_r_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x28)))
        .bg(rgba(with_alpha(theme.panel_high, 0x72)))
        .p(px(16.0))
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(20.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(theme.text_primary))
                                .child("Agents"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(summary),
                        ),
                )
                .child(primary_button(
                    theme,
                    "settings-agent-new",
                    "+ New Agent",
                    true,
                    cx.listener(MultisoulApp::on_agent_settings_new_definition),
                )),
        )
        .child(agent_search_input(theme, state, window, cx))
        .child(agent_status_filter(theme, state.status_filter(), cx))
        .child(
            div()
                .id("settings-agents-list")
                .debug_selector(|| "settings-agents-list".to_string())
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .track_scroll(list_scroll)
                .gap(px(2.0))
                .when(state.is_creating_new(), |this| {
                    this.child(unsaved_agent_row(theme, &state.draft().name))
                })
                .when(no_rows && !state.is_creating_new(), |this| {
                    this.child(empty_list_message(
                        theme,
                        if state.definitions().is_empty() {
                            "No project agents saved yet."
                        } else {
                            "No agents match your search."
                        },
                    ))
                })
                .when(!groups.on_canvas.is_empty(), |this| {
                    this.child(section_label(theme, "ON CANVAS"))
                })
                .children(groups.on_canvas.into_iter().map(|definition| {
                    agent_list_row(
                        theme,
                        definition,
                        true,
                        state.selected_definition_id() == Some(definition.id),
                        cx,
                    )
                }))
                .when(!groups.library.is_empty(), |this| {
                    this.child(section_label(theme, "LIBRARY"))
                })
                .children(groups.library.into_iter().map(|definition| {
                    agent_list_row(
                        theme,
                        definition,
                        false,
                        state.selected_definition_id() == Some(definition.id),
                        cx,
                    )
                })),
        )
}

fn section_label(theme: Theme, label: &'static str) -> AnyElement {
    div()
        .pt(px(8.0))
        .pb(px(4.0))
        .px(px(8.0))
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba(theme.text_secondary))
        .child(label)
        .into_any_element()
}

fn unsaved_agent_row(theme: Theme, name: &str) -> AnyElement {
    div()
        .id("settings-agent-list-row-unsaved")
        .debug_selector(|| "settings-agent-list-row-unsaved".to_string())
        .h(px(56.0))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .px(px(8.0))
        .rounded(px(8.0))
        .bg(rgba(with_alpha(theme.accent_orange, 0x1f)))
        .child(avatar(name, AgentProfile::Codex, 32.0))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .truncate()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba(theme.text_primary))
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(theme.accent_orange))
                        .child("Unsaved · Codex"),
                ),
        )
        .into_any_element()
}

fn agent_status_filter(
    theme: Theme,
    selected: AgentStatusFilter,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    div()
        .id("settings-agent-status-filter")
        .w_full()
        .h(px(28.0))
        .flex_none()
        .flex()
        .flex_row()
        .rounded(px(8.0))
        .bg(rgba(with_alpha(theme.text_primary, 0x0d)))
        .p(px(2.0))
        .gap(px(2.0))
        .child(status_filter_button(
            theme,
            "All",
            "settings-agent-filter-all",
            AgentStatusFilter::All,
            selected,
            cx,
        ))
        .child(status_filter_button(
            theme,
            "Enabled",
            "settings-agent-filter-enabled",
            AgentStatusFilter::Enabled,
            selected,
            cx,
        ))
        .child(status_filter_button(
            theme,
            "Disabled",
            "settings-agent-filter-disabled",
            AgentStatusFilter::Disabled,
            selected,
            cx,
        ))
        .into_any_element()
}

fn status_filter_button(
    theme: Theme,
    label: &'static str,
    selector: &'static str,
    filter: AgentStatusFilter,
    selected: AgentStatusFilter,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let is_selected = selected == filter;
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x00)))
        .bg(rgba(if is_selected {
            theme.panel_high
        } else {
            with_alpha(theme.panel_high, 0x00)
        }))
        .when(is_selected, |this| this.shadow_sm())
        .text_size(px(12.0))
        .font_weight(if is_selected {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .text_color(rgba(if is_selected {
            theme.text_primary
        } else {
            theme.text_secondary
        }))
        .cursor_pointer()
        .hover(move |style| {
            if is_selected {
                style
            } else {
                style.bg(rgba(with_alpha(theme.text_primary, 0x0d)))
            }
        })
        .active(move |style| style.bg(rgba(with_alpha(theme.accent_orange, 0x33))))
        .tab_index(0)
        .focus(move |style| style.border_color(rgba(theme.accent_orange)))
        .on_click(cx.listener(move |app, event, window, cx| {
            app.on_agent_settings_set_status_filter(filter, event, window, cx);
        }))
        .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                if app.agent_definitions.set_status_filter(filter) {
                    cx.notify();
                }
                cx.stop_propagation();
            }
        }))
        .child(label)
        .into_any_element()
}

fn agent_search_input(
    theme: Theme,
    state: &AgentDefinitionsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let app = cx.weak_entity();
    let initial_value = state.search_query().to_string();
    let search_state =
        window.use_keyed_state("settings-agent-search-state", cx, move |window, cx| {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Search agents")
                    .default_value(initial_value)
            });
            let subscription = cx.subscribe(&input, move |_, input, event: &InputEvent, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let Some(app) = app.upgrade() else {
                    return;
                };
                let query = input.read(cx).value().to_string();
                app.update(cx, |app, cx| {
                    if app.agent_definitions.set_search_query(query) {
                        cx.notify();
                    }
                });
            });
            AgentSearchInputState {
                input,
                _subscription: subscription,
            }
        });
    let input = search_state.read(cx).input.clone();
    let focus = input.focus_handle(cx);

    div()
        .id("settings-agent-search-input")
        .debug_selector(|| "settings-agent-search-input".to_string())
        .w_full()
        .h(px(32.0))
        .flex_none()
        .flex()
        .items_center()
        .px(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(theme.text_secondary))
        .bg(rgba(with_alpha(theme.panel_high, 0x96)))
        .text_size(px(12.0))
        .text_color(rgba(theme.text_primary))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            focus.focus(window);
            cx.stop_propagation();
        })
        .child(
            Input::new(&input)
                .appearance(false)
                .cleanable(true)
                .prefix(Icon::new(IconName::Search).text_color(rgba(theme.text_secondary))),
        )
        .into_any_element()
}

fn agent_list_row(
    theme: Theme,
    definition: &AgentDefinition,
    on_canvas: bool,
    selected: bool,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let id = definition.id;
    let selector = format!("settings-agent-list-row-{id}");
    let bg = if selected {
        with_alpha(theme.accent_orange, 0x1f)
    } else {
        with_alpha(theme.text_primary, 0x00)
    };

    div()
        .id(("settings-agent-list-row", id as u64))
        .debug_selector(move || selector.clone())
        .h(px(56.0))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .px(px(8.0))
        .rounded(px(8.0))
        .border_2()
        .border_color(rgba(with_alpha(theme.accent_orange, 0x00)))
        .bg(rgba(bg))
        .when(!definition.enabled, |this| this.opacity(0.5))
        .cursor_pointer()
        .hover(move |style| {
            if selected {
                style
            } else {
                style.bg(rgba(with_alpha(theme.text_primary, 0x0d)))
            }
        })
        .active(move |style| style.bg(rgba(with_alpha(theme.accent_orange, 0x33))))
        .tab_index(0)
        .focus(move |style| style.border_color(rgba(theme.accent_orange)))
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
        .child(avatar(&definition.name, definition.runtime_profile, 32.0))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(2.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(14.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgba(theme.text_primary))
                                .child(definition.name.clone()),
                        )
                        .when(!definition.enabled, |this| {
                            this.child(
                                div()
                                    .flex_none()
                                    .px(px(6.0))
                                    .rounded(px(8.0))
                                    .bg(rgba(with_alpha(theme.text_primary, 0x14)))
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgba(theme.text_secondary))
                                    .child("Off"),
                            )
                        }),
                )
                .child(
                    div()
                        .id(("settings-agent-list-runtime-profile", id as u64))
                        .debug_selector(|| "settings-agent-list-runtime-profile".to_string())
                        .truncate()
                        .text_size(px(12.0))
                        .text_color(rgba(with_alpha(theme.text_primary, 0xc0)))
                        .child(format!(
                            "{} · {}",
                            definition.runtime_profile.display_title(),
                            workflow_summary(&definition.workflow_description)
                        )),
                ),
        )
        .when(on_canvas, |this| {
            this.child(
                div()
                    .id(("settings-agent-list-on-canvas-dot", id as u64))
                    .w(px(8.0))
                    .h(px(8.0))
                    .flex_none()
                    .rounded(px(4.0))
                    .bg(rgba(theme.accent_green)),
            )
        })
        .into_any_element()
}

fn workflow_summary(workflow: &str) -> String {
    let trimmed = workflow.trim();
    if trimmed.is_empty() {
        "No workflow instructions".to_string()
    } else if trimmed.chars().count() > 64 {
        format!("{}…", trimmed.chars().take(63).collect::<String>())
    } else {
        trimmed.to_string()
    }
}
