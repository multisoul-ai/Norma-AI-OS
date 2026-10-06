use super::super::with_alpha;
use crate::settings::typography::{SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, App, RenderOnce, SharedString, Window};
use tts::providers::doubao::DoubaoVoice;
use ui::component::{Icon, IconName, Selectable};
use ui::Theme;

// Keep the saved voice's check separate from the list's temporary keyboard highlight.
// Popover, List and Input still own anchoring, virtualization, search and navigation.
#[derive(IntoElement)]
pub(super) struct VoiceMenuItem {
    theme: Theme,
    voice: &'static DoubaoVoice,
    checked: bool,
    highlighted: bool,
}

impl VoiceMenuItem {
    pub(super) fn new(theme: Theme, voice: &'static DoubaoVoice, checked: bool) -> Self {
        Self {
            theme,
            voice,
            checked,
            highlighted: false,
        }
    }
}

impl Selectable for VoiceMenuItem {
    fn selected(mut self, selected: bool) -> Self {
        self.highlighted = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.highlighted
    }
}

impl RenderOnce for VoiceMenuItem {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let voice = self.voice;
        div()
            .id(SharedString::from(format!(
                "doubao-voice-item-{}",
                voice.id
            )))
            .debug_selector(move || format!("settings-doubao-voice-{}", voice.id))
            .w_full()
            .h(px(48.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgba(with_alpha(
                theme.accent,
                if self.highlighted { 0x70 } else { 0 },
            )))
            .bg(rgba(with_alpha(
                theme.accent,
                if self.highlighted {
                    0x20
                } else if self.checked {
                    0x14
                } else {
                    0
                },
            )))
            .cursor_pointer()
            .hover(move |style| style.bg(rgba(with_alpha(theme.accent, 0x28))))
            .active(move |style| style.bg(rgba(with_alpha(theme.accent, 0x38))))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .settings_text(TextRole::RowTitle, theme)
                            .text_ellipsis()
                            .child(voice.name.clone()),
                    )
                    .child(
                        div()
                            .settings_text(TextRole::Caption, theme)
                            .text_ellipsis()
                            .child(voice.language.clone()),
                    ),
            )
            .child(div().flex_none().w(px(20.0)).when(self.checked, |this| {
                this.child(
                    Icon::new(IconName::Check)
                        .size(px(16.0))
                        .text_color(rgba(theme.accent)),
                )
            }))
    }
}
