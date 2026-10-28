//! Quiet controls shared by the right-drawer reader chrome.

use super::chrome_format::ReaderChromeColors;

use crate::app::MultisoulApp;
use gpui::{rgba, Context};
use ui::component::button::ButtonCustomVariant;

impl MultisoulApp {
    pub(super) fn reader_control_variant(
        &self,
        colors: ReaderChromeColors,
        cx: &mut Context<Self>,
    ) -> ButtonCustomVariant {
        let theme = self.desktop_theme();
        ButtonCustomVariant::new(cx)
            .color(rgba(colors.control_rest).into())
            .foreground(rgba(theme.text_primary).into())
            .border(rgba(colors.control_border).into())
            .hover(rgba(colors.control_hover).into())
            .active(rgba(colors.control_pressed).into())
    }
}
