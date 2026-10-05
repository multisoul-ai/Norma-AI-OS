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
        ui::i18n::text("mobile.paired_phones"),
        ui::i18n::text("mobile.manage_access_for_phones_you_ve_already_paired").into(),
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
                .child(ui::i18n::text("mobile.no_paired_phones_yet")),
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
                                .child(ui::i18n::format(
                                    "mobile.projects_permission_version",
                                    &[
                                        (
                                            "arg0",
                                            (match device.all_projects_access {
                                                Some(storage::MobilePermission::Control) => {
                                                    ui::i18n::text("mobile.all_projects_control")
                                                }
                                                Some(storage::MobilePermission::Read) => {
                                                    ui::i18n::text("mobile.all_projects_read_only")
                                                }
                                                None => ui::i18n::text("mobile.selected_projects"),
                                            })
                                            .to_string(),
                                        ),
                                        ("arg1", (device.projects.len()).to_string()),
                                        ("arg2", (device.revision).to_string()),
                                    ],
                                )),
                        ),
                )
                .child(
                    Button::new(("mobile-edit", index))
                        .label(ui::i18n::text("mobile.permissions"))
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
                        ui::i18n::text("mobile.unpair"),
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
            ui::i18n::text("mobile.edit_project_access"),
            ui::i18n::text(
                "mobile.choose_access_for_this_phone_including_future_projects_if_needed",
            )
            .into(),
        ))
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_secondary))
                .child(match current_scope {
                    Some(MobilePermission::Control) => {
                        ui::i18n::text("mobile.current_all_current_and_future_projects_control")
                    }
                    Some(MobilePermission::Read) => {
                        ui::i18n::text("mobile.current_all_current_and_future_projects_read_only")
                    }
                    None => ui::i18n::text("mobile.current_selected_projects_only"),
                }),
        )
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(action(
                    entity,
                    "mobile-all-read",
                    ui::i18n::text("mobile.all_projects_read_only_2"),
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
                        ui::i18n::text("mobile.all_projects_control_2"),
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
            ui::i18n::text("mobile.selected_projects"),
            ui::i18n::text("mobile.saving_this_list_ends_access_to_future_projects").into(),
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
                        ui::i18n::text("mobile.save_permissions"),
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
                        ui::i18n::text("common.cancel"),
                        Command::Reload,
                        host.busy,
                    )
                    .ghost(),
                ),
        )
}
