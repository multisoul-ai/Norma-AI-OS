use crate::app::MultisoulApp;
use gpui::{ClickEvent, Context, Window};

impl MultisoulApp {
    pub(crate) fn on_feedback_click(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::info!("feedback button clicked");
        cx.notify();
    }
}
