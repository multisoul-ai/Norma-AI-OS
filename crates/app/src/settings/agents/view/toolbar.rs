//! Agent 库工具栏：搜索框与状态筛选 Chip。

use super::super::components::with_alpha;
use super::super::{AgentDefinitionsState, AgentStatusFilter};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use gpui::{
    div, prelude::*, px, rgba, AnyElement, AppContext, Context, Entity, Focusable, KeyDownEvent,
    MouseButton, Subscription, Window,
};
use ui::component::input::{Input, InputEvent, InputState};
use ui::component::{Icon, IconName};
use ui::Theme;

struct AgentSearchInputState {
    input: Entity<InputState>,
    _subscription: Subscription,
}

pub(super) fn agent_status_filter(
    theme: Theme,
    selected: AgentStatusFilter,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    div()
        .id("settings-agent-status-filter")
        .flex_none()
        .flex()
        .flex_row()
        .gap(px(8.0))
        .child(status_filter_button(
            theme,
            ui::i18n::text("settings.all"),
            "settings-agent-filter-all",
            AgentStatusFilter::All,
            selected,
            cx,
        ))
        .child(status_filter_button(
            theme,
            ui::i18n::text("settings.enabled"),
            "settings-agent-filter-enabled",
            AgentStatusFilter::Enabled,
            selected,
            cx,
        ))
        .child(status_filter_button(
            theme,
            ui::i18n::text("settings.disabled"),
            "settings-agent-filter-disabled",
            AgentStatusFilter::Disabled,
            selected,
            cx,
        ))
        .into_any_element()
}

/// Material Filter Chip：选中时淡强调色底并带对勾。
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
        .h(px(32.0))
        .px(px(12.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(if is_selected {
            with_alpha(theme.accent_orange, 0x00)
        } else {
            with_alpha(theme.text_secondary, 0x80)
        }))
        .bg(rgba(if is_selected {
            with_alpha(theme.accent_orange, 0x2e)
        } else {
            with_alpha(theme.text_primary, 0x00)
        }))
        .settings_font(TextRole::Control)
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
        .active(move |style| style.bg(rgba(with_alpha(theme.accent_orange, 0x44))))
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
        .when(is_selected, |this| {
            this.child(Icon::new(IconName::Check).size_3())
        })
        .child(label)
        .into_any_element()
}

pub(super) fn agent_search_input(
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
                    .placeholder(ui::i18n::input_placeholder(
                        "settings.search_agents",
                        window,
                        cx,
                    ))
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
        .flex_1()
        .min_w(px(0.0))
        .h(px(40.0))
        .flex()
        .items_center()
        .px(px(12.0))
        .rounded_full()
        .border_1()
        .border_color(rgba(theme.text_secondary))
        .bg(rgba(with_alpha(theme.text_primary, 0x0d)))
        .settings_text(TextRole::Body, theme)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            focus.focus(window);
            cx.stop_propagation();
        })
        .child(
            Input::new(&input)
                .settings_font(TextRole::Body)
                .appearance(false)
                .cleanable(true)
                .prefix(Icon::new(IconName::Search).text_color(rgba(theme.text_secondary))),
        )
        .into_any_element()
}
