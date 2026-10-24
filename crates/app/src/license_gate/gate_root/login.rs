//! 启动登录页：Launchpad 专用壁纸 + 居中品牌与磨砂卡片（原型变体 D）。

use super::*;
use crate::account::{failure_text, provider_button};
use account::{FlowState, Provider};
use gpui::{img, AnyElement};
use ui::component::button::{Button, ButtonVariants as _};

const CARD_WIDTH: f32 = 340.0;
const BUTTON_WIDTH: f32 = 260.0;

impl GateRoot {
    pub(super) fn render_login(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(account) = self.account.clone() else {
            return div().size_full().into_any_element();
        };
        let state = account.read(cx).state().clone();
        let card = match &state {
            FlowState::Idle => self.login_choices(&account, None, cx),
            FlowState::Failed { provider, reason } => {
                self.login_choices(&account, Some(failure_text(*provider, reason)), cx)
            }
            FlowState::AwaitingBrowser(attempt) => {
                self.login_waiting(&account, attempt.provider, cx)
            }
            FlowState::Exchanging(provider) => self.login_waiting(&account, *provider, cx),
            FlowState::SignedIn(profile) => self.login_welcome(&account, profile, cx),
        };

        #[cfg(target_os = "macos")]
        let video_background = self.render_license_video_layer();
        #[cfg(not(target_os = "macos"))]
        let video_background = div().size_full();

        let content = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .text_color(gpui::rgb(0xeff7ff))
            .child(img(crate::assets::APP_ICON_ASSET).size(px(84.0)))
            .child(
                div()
                    .mb(px(14.0))
                    .text_size(px(30.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Norma OS"),
            )
            .child(
                div()
                    .id("login-card")
                    .debug_selector(|| "login-card".to_string())
                    .w(px(CARD_WIDTH))
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(gpui::rgba(0xffffff2e))
                    .bg(gpui::rgba(0x0a1828d9))
                    .child(card),
            );

        div()
            .id("login-shell")
            .debug_selector(|| "login-shell".to_string())
            .key_context("LoginShell")
            .track_focus(&self.focus_handle)
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(gpui::rgb(0x071324))
            .child(
                // 视频首帧出画之前的静态壁纸，避免空白闪烁。
                img(crate::assets::HOME_BACKGROUND_ASSET)
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .object_fit(gpui::ObjectFit::Cover),
            )
            .child(video_background)
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .bg(gpui::rgba(0x071324b8)),
            )
            .child(content)
            .on_action(cx.listener(|_this, _: &QuitNormaOs, _w, cx| {
                crate::settings::general::quit_after_saving(cx)
            }))
            .into_any_element()
    }

    fn login_choices(
        &mut self,
        account: &Entity<AccountController>,
        failure: Option<ui::i18n::LocalizedText>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut column = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(ui::i18n::text("account.sign_in_title")),
            )
            .child(
                div()
                    .mb(px(6.0))
                    .text_size(px(12.5))
                    .text_color(gpui::rgba(0xeff7ffa6))
                    .child(ui::i18n::text("account.sign_in_subtitle")),
            );
        if let Some(failure) = failure {
            column = column.child(
                div()
                    .id("login-error")
                    .debug_selector(|| "login-error".to_string())
                    .w(px(BUTTON_WIDTH))
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(9.0))
                    .border_1()
                    .border_color(gpui::rgba(0xff6b7a4d))
                    .bg(gpui::rgba(0xff6b7a1a))
                    .text_size(px(12.5))
                    .text_color(gpui::rgb(0xff9aa5))
                    .child(failure.to_string())
                    .child(
                        div()
                            .mt(px(2.0))
                            .text_color(gpui::rgba(0xeff7ff99))
                            .child(ui::i18n::text("account.failed_note")),
                    ),
            );
        }
        column
            .child(provider_button(
                "login-github",
                Provider::GitHub,
                BUTTON_WIDTH,
                account,
                cx,
            ))
            .child(provider_button(
                "login-google",
                Provider::Google,
                BUTTON_WIDTH,
                account,
                cx,
            ))
            .child(
                Button::new("login-skip")
                    .ghost()
                    .label(ui::i18n::text("account.skip_for_now"))
                    .text_color(gpui::rgba(0xeff7ffb3))
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        if let Some(account) = this.account.clone() {
                            account.update(cx, |account, cx| account.dismiss_login_prompt(cx));
                        }
                        this.enter_workbench = true;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(gpui::rgba(0xeff7ff80))
                    .child(ui::i18n::text("account.privacy_note")),
            )
            .into_any_element()
    }

    fn login_waiting(
        &mut self,
        account: &Entity<AccountController>,
        provider: Provider,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = provider.display_name().to_string();
        let (reopen, copy, cancel) = (account.clone(), account.clone(), account.clone());
        div()
            .id("login-waiting")
            .debug_selector(|| "login-waiting".to_string())
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(px(15.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(
                        ui::i18n::message("account.waiting_title", &[("provider", name)])
                            .to_string(),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .text_center()
                    .mb(px(6.0))
                    .text_size(px(12.5))
                    .text_color(gpui::rgba(0xeff7ffa6))
                    .child(ui::i18n::text("account.waiting_hint")),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(8.0))
                    .child(
                        Button::new("login-reopen")
                            .outline()
                            .label(ui::i18n::text("account.reopen_browser"))
                            .on_click(move |_, _, cx| {
                                reopen.update(cx, |account, cx| account.reopen_browser(cx))
                            }),
                    )
                    .child(
                        Button::new("login-copy")
                            .outline()
                            .label(ui::i18n::text("account.copy_link"))
                            .on_click(move |_, _, cx| {
                                copy.update(cx, |account, cx| account.copy_authorize_link(cx))
                            }),
                    )
                    .child(
                        Button::new("login-cancel")
                            .ghost()
                            .label(ui::i18n::text("account.cancel"))
                            .on_click(move |_, _, cx| {
                                cancel.update(cx, |account, cx| account.cancel(cx))
                            }),
                    ),
            )
            .into_any_element()
    }

    fn login_welcome(
        &mut self,
        account: &Entity<AccountController>,
        profile: &account::Profile,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider = profile
            .primary_provider()
            .map(|provider| provider.display_name().to_string())
            .unwrap_or_default();
        let switch = account.clone();
        div()
            .id("login-welcome")
            .debug_selector(|| "login-welcome".to_string())
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(
                        ui::i18n::message(
                            "account.welcome_back",
                            &[("name", profile.display_name.clone())],
                        )
                        .to_string(),
                    ),
            )
            .child(
                div()
                    .mb(px(8.0))
                    .text_size(px(12.5))
                    .text_color(gpui::rgba(0xeff7ffa6))
                    .child(
                        ui::i18n::message("account.signed_in_via", &[("provider", provider)])
                            .to_string(),
                    ),
            )
            .child(
                Button::new("login-enter")
                    .primary()
                    .w(px(BUTTON_WIDTH))
                    .h(px(40.0))
                    .label(ui::i18n::text("account.enter_norma"))
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.enter_workbench = true;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("login-switch")
                    .ghost()
                    .label(ui::i18n::text("account.switch_account"))
                    .on_click(move |_, _, cx| {
                        switch.update(cx, |account, cx| account.sign_out(cx))
                    }),
            )
            .into_any_element()
    }
}
