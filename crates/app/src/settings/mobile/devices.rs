use super::{action, card, projects, section_title};
use crate::mobile::controller::{Command, MobileController};
use gpui::{div, prelude::*, px, rgba, App, Div, Entity};
use storage::MobilePermission;
use ui::{
    component::{
        button::{Button, ButtonVariants},
        Disableable, IconName,
    },
    Theme,
};

pub(super) fn devices_card(
    theme: Theme,
    entity: &Entity<MobileController>,
    state: &MobileController,
) -> Div {
    let mut body = card(theme).child(section_title(
        theme,
        "Paired Phones",
        "Manage access for phones you've already paired.".into(),
    ));
    let active: Vec<_> = state
        .devices
        .iter()
        .filter(|device| !device.revoked)
        .collect();
    if active.is_empty() {
        return body.child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_tertiary))
                .child("No paired phones yet"),
        );
    }
    for (index, device) in active.into_iter().enumerate() {
        let target = entity.clone();
        let editing = device.clone();
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .py(px(10.0))
                .px(px(4.0))
                .when(index > 0, |row| {
                    row.border_t_1().border_color(rgba(theme.node_stroke))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(device.device_name.clone())
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(format!(
                                    "{} · {} projects · Permission version {}",
                                    match device.all_projects_access {
                                        Some(storage::MobilePermission::Control) =>
                                            "All projects · Control",
                                        Some(storage::MobilePermission::Read) =>
                                            "All projects · Read Only",
                                        None => "Selected projects",
                                    },
                                    device.projects.len(),
                                    device.revision
                                )),
                        ),
                )
                .child(
                    Button::new(("mobile-edit", index))
                        .label("Permissions")
                        .outline()
                        .disabled(state.busy || !state.enabled() || state.qr.is_some())
                        .on_click(move |_, _, cx| {
                            target.update(cx, |host, cx| host.edit(&editing, cx))
                        }),
                )
                .child(
                    action(
                        entity,
                        &format!("mobile-revoke-{index}"),
                        "Unpair",
                        Command::Revoke(device.device_id.clone()),
                        state.busy,
                    )
                    .icon(IconName::Delete)
                    .danger(),
                ),
        );
    }
    body
}

pub(super) fn editing_card(
    theme: Theme,
    entity: &Entity<MobileController>,
    cx: &App,
    device_id: &str,
    revision: u64,
) -> Div {
    let host = entity.read(cx);
    let current_scope = host
        .devices
        .iter()
        .find(|device| device.device_id == device_id)
        .and_then(|device| device.all_projects_access);
    card(theme)
        .child(section_title(
            theme,
            "Edit Project Access",
            "Choose access for this phone, including future projects if needed.".into(),
        ))
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_secondary))
                .child(match current_scope {
                    Some(MobilePermission::Control) => {
                        "Current: all current and future projects · Control"
                    }
                    Some(MobilePermission::Read) => {
                        "Current: all current and future projects · Read Only"
                    }
                    None => "Current: selected projects only",
                }),
        )
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(action(
                    entity,
                    "mobile-all-read",
                    "All Projects · Read Only",
                    Command::PermissionsAll {
                        device_id: device_id.to_string(),
                        revision,
                        access: MobilePermission::Read,
                    },
                    host.busy,
                ))
                .child(
                    action(
                        entity,
                        "mobile-all-control",
                        "All Projects · Control",
                        Command::PermissionsAll {
                            device_id: device_id.to_string(),
                            revision,
                            access: MobilePermission::Control,
                        },
                        host.busy,
                    )
                    .primary(),
                ),
        )
        .child(section_title(
            theme,
            "Selected Projects",
            "Saving this list ends access to future projects.".into(),
        ))
        .child(projects(theme, entity, cx))
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(
                    action(
                        entity,
                        "mobile-save-permissions",
                        "Save Permissions",
                        Command::Permissions {
                            device_id: device_id.to_string(),
                            revision,
                            projects: host.chosen_projects(),
                        },
                        host.busy,
                    )
                    .primary(),
                )
                .child(
                    action(
                        entity,
                        "mobile-cancel-permissions",
                        "Cancel",
                        Command::Reload,
                        host.busy,
                    )
                    .ghost(),
                ),
        )
}
