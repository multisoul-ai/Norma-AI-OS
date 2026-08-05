use super::super::components::{action_button, empty_list_message, with_alpha};
use super::super::{AgentDefinitionsState, AgentStatusFilter};
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, AnyElement, AppContext, Context, Div, Entity, Focusable, FontWeight,
    KeyDownEvent, MouseButton, MouseDownEvent, ScrollHandle, Subscription, Window,
};
use storage::AgentDefinition;
use ui::component::input::{Input, InputEvent, InputState};
use ui::Theme;

struct AgentSearchInputState {
    input: Entity<InputState>,
    _subscription: Subscription,
}

pub(super) fn agent_list(
    theme: Theme,
    state: &AgentDefinitionsState,
    list_scroll: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let definitions = state.visible_definitions();

    div()
        .w(px(224.0))
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
        .p(px(12.0))
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba(theme.text_secondary))
                        .child("Agent Library"),
                )
                .child(action_button(
                    theme,
                    "settings-agent-new",
                    "New Agent",
                    theme.accent_orange,
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
                .gap(px(3.0))
                .when(state.is_creating_new(), |this| {
                    this.child(unsaved_agent_row(theme, &state.draft().name))
                })
                .when(definitions.is_empty() && !state.is_creating_new(), |this| {
                    this.child(empty_list_message(
                        theme,
                        if state.definitions().is_empty() {
                            "No project agents saved yet."
                        } else {
                            "No agents match your search."
                        },
                    ))
                })
                .children(definitions.into_iter().map(|definition| {
                    agent_list_row(
                        theme,
                        definition,
                        state.selected_definition_id() == Some(definition.id),
                        cx,
                    )
                })),
        )
}

fn unsaved_agent_row(theme: Theme, name: &str) -> AnyElement {
    div()
        .id("settings-agent-list-row-unsaved")
        .debug_selector(|| "settings-agent-list-row-unsaved".to_string())
        .min_h(px(58.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.0))
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .bg(rgba(with_alpha(theme.accent_orange, 0x18)))
        .child(
            div()
                .w(px(3.0))
                .h_full()
                .rounded(px(2.0))
                .bg(rgba(theme.accent_orange)),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(3.0))
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
                        .text_size(px(11.0))
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
        .h(px(30.0))
        .flex_none()
        .flex()
        .flex_row()
        .rounded(px(7.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x38)))
        .bg(rgba(with_alpha(theme.panel_high, 0x58)))
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
        .rounded(px(5.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x00)))
        .bg(rgba(if is_selected {
            with_alpha(theme.accent_orange, 0x1f)
        } else {
            with_alpha(theme.panel_high, 0x00)
        }))
        .text_size(px(11.0))
        .font_weight(if is_selected {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .text_color(rgba(if is_selected {
            theme.accent_orange
        } else {
            theme.text_secondary
        }))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xb8))))
        .active(move |style| style.bg(rgba(with_alpha(theme.accent_orange, 0x2c))))
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
        .h(px(34.0))
        .flex_none()
        .flex()
        .items_center()
        .px(px(9.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(rgba(theme.text_secondary))
        .bg(rgba(with_alpha(theme.panel_high, 0x96)))
        .text_size(px(12.0))
        .text_color(rgba(theme.text_primary))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            focus.focus(window);
            cx.stop_propagation();
        })
        .child(Input::new(&input).appearance(false).cleanable(true))
        .into_any_element()
}

fn agent_list_row(
    theme: Theme,
    definition: &AgentDefinition,
    selected: bool,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let id = definition.id;
    let selector = format!("settings-agent-list-row-{id}");
    let initial = definition.name.chars().next().unwrap_or('A').to_string();
    let bg = if selected {
        with_alpha(theme.accent_orange, 0x18)
    } else {
        with_alpha(theme.panel_high, 0x00)
    };

    div()
        .id(("settings-agent-list-row", id as u64))
        .debug_selector(move || selector.clone())
        .min_h(px(68.0))
        .flex()
        .flex_row()
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x00)))
        .bg(rgba(bg))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xb0))))
        .active(move |style| style.bg(rgba(with_alpha(theme.accent_orange, 0x28))))
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
        .child(
            div()
                .w(px(3.0))
                .flex_none()
                .rounded(px(2.0))
                .bg(rgba(if selected {
                    theme.accent_orange
                } else {
                    with_alpha(theme.accent_orange, 0x00)
                })),
        )
        .child(
            div()
                .ml(px(8.0))
                .w(px(28.0))
                .h(px(28.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(14.0))
                .bg(rgba(with_alpha(theme.accent_orange, 0x18)))
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba(theme.accent_orange))
                .child(initial),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .pl(px(8.0))
                .pr(px(6.0))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(3.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
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
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgba(if definition.enabled {
                                    theme.accent_cyan
                                } else {
                                    theme.text_tertiary
                                }))
                                .child(if definition.enabled {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                }),
                        ),
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
