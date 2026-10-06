use super::*;
use crate::settings::typography::{control_label, SettingsText, TextRole};

pub(super) fn pair_prompt_card(entity: &Entity<MobileController>, busy: bool) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .py(px(22.0))
        .child(
            action(
                entity,
                "mobile-new-code",
                ui::i18n::text("mobile.pair_a_phone"),
                Command::NewCode,
                busy,
            )
            .primary(),
        )
}

pub(super) fn qr_card(
    theme: Theme,
    entity: &Entity<MobileController>,
    state: &MobileController,
    image: &std::sync::Arc<gpui::RenderImage>,
) -> Div {
    card(theme)
        .child(section_title(
            theme,
            ui::i18n::text("mobile.pair_a_phone"),
            ui::i18n::text("mobile.in_norma_on_your_iphone_choose_add_computer_then_enter_the")
                .into(),
        ))
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
                                .settings_text(TextRole::Label, theme)
                                .child(ui::i18n::text("mobile.pairing_code")),
                        )
                        .child(
                            div()
                                .id("mobile-code")
                                .debug_selector(|| "mobile-code".into())
                                .settings_font(TextRole::Code)
                                .child(state.pairing_code.clone().unwrap_or_default()),
                        )
                        .child(
                            div()
                                .settings_text(TextRole::Helper, theme)
                                .child(ui::i18n::text(
                                    "mobile.enter_the_mac_address_above_and_the_6_digit_code_on",
                                )),
                        )
                        .child(ui::i18n::format(
                            "mobile.expires_in_seconds",
                            &[("arg0", (state.seconds_left).to_string())],
                        ))
                        .child(
                            Button::new("mobile-copy-code")
                                .icon(if state.code_copied {
                                    IconName::Check
                                } else {
                                    IconName::Copy
                                })
                                .child(control_label(if state.code_copied {
                                    ui::i18n::text("mobile.code_copied")
                                } else {
                                    ui::i18n::text("mobile.copy_code")
                                }))
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
                .child(action(
                    entity,
                    "mobile-new-code",
                    ui::i18n::text("mobile.new_code"),
                    Command::NewCode,
                    state.busy,
                ))
                .child(
                    action(
                        entity,
                        "mobile-cancel-code",
                        ui::i18n::text("mobile.cancel_pairing"),
                        Command::CancelCode,
                        state.busy,
                    )
                    .ghost(),
                ),
        )
}

pub(super) fn pending_card(
    theme: Theme,
    entity: &Entity<MobileController>,
    cx: &App,
    pending: &norma::mobile::PairingPending,
) -> Div {
    let host = entity.read(cx);
    card(theme)
        .border_color(rgba(with_alpha(theme.accent_orange, 0x60)))
        .child(section_title(
            theme,
            ui::i18n::text("mobile.pairing_request"),
            ui::i18n::format(
                "mobile.wants_to_pair",
                &[("arg0", (pending.device_name).to_string())],
            )
            .into(),
        ))
        .child(
            div()
                .settings_font(TextRole::Code)
                .child(pending.verification_code.clone()),
        )
        .child(
            div()
                .text_color(rgba(theme.text_secondary))
                .child(ui::i18n::text(
                    "mobile.confirm_that_your_iphone_shows_the_same_code_then_choose_which",
                )),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text(
                    "mobile.one_time_authorization_for_this_iphone_all_projects_registered_on_this",
                )),
        )
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(
                    action(
                        entity,
                        "mobile-approve-all-read",
                        ui::i18n::text("mobile.approve_all_read_only"),
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
                        ui::i18n::text("mobile.approve_all_control"),
                        Command::ApproveAll {
                            request_id: pending.request_id.clone(),
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
            ui::i18n::text("mobile.or_grant_access_to_individual_projects_only").into(),
        ))
        .child(projects(theme, entity, cx))
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(
                    action(
                        entity,
                        "mobile-approve",
                        ui::i18n::text("mobile.approve_pairing"),
                        Command::Approve {
                            request_id: pending.request_id.clone(),
                            projects: host.chosen_projects(),
                        },
                        host.busy,
                    )
                    .primary(),
                )
                .child(
                    action(
                        entity,
                        "mobile-reject",
                        ui::i18n::text("mobile.reject"),
                        Command::CancelCode,
                        host.busy,
                    )
                    .danger()
                    .outline(),
                ),
        )
}
