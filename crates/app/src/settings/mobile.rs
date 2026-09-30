use crate::mobile::controller::{Command, MobileController};
use gpui::{div, img, prelude::*, px, rgba, App, Div, Entity, FontWeight, SharedString};
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
        (theme.text_tertiary, "Working…")
    } else if state.enabled() {
        (theme.success, "Mobile connection is on")
    } else {
        (theme.text_tertiary, "Mobile connection is off")
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
                .label("Disable")
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
                "Enable",
                Command::Enable,
                state.busy,
            )
            .primary(),
        )
    };

    let mut body = card(theme)
        .child(section_title(
            theme,
            "Connection",
            "Pair your iPhone to run Norma from anywhere.".into(),
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
            .child("Projects and agents keep running after windows close while this is on. Quit Norma to stop them; disconnecting your phone does not."),
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

fn pair_prompt_card(entity: &Entity<MobileController>, busy: bool) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .py(px(22.0))
        .child(
            action(
                entity,
                "mobile-new-code",
                "Pair a Phone",
                Command::NewCode,
                busy,
            )
            .primary(),
        )
}

fn qr_card(
    theme: Theme,
    entity: &Entity<MobileController>,
    state: &MobileController,
    image: &std::sync::Arc<gpui::RenderImage>,
) -> Div {
    card(theme)
        .child(section_title(theme, "Pair a Phone", "In Norma on your iPhone, choose Add Computer, then enter the code or scan the QR code.".into()))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(24.0))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_tertiary))
                                .child("Pairing Code"),
                        )
                        .child(
                            div()
                                .id("mobile-code")
                                .debug_selector(|| "mobile-code".into())
                                .text_size(px(32.0))
                                .font_weight(FontWeight::BOLD)
                                .child(state.pairing_code.clone().unwrap_or_default()),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_tertiary))
                                .child("Enter the Mac address above and the 6-digit code on your iPhone. Each code allows up to 3 attempts."),
                        )
                        .child(format!("Expires in {} seconds", state.seconds_left))
                        .child(
                            Button::new("mobile-copy-code")
                                .icon(if state.code_copied {
                                    IconName::Check
                                } else {
                                    IconName::Copy
                                })
                                .label(if state.code_copied {
                                    "Code Copied"
                                } else {
                                    "Copy Code"
                                })
                                .outline()
                                .disabled(state.busy || state.seconds_left == 0)
                                .on_click({
                                    let target = entity.clone();
                                    move |_, _, cx| {
                                        target.update(cx, |host, cx| host.copy_pairing_code(cx))
                                    }
                                }),
                        ),
                )
                .child(
                    div()
                        .id("mobile-qr")
                        .debug_selector(|| "mobile-qr".into())
                        .flex_none()
                        .rounded(px(10.0))
                        .bg(gpui::white())
                        .p(px(8.0))
                        .child(img(image.clone()).w(px(144.0)).h(px(144.0))),
                ),
        )
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(action(entity, "mobile-new-code", "New Code", Command::NewCode, state.busy))
                .child(
                    action(entity, "mobile-cancel-code", "Cancel Pairing", Command::CancelCode, state.busy)
                        .ghost(),
                ),
        )
}

fn pending_card(
    theme: Theme,
    entity: &Entity<MobileController>,
    cx: &App,
    pending: &norma::mobile::PairingPending,
) -> Div {
    let host = entity.read(cx);
    card(theme)
        .border_color(rgba(with_alpha(theme.accent_orange, 0x60)))
        .child(section_title(theme, "Pairing Request", format!("{} wants to pair", pending.device_name).into()))
        .child(
            div()
                .text_size(px(30.0))
                .font_weight(FontWeight::BOLD)
                .child(pending.verification_code.clone()),
        )
        .child(
            div()
                .text_color(rgba(theme.text_secondary))
                .child("Confirm that your iPhone shows the same code, then choose which projects it can access."),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_secondary))
                .child("One-time authorization for this iPhone: all projects registered on this Mac now and in the future."),
        )
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(
                    action(
                        entity,
                        "mobile-approve-all-read",
                        "Approve All · Read Only",
                        Command::ApproveAll {
                            request_id: pending.request_id.clone(),
                            access: MobilePermission::Read,
                        },
                        host.busy,
                    )
                    .outline(),
                )
                .child(
                    action(
                        entity,
                        "mobile-approve-all-control",
                        "Approve All · Control",
                        Command::ApproveAll {
                            request_id: pending.request_id.clone(),
                            access: MobilePermission::Control,
                        },
                        host.busy,
                    )
                    .primary(),
                ),
        )
        .child(section_title(theme, "Selected Projects", "Or grant access to individual projects only.".into()))
        .child(projects(theme, entity, cx))
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(
                    action(
                        entity,
                        "mobile-approve",
                        "Approve Pairing",
                        Command::Approve {
                            request_id: pending.request_id.clone(),
                            projects: host.chosen_projects(),
                        },
                        host.busy,
                    )
                    .primary(),
                )
                .child(
                    action(entity, "mobile-reject", "Reject", Command::CancelCode, host.busy)
                        .danger()
                        .outline(),
                ),
        )
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
            ("No Access", None),
            ("Read Only", Some(MobilePermission::Read)),
            ("Control", Some(MobilePermission::Control)),
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
            .child(
                "If no projects are selected, your phone will pair without access to any projects.",
            ),
    )
}
