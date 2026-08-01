use super::super::components::{action_button, empty_list_message, with_alpha};
use super::super::AgentDefinitionsState;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, AnyElement, Context, Div, FontWeight, MouseButton, MouseDownEvent,
};
use storage::AgentDefinition;
use ui::Theme;

pub(super) fn agent_list(
    theme: Theme,
    state: &AgentDefinitionsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let definitions = state.definitions();

    div()
        .w(px(224.0))
        .flex_none()
        .flex()
        .flex_col()
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
                        .child("Project Agents"),
                )
                .child(
                    action_button(theme, "+ New", theme.accent_orange)
                        .id("settings-agent-new")
                        .debug_selector(|| "settings-agent-new".to_string())
                        .on_click(cx.listener(MultisoulApp::on_agent_settings_new_definition)),
                ),
        )
        .child(
            div()
                .id("settings-agents-list")
                .debug_selector(|| "settings-agents-list".to_string())
                .flex()
                .flex_col()
                .gap(px(3.0))
                .when(definitions.is_empty(), |this| {
                    this.child(empty_list_message(theme, "No project agents saved yet."))
                })
                .children(definitions.iter().map(|definition| {
                    agent_list_row(
                        theme,
                        definition,
                        state.selected_definition_id() == Some(definition.id),
                        cx,
                    )
                })),
        )
}

fn agent_list_row(
    theme: Theme,
    definition: &AgentDefinition,
    selected: bool,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let id = definition.id;
    let selector = format!("settings-agent-list-row-{id}");
    let bg = if selected {
        with_alpha(theme.accent_orange, 0x18)
    } else {
        with_alpha(theme.panel_high, 0x00)
    };

    div()
        .id(("settings-agent-list-row", id as u64))
        .debug_selector(move || selector.clone())
        .min_h(px(54.0))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(3.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .bg(rgba(bg))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xb0))))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                app.on_agent_settings_select_definition(id, event, window, cx);
            }),
        )
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba(theme.text_primary))
                .child(definition.name.clone()),
        )
        .child(
            div()
                .id(("settings-agent-list-runtime-profile", id as u64))
                .debug_selector(|| "settings-agent-list-runtime-profile".to_string())
                .text_size(px(12.0))
                .text_color(rgba(with_alpha(theme.text_primary, 0xc0)))
                .child(definition.runtime_profile.display_title().to_string()),
        )
        .into_any_element()
}
