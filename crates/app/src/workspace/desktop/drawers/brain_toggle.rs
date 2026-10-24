//! 标题栏的“语音汇报”开关：开启大脑后出现，只管大脑的汇报是否出声。

use super::super::glass_button_variant;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, AnyElement, Context};
use ui::component::button::{Button, ButtonVariants};
use ui::component::Selectable;

impl MultisoulApp {
    pub(crate) fn render_brain_toggle(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.brain_toggle_visible() {
            return None;
        }
        let theme = self.desktop_theme();
        let speaking = self.brain_speaking();
        Some(
            div()
                .child(
                    Button::new("desktop-brain-voice-report")
                        .debug_selector(|| "desktop-brain-voice-report".to_string())
                        .h(px(32.0))
                        .px(px(11.0))
                        .rounded(px(9.0))
                        .custom(glass_button_variant(theme, cx))
                        .selected(speaking)
                        .tooltip(ui::i18n::text("chrome.brain_voice_report_tip"))
                        .child(ui::i18n::text("chrome.brain_voice_report"))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            app.set_brain_speaking(!speaking, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}
