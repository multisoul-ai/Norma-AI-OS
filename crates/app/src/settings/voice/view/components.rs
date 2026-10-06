use super::{setting_label, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::spacing::{FORM_ROW_PADDING_Y, SECTION_GAP};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use crate::settings::voice::VoiceSettingsField;
use gpui::{
    div, prelude::*, px, rgba, App, ClickEvent, Context, Div, KeyDownEvent, SharedString, Stateful,
    Window,
};
use std::rc::Rc;
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;

/// The pinned component button provides focus styling but no keyboard activation.
/// Keep mouse and keyboard actions on one callback for the enabled Voice controls.
pub(super) trait VoiceAction {
    fn on_activate(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self;
}

impl VoiceAction for Button {
    fn on_activate(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        let handler = Rc::new(handler);
        let on_click = handler.clone();
        self.on_click(move |event, window, cx| on_click(event, window, cx))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    handler(&ClickEvent::default(), window, cx);
                    cx.stop_propagation();
                }
            })
    }
}

/// Settings actions keep a GPUI `Div` because callers attach stable debug selectors after
/// construction; gpui-component 0.5.1's `Button` does not expose that test seam or this compact
/// accent variant. Keep this exception local instead of introducing another general button type.
pub(crate) fn action_button(_theme: Theme, label: impl Into<SharedString>, accent: u32) -> Div {
    div()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(accent, 0x70)))
        .bg(rgba(with_alpha(accent, 0x18)))
        .settings_font(TextRole::Control)
        .text_color(rgba(accent))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(accent, 0x28))))
        .child(label.into())
}

pub(in crate::settings::voice) fn text_setting_row(
    theme: Theme,
    inputs: &VoiceSettingsInputs,
    field: VoiceSettingsField,
    title: &'static str,
    hint: &'static str,
    _cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(SECTION_GAP))
        .py(px(FORM_ROW_PADDING_Y))
        .child(
            div()
                .flex_1()
                .min_w(px(180.0))
                .child(setting_label(theme, title, hint)),
        )
        .child(div().flex_1().min_w(px(200.0)).child(inputs.input(field)))
}

pub(super) fn component_action(
    theme: Theme,
    selector: &'static str,
    label: &'static str,
    cx: &Context<MultisoulApp>,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(with_alpha(theme.accent_blue, 0x10)).into())
        .foreground(rgba(theme.accent_blue).into())
        .border(rgba(with_alpha(theme.accent_blue, 0x40)).into())
        .hover(rgba(with_alpha(theme.accent_blue, 0x28)).into())
        .active(rgba(with_alpha(theme.accent_blue, 0x38)).into());
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .flex_none()
        .child(
            Button::new(selector)
                .child(control_label(label))
                .compact()
                .custom(variant)
                .on_activate(handler),
        )
}
