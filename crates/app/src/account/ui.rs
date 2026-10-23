//! 登录相关的共享界面片段：启动登录页与设置 → 账号页共用，避免两处各写一份。

use super::{logo_asset, AccountController};
use account::{FailReason, Provider};
use gpui::{div, img, prelude::*, px, AnyElement, App, Entity, Hsla, Rgba};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};

fn hsla(color: u32) -> Hsla {
    Rgba::from(gpui::rgb(color)).into()
}

/// 失败原因对应的本地化提示（不含服务端细节，避免泄露内部信息）。
pub(crate) fn failure_text(provider: Provider, reason: &FailReason) -> ui::i18n::LocalizedText {
    let name = provider.display_name().to_string();
    match reason {
        FailReason::ProviderDenied(_) => {
            ui::i18n::message("account.failed_denied", &[("provider", name)])
        }
        FailReason::TimedOut => ui::i18n::LocalizedText::new("account.failed_timeout"),
        FailReason::StateMismatch => ui::i18n::LocalizedText::new("account.failed_state"),
        FailReason::InvalidCallback => ui::i18n::LocalizedText::new("account.failed_invalid"),
        FailReason::Exchange(_) => ui::i18n::LocalizedText::new("account.failed_exchange"),
        FailReason::Listener(_) => ui::i18n::LocalizedText::new("account.failed_listener"),
    }
}

/// 头像首字母：取显示名第一个字符的大写形式；空名回退为 "?"。
pub(crate) fn avatar_initial(display_name: &str) -> String {
    display_name
        .trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_string())
}

/// 提供方品牌按钮：GitHub 浅灰白底、Google 白底，遵循各自品牌规范，不随主题变色。
pub(crate) fn provider_button(
    id: &'static str,
    provider: Provider,
    width: f32,
    account: &Entity<AccountController>,
    cx: &App,
) -> AnyElement {
    let (label, background, foreground, hover, active) = match provider {
        Provider::GitHub => (
            ui::i18n::text("account.continue_with_github"),
            0xf0f6fc,
            0x0d1117,
            0xdfe8f1,
            0xcfd9e4,
        ),
        Provider::Google => (
            ui::i18n::text("account.continue_with_google"),
            0xffffff,
            0x1f1f1f,
            0xeceff1,
            0xdde1e4,
        ),
    };
    let style = ButtonCustomVariant::new(cx)
        .color(hsla(background))
        .foreground(hsla(foreground))
        .border(hsla(background))
        .hover(hsla(hover))
        .active(hsla(active));
    let account = account.clone();
    div()
        .debug_selector(move || id.to_string())
        .w(px(width))
        .child(
            Button::new(id)
                .custom(style)
                .w_full()
                .h(px(40.0))
                .rounded(px(10.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(9.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(img(logo_asset(provider)).size(px(18.0)))
                        .child(label),
                )
                .on_click(move |_, _, cx| {
                    account.update(cx, |account, cx| account.begin(provider, cx))
                }),
        )
        .into_any_element()
}
