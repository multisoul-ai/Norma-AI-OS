mod pairing;
use crate::mobile::controller::{Command, MobileController};
use gpui::{div, img, prelude::*, px, rgba, App, Div, Entity, FontWeight, SharedString};
use pairing::{pair_prompt_card, pending_card, qr_card};
use storage::MobilePermission;
use ui::{
    component::{
        button::{Button, ButtonVariants},
        Disableable, Icon, IconName, Selectable,
    },
    Theme,
};

mod devices;
mod push;

pub(super) fn settings(theme: Theme, entity: &Entity<MobileController>, cx: &mut App) -> Div {
    let state = entity.read(cx);
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .py(px(24.0))
        .text_size(px(14.0))
        .text_color(rgba(theme.text_primary));

    body = body.child(connection_card(theme, entity, state));
    if let Some(error) = &state.error {
        body = body.child(error_banner(theme, error));
    }
    body = body.child(push::entry(theme, entity, state.busy));

    if let Some(pending) = &state.pending {
        body = body.child(pending_card(theme, entity, cx, pending));
    } else if let Some(image) = &state.qr {
        body = body.child(qr_card(theme, entity, state, image));
    } else if state.enabled() && state.editing.is_none() {
        body = body.child(pair_prompt_card(entity, state.busy));
    }

    if let Some((device_id, revision)) = &state.editing {
        body = body.child(devices::editing_card(
            theme, entity, cx, device_id, *revision,
        ));
    }

    body.child(devices::devices_card(theme, entity, state))
}

/// Base card shell shared by every section: rounded, bordered, tinted panel.
fn card(theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .p(px(18.0))
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba(theme.node_stroke))
        .bg(rgba(theme.panel_high))
}

fn section_title(theme: Theme, title: &'static str, subtitle: SharedString) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba(theme.text_primary))
                .child(title),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_tertiary))
                .child(subtitle),
        )
}

fn status_dot(color: u32) -> Div {
    div()
        .flex_none()
        .w(px(8.0))
        .h(px(8.0))
        .rounded(px(4.0))
        .bg(rgba(color))
}

fn connection_card(
    theme: Theme,
    entity: &Entity<MobileController>,
    state: &MobileController,
) -> Div {
    let (dot, status) = if state.busy {
        (theme.text_tertiary, ui::i18n::text("mobile.working"))
    } else if state.enabled() {
        (
            theme.success,
            ui::i18n::text("mobile.mobile_connection_is_on"),
        )
    } else {
        (
            theme.text_tertiary,
            ui::i18n::text("mobile.mobile_connection_is_off"),
        )
    };
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(status_dot(dot))
        .child(div().flex_1().font_weight(FontWeight::MEDIUM).child(status));
    row = if state.enabled() {
        let target = entity.clone();
        row.child(
            Button::new("mobile-disable")
                .debug_selector(|| "mobile-disable".into())
                .label(ui::i18n::text("common.disable"))
                .danger()
                .outline()
                .disabled(state.busy)
                .on_click(move |_, _, cx| target.update(cx, |host, cx| host.disable(cx))),
        )
    } else {
        row.child(
            action(
                entity,
                "mobile-enable",
                ui::i18n::text("common.enable"),
                Command::Enable,
                state.busy,
            )
            .primary(),
        )
    };

    let mut body = card(theme)
        .child(section_title(
            theme,
            ui::i18n::text("mobile.connection"),
            ui::i18n::text("mobile.pair_your_iphone_to_run_norma_from_anywhere").into(),
        ))
        .child(row);
    if state.enabled() {
        body = body.child(div().flex().flex_wrap().gap(px(6.0)).children(
            state.addresses().into_iter().map(|address| {
                div()
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(8.0))
                    .bg(rgba(theme.panel))
                    .text_size(px(11.0))
                    .text_color(rgba(theme.text_secondary))
                    .child(address)
            }),
        ));
    }
    body.child(
        div()
            .text_size(px(12.0))
            .text_color(rgba(theme.text_tertiary))
            .child(ui::i18n::text(
                "mobile.projects_and_agents_keep_running_after_windows_close_while_this_is",
            )),
    )
}

fn error_banner(theme: Theme, error: &str) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.destructive, 0x40)))
        .bg(rgba(with_alpha(theme.destructive, 0x18)))
        .child(Icon::new(IconName::TriangleAlert).text_color(rgba(theme.destructive)))
        .child(
            div()
                .flex_1()
                .text_size(px(13.0))
                .text_color(rgba(theme.destructive))
                .child(error.to_string()),
        )
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

fn action(
    entity: &Entity<MobileController>,
    id: &str,
    title: &str,
    command: Command,
    disabled: bool,
) -> Button {
    let target = entity.clone();
    let selector = id.to_string();
    Button::new(SharedString::from(id.to_string()))
        .debug_selector(move || selector.clone())
        .label(title.to_string())
        .outline()
        .disabled(disabled)
        .on_click(move |_, _, cx| target.update(cx, |host, cx| host.request(command.clone(), cx)))
}

fn projects(theme: Theme, entity: &Entity<MobileController>, cx: &App) -> Div {
    let host = entity.read(cx);
    let mut list = div().flex().flex_col().gap(px(10.0));
    for project in &host.projects {
        let mut choices = div().flex().gap(px(6.0));
        for (index, (label, access)) in [
            (ui::i18n::text("mobile.no_access"), None),
            (
                ui::i18n::text("mobile.read_only"),
                Some(MobilePermission::Read),
            ),
            (
                ui::i18n::text("mobile.control"),
                Some(MobilePermission::Control),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let target = entity.clone();
            let project_id = project.workspace_id.clone();
            choices = choices.child(
                Button::new(SharedString::from(format!(
                    "mobile-project-{project_id}-{index}"
                )))
                .label(label)
                .selected(host.selected.get(&project_id).copied() == access)
                .disabled(host.busy)
                .on_click(move |_, _, cx| {
                    target.update(cx, |host, cx| {
                        if let Some(access) = access {
                            host.selected.insert(project_id.clone(), access);
                        } else {
                            host.selected.remove(&project_id);
                        }
                        cx.notify();
                    })
                }),
            );
        }
        list = list.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .py(px(8.0))
                .child(project.display_name.clone())
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_secondary))
                        .child(project.canonical_root.display().to_string()),
                )
                .child(choices),
        );
    }
    list.child(
        div()
            .text_size(px(12.0))
            .text_color(rgba(theme.text_secondary))
            .child(ui::i18n::text(
                "mobile.if_no_projects_are_selected_your_phone_will_pair_without_access",
            )),
    )
}
