use super::{action, card, section_title};
use crate::mobile::controller::{Command, MobileController};
use gpui::{div, prelude::*, px, rgba, Div, Entity};
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
                                    "{} projects · Permission version {}",
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
