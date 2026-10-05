use super::super::with_alpha;
use super::menu_state::VoiceMenuState;
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsState;
use gpui::{
    div, prelude::*, px, rgba, App, Context, Corner, Div, FontWeight, KeyDownEvent, RenderOnce,
    Window,
};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    popover::Popover,
    Icon, IconName, Selectable,
};
use ui::Theme;

pub(super) fn voice_picker(
    theme: Theme,
    state: &VoiceSettingsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selected = state.values().doubao_voice();
    let app = cx.weak_entity();
    let menu = window.use_keyed_state("doubao-voice-menu-state", cx, move |window, cx| {
        VoiceMenuState::new(theme, selected, app, window, cx)
    });
    menu.update(cx, |menu, cx| menu.sync(theme, selected, cx));
    let open = menu.read(cx).open;
    let trigger_focus = menu.read(cx).trigger_focus.clone();
    let keyboard_menu = menu.clone();
    let toggle_menu = menu.clone();
    let content_menu = menu.clone();
    let surface = (theme.panel_high & 0xffffff00) | 0xff;
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(with_alpha(theme.accent, if open { 0x20 } else { 0x0d })).into())
        .foreground(rgba(theme.text_primary).into())
        .border(rgba(with_alpha(theme.text_primary, 0x20)).into())
        .hover(rgba(with_alpha(theme.accent, 0x24)).into())
        .active(rgba(with_alpha(theme.accent, 0x38)).into());
    // Leave 20px on each side of a narrow window for the anchored menu.
    let menu_width = px(400.0).min(window.viewport_size().width - px(40.0));
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .py(px(4.0))
        .child(
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(24.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgba(theme.text_primary))
                                .child(ui::i18n::text("doubao.voice_selection")),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(ui::i18n::text("doubao.voice_applies_next_call")),
                        ),
                )
                .child(
                    div()
                        .id("settings-doubao-voice-picker")
                        .debug_selector(|| "settings-doubao-voice-picker".into())
                        .track_focus(&trigger_focus)
                        .w(px(320.0))
                        .max_w_full()
                        .border_1()
                        .border_color(rgba(with_alpha(theme.accent, 0)))
                        .rounded(px(12.0))
                        .focus(move |style| style.border_color(rgba(theme.accent)))
                        .on_key_down(move |event: &KeyDownEvent, window, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space" | "down") {
                                keyboard_menu.update(cx, |menu, cx| menu.show(window, cx));
                                cx.stop_propagation();
                            }
                        })
                        .child(
                            Popover::new("settings-doubao-voice-popover")
                                .anchor(Corner::BottomRight)
                                .open(open)
                                .appearance(false)
                                .w(menu_width)
                                .p(px(8.0))
                                .bg(rgba(surface))
                                .rounded(px(12.0))
                                .shadow_md()
                                .border_1()
                                .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
                                .on_open_change(move |open, window, cx| {
                                    toggle_menu.update(cx, |menu, cx| {
                                        if *open {
                                            menu.show(window, cx);
                                        } else {
                                            menu.open = false;
                                            cx.notify();
                                        }
                                    });
                                })
                                .trigger(VoiceTrigger {
                                    button: Button::new("settings-doubao-voice-trigger")
                                        .custom(variant)
                                        .tab_stop(false)
                                        .w_full()
                                        .h(px(48.0))
                                        .rounded(px(10.0))
                                        .justify_start()
                                        .px(px(16.0))
                                        .child(
                                            div()
                                                .max_w(px(260.0))
                                                .flex()
                                                .items_center()
                                                .gap(px(12.0))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w(px(0.0))
                                                        .flex()
                                                        .flex_col()
                                                        .child(
                                                            div()
                                                                .text_size(px(13.0))
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .text_ellipsis()
                                                                .child(selected.name.clone()),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(11.0))
                                                                .text_color(rgba(
                                                                    theme.text_secondary,
                                                                ))
                                                                .text_ellipsis()
                                                                .child(selected.language.clone()),
                                                        ),
                                                ),
                                        ),
                                    icon: Icon::new(if open {
                                        IconName::ChevronUp
                                    } else {
                                        IconName::ChevronDown
                                    })
                                    .size(px(16.0))
                                    .text_color(rgba(theme.text_secondary)),
                                })
                                .content(move |_, _, cx| content_menu.read(cx).body.clone()),
                        ),
                ),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgba(theme.text_tertiary))
                .child(ui::i18n::text("doubao.voice_catalog_permissions")),
        )
}

// Button's internal label wrapper has intrinsic width. Keep the caret on the
// field's trailing edge while retaining the component's button interaction states.
#[derive(IntoElement)]
struct VoiceTrigger {
    button: Button,
    icon: Icon,
}

impl Selectable for VoiceTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.button = self.button.selected(selected);
        self
    }

    fn is_selected(&self) -> bool {
        self.button.is_selected()
    }
}

impl RenderOnce for VoiceTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().relative().w_full().child(self.button).child(
            div()
                .absolute()
                .right(px(16.0))
                .top(px(16.0))
                .child(self.icon),
        )
    }
}
