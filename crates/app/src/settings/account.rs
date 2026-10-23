//! 设置 → 账号：未登录显示登录卡片，已登录显示资料、登录方式与退出登录（原型变体 A）。

use crate::account::{
    avatar_initial, failure_text, logo_asset, provider_button, AccountController, AccountHandle,
};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use account::{FlowState, Profile, Provider};
use gpui::{div, img, prelude::*, px, rgba, App, Div, Entity, Stateful};
use ui::{
    component::button::{Button, ButtonVariants},
    Theme,
};

const PROVIDER_BUTTON_WIDTH: f32 = 220.0;

pub(super) fn settings(theme: Theme, cx: &mut App) -> Div {
    let body = div()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .py(px(24.0))
        .settings_font(TextRole::Body)
        .text_color(rgba(theme.text_primary));
    let Some(account) = cx
        .try_global::<AccountHandle>()
        .map(|handle| handle.0.clone())
    else {
        return body;
    };
    let state = account.read(cx).state().clone();
    match &state {
        FlowState::SignedIn(profile) => body
            .child(profile_card(theme, profile))
            .child(methods_card(theme, profile))
            .child(sign_out_row(theme, &account)),
        FlowState::AwaitingBrowser(attempt) => {
            body.child(waiting_card(theme, &account, attempt.provider))
        }
        FlowState::Exchanging(provider) => body.child(waiting_card(theme, &account, *provider)),
        FlowState::Failed { provider, reason } => body.child(sign_in_card(
            theme,
            &account,
            Some(failure_text(*provider, reason).to_string()),
            cx,
        )),
        FlowState::Idle => body.child(sign_in_card(theme, &account, None, cx)),
    }
}

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

fn avatar(profile: &Profile, size: f32) -> Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(size / 2.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgb(0x6f8dff))
        .text_color(gpui::rgb(0xffffff))
        .text_size(px(size * 0.4))
        .font_weight(gpui::FontWeight::BOLD)
        .child(avatar_initial(&profile.display_name))
}

fn sign_in_card(
    theme: Theme,
    account: &Entity<AccountController>,
    failure: Option<String>,
    cx: &App,
) -> Stateful<Div> {
    let mut body = card(theme)
        .id("account-sign-in-card")
        .debug_selector(|| "account-sign-in-card".to_string())
        .items_center()
        .text_center()
        .py(px(28.0))
        .child(img(crate::assets::APP_ICON_ASSET).size(px(56.0)))
        .child(
            div()
                .settings_text(TextRole::Headline, theme)
                .child(ui::i18n::text("account.sign_in_title")),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("account.sign_in_subtitle")),
        );
    if let Some(failure) = failure {
        body = body.child(
            div()
                .id("account-error")
                .debug_selector(|| "account-error".to_string())
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(9.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.destructive, 0x4d)))
                .bg(rgba(with_alpha(theme.destructive, 0x1a)))
                .settings_text(TextRole::Helper, theme)
                .text_color(rgba(theme.destructive))
                .child(failure)
                .child(
                    div()
                        .mt(px(2.0))
                        .settings_text(TextRole::Helper, theme)
                        .child(ui::i18n::text("account.failed_note")),
                ),
        );
    }
    body.child(
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(provider_button(
                "account-github",
                Provider::GitHub,
                PROVIDER_BUTTON_WIDTH,
                account,
                cx,
            ))
            .child(provider_button(
                "account-google",
                Provider::Google,
                PROVIDER_BUTTON_WIDTH,
                account,
                cx,
            )),
    )
    .child(
        div()
            .settings_text(TextRole::Caption, theme)
            .child(ui::i18n::text("account.privacy_note")),
    )
}

fn waiting_card(
    theme: Theme,
    account: &Entity<AccountController>,
    provider: Provider,
) -> Stateful<Div> {
    let (reopen, copy, cancel) = (account.clone(), account.clone(), account.clone());
    card(theme)
        .id("account-waiting-card")
        .debug_selector(|| "account-waiting-card".to_string())
        .items_center()
        .text_center()
        .py(px(28.0))
        .child(
            div().settings_text(TextRole::Headline, theme).child(
                ui::i18n::message(
                    "account.waiting_title",
                    &[("provider", provider.display_name().to_string())],
                )
                .to_string(),
            ),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("account.waiting_hint")),
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    Button::new("account-reopen")
                        .outline()
                        .child(control_label(ui::i18n::text("account.reopen_browser")))
                        .on_click(move |_, _, cx| {
                            reopen.update(cx, |account, cx| account.reopen_browser(cx))
                        }),
                )
                .child(
                    Button::new("account-copy")
                        .outline()
                        .child(control_label(ui::i18n::text("account.copy_link")))
                        .on_click(move |_, _, cx| {
                            copy.update(cx, |account, cx| account.copy_authorize_link(cx))
                        }),
                )
                .child(
                    Button::new("account-cancel")
                        .ghost()
                        .child(control_label(ui::i18n::text("account.cancel")))
                        .on_click(move |_, _, cx| {
                            cancel.update(cx, |account, cx| account.cancel(cx))
                        }),
                ),
        )
}

fn profile_card(theme: Theme, profile: &Profile) -> Stateful<Div> {
    card(theme)
        .id("account-profile-card")
        .debug_selector(|| "account-profile-card".to_string())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14.0))
                .child(avatar(profile, 56.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .settings_text(TextRole::Headline, theme)
                                .child(profile.display_name.clone()),
                        )
                        .child(
                            div()
                                .settings_text(TextRole::Helper, theme)
                                .child(profile.email.clone().unwrap_or_default()),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(10.0))
                        .py(px(3.0))
                        .rounded(px(10.0))
                        .bg(rgba(with_alpha(theme.success, 0x29)))
                        .settings_text(TextRole::Badge, theme)
                        .text_color(rgba(theme.success))
                        .child(ui::i18n::text("account.signed_in_badge")),
                ),
        )
        .child(
            div()
                .settings_text(TextRole::Caption, theme)
                .child(ui::i18n::text("account.session_local_note")),
        )
}

/// 只读的登录方式列表：本期不开放关联，数据模型已支持多身份。
fn methods_card(theme: Theme, profile: &Profile) -> Stateful<Div> {
    let mut list = card(theme)
        .id("account-methods-card")
        .debug_selector(|| "account-methods-card".to_string())
        .gap(px(4.0))
        .child(
            div()
                .settings_text(TextRole::Headline, theme)
                .child(ui::i18n::text("account.sign_in_methods")),
        );
    for (index, identity) in profile.identities.iter().enumerate() {
        list = list.child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .py(px(10.0))
                .when(index > 0, |row| {
                    row.border_t_1().border_color(rgba(theme.node_stroke))
                })
                .child(img(logo_asset(identity.provider)).size(px(20.0)))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .settings_text(TextRole::RowTitle, theme)
                                .child(identity.provider.display_name()),
                        )
                        .child(
                            div()
                                .settings_text(TextRole::Helper, theme)
                                .child(profile.email.clone().unwrap_or_default()),
                        ),
                )
                .when(index == 0, |row| {
                    row.child(
                        div()
                            .flex_none()
                            .px(px(10.0))
                            .py(px(3.0))
                            .rounded(px(10.0))
                            .bg(rgba(with_alpha(theme.accent, 0x29)))
                            .settings_text(TextRole::Badge, theme)
                            .text_color(rgba(theme.accent))
                            .child(ui::i18n::text("account.primary")),
                    )
                }),
        );
    }
    list
}

fn sign_out_row(theme: Theme, account: &Entity<AccountController>) -> Div {
    let account = account.clone();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .flex_1()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("account.sign_out_note")),
        )
        .child(
            Button::new("account-sign-out")
                .outline()
                .child(
                    control_label(ui::i18n::text("account.sign_out"))
                        .text_color(rgba(theme.destructive)),
                )
                .on_click(move |_, _, cx| account.update(cx, |account, cx| account.sign_out(cx))),
        )
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffff_ff00) | u32::from(alpha)
}
