use crate::mobile::controller::{Command, MobileController};
use gpui::{div, img, prelude::*, px, rgba, App, Div, Entity, SharedString};
use storage::MobilePermission;
use ui::{
    component::{button::Button, Disableable, Selectable},
    Theme,
};

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
    let status = if state.busy {
        "Working…"
    } else if state.enabled() {
        "Mobile connection is on"
    } else {
        "Mobile connection is off"
    };
    let mut controls = div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(div().flex_1().child(status));
    if state.enabled() {
        let target = entity.clone();
        controls = controls.child(
            Button::new("mobile-disable")
                .debug_selector(|| "mobile-disable".into())
                .label("Disable")
                .on_click(move |_, _, cx| target.update(cx, |host, cx| host.disable(cx))),
        );
    } else {
        controls = controls.child(action(
            entity,
            "mobile-enable",
            "Enable",
            Command::Enable,
            state.busy,
        ));
    }
    body = body.child(controls).child(push::entry(theme, entity, state.busy)).child(div().text_size(px(12.0)).text_color(rgba(theme.text_secondary))
        .child("With Mobile Connection enabled, projects and agents keep running after all windows close. Quit Norma to stop them. Disconnecting your phone does not stop agents."));
    if let Some(error) = &state.error {
        body = body.child(
            div()
                .text_color(rgba(theme.destructive))
                .child(error.clone()),
        );
    }
    if state.enabled() {
        body = body.child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_secondary))
                .child(format!(
                    "Connection addresses: {}",
                    state.addresses().join(" · ")
                )),
        );
    }
    if let Some(pending) = &state.pending {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(format!("{} wants to pair", pending.device_name))
                .child(
                    div()
                        .text_size(px(30.0))
                        .child(pending.verification_code.clone()),
                )
                .child(
                    div()
                        .text_color(rgba(theme.text_secondary))
                        .child("Confirm that your iPhone shows the same code, then choose which projects it can access."),
                ),
        );
        body = body.child(projects(theme, entity, cx));
        body = body.child(
            div()
                .flex()
                .gap(px(12.0))
                .child(action(
                    entity,
                    "mobile-approve",
                    "Approve Pairing",
                    Command::Approve {
                        request_id: pending.request_id.clone(),
                        projects: state.chosen_projects(),
                    },
                    state.busy,
                ))
                .child(action(
                    entity,
                    "mobile-reject",
                    "Reject",
                    Command::CancelCode,
                    state.busy,
                )),
        );
    } else if let Some(image) = &state.qr {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(12.0))
                .child("In Norma on your iPhone, choose Add Computer, then enter the code or scan the QR code.")
                .child("Pairing Code")
                .child(div().id("mobile-code").debug_selector(|| "mobile-code".into())
                    .text_size(px(32.0)).child(state.pairing_code.clone().unwrap_or_default()))
                .child(div().text_size(px(12.0)).text_color(rgba(theme.text_secondary))
                    .child("Enter the Mac address above and the 6-digit code on your iPhone. Each code allows up to 3 attempts. You can also scan the QR code below."))
                .child(
                    div()
                        .id("mobile-qr")
                        .debug_selector(|| "mobile-qr".into())
                        .rounded(px(10.0))
                        .bg(gpui::white())
                        .p(px(8.0))
                        .child(img(image.clone()).w(px(144.0)).h(px(144.0))),
                )
                .child(format!(
                    "Code and QR code expire in {} seconds",
                    state.seconds_left
                ))
                .child(
                    Button::new("mobile-copy-code")
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
                )
                .child(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .child(action(
                            entity,
                            "mobile-new-code",
                            "New Code",
                            Command::NewCode,
                            state.busy,
                        ))
                        .child(action(
                            entity,
                            "mobile-cancel-code",
                            "Cancel Pairing",
                            Command::CancelCode,
                            state.busy,
                        )),
                ),
        );
    } else if state.enabled() && state.editing.is_none() {
        body = body.child(action(
            entity,
            "mobile-new-code",
            "Pair a Phone",
            Command::NewCode,
            state.busy,
        ));
    }
    if let Some((device_id, revision)) = &state.editing {
        body = body
            .child("Edit Project Access")
            .child(projects(theme, entity, cx))
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(action(
                        entity,
                        "mobile-save-permissions",
                        "Save Permissions",
                        Command::Permissions {
                            device_id: device_id.clone(),
                            revision: *revision,
                            projects: state.chosen_projects(),
                        },
                        state.busy,
                    ))
                    .child(action(
                        entity,
                        "mobile-cancel-permissions",
                        "Cancel",
                        Command::Reload,
                        state.busy,
                    )),
            );
    }
    let mut devices = div().flex().flex_col().gap(px(12.0)).child("Paired Phones");
    for (index, device) in state
        .devices
        .iter()
        .filter(|device| !device.revoked)
        .enumerate()
    {
        let target = entity.clone();
        let editing = device.clone();
        devices = devices.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .py(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(device.device_name.clone())
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(format!(
                                    "{} projects · Permission version {}",
                                    device.projects.len(),
                                    device.revision
                                )),
                        ),
                )
                .child(
                    Button::new(("mobile-edit", index))
                        .label("Permissions")
                        .disabled(state.busy || !state.enabled() || state.qr.is_some())
                        .on_click(move |_, _, cx| {
                            target.update(cx, |host, cx| host.edit(&editing, cx))
                        }),
                )
                .child(action(
                    entity,
                    &format!("mobile-revoke-{index}"),
                    "Unpair",
                    Command::Revoke(device.device_id.clone()),
                    state.busy,
                )),
        );
    }
    if state.devices.iter().all(|device| device.revoked) {
        devices = devices.child(
            div()
                .text_color(rgba(theme.text_secondary))
                .child("No paired phones yet"),
        );
    }
    body.child(devices)
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
