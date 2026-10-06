use super::super::components::VoiceAction;
use crate::app::MultisoulApp;
use crate::settings::spacing::{SECTION_GAP, SETUP_ROW_PADDING_Y};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use gpui::{
    div, prelude::*, px, rgba, App, ClickEvent, Context, Div, IntoElement, SharedString, Window,
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
        .child(control_label(label))
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
        .gap(px(SECTION_GAP))
        .py(px(SETUP_ROW_PADDING_Y))
        .child(
            div()
                .flex_1()
                .min_w(px(150.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(div().settings_text(TextRole::RowTitle, theme).child(title))
                .child(div().settings_text(TextRole::Helper, theme).child(hint)),
        )
        .child(div().flex_none().child(action))
}
