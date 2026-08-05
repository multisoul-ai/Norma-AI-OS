use super::components::with_alpha;
use super::{AgentDefinitionField, AgentDefinitionsState};
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, AppContext, Context, CursorStyle, Entity, FocusHandle, Focusable,
    FontWeight, KeyDownEvent, MouseButton, Subscription, Window,
};
use ui::component::input::{Input, InputEvent, InputState};
use ui::Theme;

struct AgentFieldInputState {
    input: Entity<InputState>,
    _subscription: Subscription,
}

pub(super) fn editable_field(
    theme: Theme,
    state: &AgentDefinitionsState,
    field: AgentDefinitionField,
    label: &'static str,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> gpui::Div {
    let (id, min_height) = field_layout(field);
    let editing = state.is_editing(field);
    let mut surface = div()
        .id(id)
        .debug_selector(move || id.to_string())
        .w_full()
        .min_h(px(min_height))
        .p(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(if editing {
            theme.accent_orange
        } else {
            theme.text_secondary
        }))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .text_size(px(13.0))
        .text_color(rgba(if editing {
            theme.accent_orange
        } else {
            theme.text_primary
        }))
        .line_height(px(20.0));

    if editing {
        let (input, input_focus) = native_input(state, field, min_height, window, cx);
        surface = surface
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                input_focus.focus(window);
                cx.stop_propagation();
            })
            .child(input);
    } else {
        surface = surface
            .cursor_pointer()
            .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xd4))))
            .tab_index(0)
            .focus(move |style| style.border_color(rgba(theme.accent_orange)))
            .on_click(cx.listener(move |app, event, window, cx| {
                app.on_agent_settings_begin_edit(field, event, window, cx);
            }))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    app.agent_definitions.begin_edit(field);
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .child(state.row_value(field));
    }

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(theme.text_secondary))
                .child(label),
        )
        .child(surface)
}

fn native_input(
    state: &AgentDefinitionsState,
    field: AgentDefinitionField,
    min_height: f32,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> (Input, FocusHandle) {
    let app = cx.weak_entity();
    let initial_value = state.edit_buffer().to_string();
    let placeholder = match field {
        AgentDefinitionField::Name => "Set the agent name.",
        AgentDefinitionField::WorkflowDescription => {
            "Describe the workflow this agent should follow."
        }
    };
    let field_state = window.use_keyed_state(
        ("settings-agent-native-input", field_index(field)),
        cx,
        move |window, cx| {
            let input = cx.new(|cx| {
                let input = InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(initial_value);
                if field == AgentDefinitionField::WorkflowDescription {
                    input.multi_line(true)
                } else {
                    input
                }
            });
            let subscription = cx.subscribe(&input, move |_, input, event: &InputEvent, cx| {
                let Some(app) = app.upgrade() else {
                    return;
                };
                let value = input.read(cx).value().to_string();
                app.update(cx, |app, cx| match event {
                    InputEvent::Change => {
                        if app.agent_definitions.replace_edit_buffer(field, value) {
                            cx.notify();
                        }
                    }
                    InputEvent::PressEnter { .. } if field == AgentDefinitionField::Name => {
                        app.agent_definitions.replace_edit_buffer(field, value);
                        if app.agent_definitions.commit_active_edit(field) {
                            cx.notify();
                        }
                    }
                    InputEvent::Blur => {
                        app.agent_definitions.replace_edit_buffer(field, value);
                        if app.agent_definitions.commit_active_edit(field) {
                            cx.notify();
                        }
                    }
                    InputEvent::Focus | InputEvent::PressEnter { .. } => {}
                });
            });
            let focus = input.focus_handle(cx);
            window.defer(cx, move |window, _cx| focus.focus(window));
            AgentFieldInputState {
                input,
                _subscription: subscription,
            }
        },
    );
    let input = field_state.read(cx).input.clone();
    let focus = input.focus_handle(cx);
    let editor = Input::new(&input).appearance(false);
    let editor = if field == AgentDefinitionField::WorkflowDescription {
        editor.h(px((min_height - 24.0).max(20.0)))
    } else {
        editor
    };
    (editor, focus)
}

fn field_layout(field: AgentDefinitionField) -> (&'static str, f32) {
    match field {
        AgentDefinitionField::Name => ("settings-agent-name-input", 42.0),
        AgentDefinitionField::WorkflowDescription => ("settings-agent-workflow-input", 120.0),
    }
}

fn field_index(field: AgentDefinitionField) -> u64 {
    match field {
        AgentDefinitionField::Name => 0,
        AgentDefinitionField::WorkflowDescription => 1,
    }
}
