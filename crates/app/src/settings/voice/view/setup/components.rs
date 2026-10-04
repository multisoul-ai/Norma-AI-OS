use super::super::components::VoiceAction;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, App, ClickEvent, Context, Div, FontWeight, IntoElement,
    SharedString, Window,
};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;

#[derive(Clone, Copy)]
pub(super) enum SetupButtonTone {
    Default,
    Accent,
    Success,
    Destructive,
}

pub(super) fn setup_action_button(
    theme: Theme,
    selector: impl Into<SharedString>,
    label: impl Into<SharedString>,
    tone: SetupButtonTone,
    cx: &Context<MultisoulApp>,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let selector = selector.into();
    let debug_selector = selector.clone();
    let accent = match tone {
        SetupButtonTone::Default => theme.text_secondary,
        SetupButtonTone::Accent => theme.accent_blue,
        SetupButtonTone::Success => theme.accent_green,
        SetupButtonTone::Destructive => theme.destructive,
    };
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba((accent & 0xffffff00) | 0x18).into())
        .foreground(
            rgba(if matches!(tone, SetupButtonTone::Success) {
                theme.text_primary
            } else {
                accent
            })
            .into(),
        )
        .border(rgba((accent & 0xffffff00) | 0x70).into())
        .hover(rgba((accent & 0xffffff00) | 0x28).into())
        .active(rgba((accent & 0xffffff00) | 0x38).into());
    let button = Button::new(selector)
        .debug_selector(move || debug_selector.to_string())
        .label(label)
        .compact()
        .custom(variant)
        .on_activate(handler);
    div().flex_none().child(button)
}

pub(super) fn compact_setup_row(
    theme: Theme,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    action: impl IntoElement,
) -> Div {
    let title = title.into();
    let hint = hint.into();
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .py(px(4.0))
        .child(
            div()
                .flex_1()
                .min_w(px(150.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(hint),
                ),
        )
        .child(div().flex_none().child(action))
}
