use crate::settings::typography::{SettingsText, TextRole};
mod inspector;
mod list;

use self::inspector::agent_inspector;
use self::list::agent_list;
use super::components::{action_button, status_block, with_alpha};
use super::{AgentDefinitionsState, AgentDeleteDialog};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, ScrollHandle, Window};
use std::collections::HashSet;
use ui::Theme;

pub(crate) fn agents_settings(
    theme: Theme,
    state: &AgentDefinitionsState,
    selected_agent_on_canvas: bool,
    on_canvas_ids: &HashSet<i64>,
    list_scroll: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    if !state.has_project_store() {
        return div()
            .pt(px(8.0))
            .child(div().id("settings-agent-empty-project").child(status_block(
                theme,
                ui::i18n::text("settings.open_a_project_to_configure_project_agents"),
                false,
            )));
    }

    div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .gap(px(12.0))
        .pt(px(8.0))
        .child(master_detail(
            theme,
            state,
            selected_agent_on_canvas,
            on_canvas_ids,
            list_scroll,
            window,
            cx,
        ))
        .when_some(state.status(), |this, status| {
            this.child(status_block(theme, status, true))
        })
        .when_some(state.delete_dialog(), |this, dialog| {
            this.child(delete_agent_dialog(theme, dialog, cx))
        })
}

fn delete_agent_dialog(
    theme: Theme,
    dialog: &AgentDeleteDialog,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let (id, title, message, blocked) = match dialog {
        AgentDeleteDialog::Confirm { id, name } => (
            *id,
            ui::i18n::format("settings.delete_name", &[("name", (name).to_string())]),
            ui::i18n::text(
                "settings.this_removes_the_project_agent_definition_this_can_t_be_undone",
            ),
            false,
        ),
        AgentDeleteDialog::Blocked { id, name } => (
            *id,
            ui::i18n::format(
                "settings.cannot_delete_name",
                &[("name", (name).to_string())],
            ),
            ui::i18n::text(
                "settings.close_the_active_agent_card_on_canvas_before_deleting_this_agent",
            ),
            true,
        ),
    };

    div()
        .id("settings-agent-delete-dialog")
        .debug_selector(|| "settings-agent-delete-dialog".to_string())
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x00000052))
        .occlude()
        .child(
            div()
                .id("settings-agent-delete-dialog-card")
                .w(px(420.0))
                .max_w_full()
                .rounded(px(12.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x58)))
                .bg(rgba(theme.panel_high))
                .p(px(22.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(div().settings_text(TextRole::Headline, theme).child(title))
                .child(div().settings_text(TextRole::Helper, theme).child(message))
                .child(
                    div()
                        .pt(px(8.0))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0))
                        .child(action_button(
                            theme,
                            "settings-agent-delete-cancel",
                            if blocked {
                                ui::i18n::text("common.ok")
                            } else {
                                ui::i18n::text("common.cancel")
                            },
                            theme.text_secondary,
                            cx.listener(MultisoulApp::on_agent_settings_cancel_delete),
                        ))
                        .when(!blocked, |this| {
                            this.child(action_button(
                                theme,
                                "settings-agent-delete-confirm",
                                ui::i18n::text("settings.delete_agent"),
                                theme.destructive,
                                cx.listener(move |app, event, window, cx| {
                                    app.on_agent_settings_confirm_delete(id, event, window, cx);
                                }),
                            ))
                        }),
                ),
        )
        .into_any_element()
}

fn master_detail(
    theme: Theme,
    state: &AgentDefinitionsState,
    selected_agent_on_canvas: bool,
    on_canvas_ids: &HashSet<i64>,
    list_scroll: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    div()
        .id("settings-agents-master-detail")
        .debug_selector(|| "settings-agents-master-detail".to_string())
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_row()
        .overflow_hidden()
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x30)))
        .bg(rgba(with_alpha(theme.node_fill, 0xb8)))
        .child(agent_list(
            theme,
            state,
            on_canvas_ids,
            list_scroll,
            window,
            cx,
        ))
        .child(agent_detail(
            theme,
            state,
            selected_agent_on_canvas,
            window,
            cx,
        ))
        .into_any_element()
}

fn agent_detail(
    theme: Theme,
    state: &AgentDefinitionsState,
    selected_agent_on_canvas: bool,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    div()
        .id("settings-agent-detail")
        .debug_selector(|| "settings-agent-detail".to_string())
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .h_full()
        .overflow_hidden()
        .rounded_r(px(10.0))
        .p(px(20.0))
        .child(agent_inspector(
            theme,
            state,
            selected_agent_on_canvas,
            window,
            cx,
        ))
        .into_any_element()
}
