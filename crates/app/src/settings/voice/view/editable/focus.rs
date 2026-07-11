use super::{status, VoiceTextInput};
use gpui::{Context, Subscription, Window};

impl VoiceTextInput {
    pub(super) fn ensure_focus_subscriptions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.focus_subscription.is_some() {
            return;
        }
        let focus_handle = self.focus_handle.clone();
        let on_focus = cx.on_focus(&focus_handle, window, Self::focus_entered);
        let on_blur = cx.on_blur(&focus_handle, window, Self::focus_blurred);
        self.focus_subscription = Some(Subscription::join(on_focus, on_blur));
    }

    fn focus_entered(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.original_content = if self.masked {
            "".into()
        } else {
            self.content.clone()
        };
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.update_parent_draft(true, cx);
        cx.notify();
    }

    fn focus_blurred(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_focus_session(cx);
    }

    pub(super) fn finish_focus_session(&mut self, cx: &mut Context<Self>) {
        self.auto_save_task = None;
        let field = self.field;
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let (value, placeholder) = app.update(cx, |app, cx| {
            app.cancel_voice_text_input(field, cx);
            app.flush_voice_text_input_autosave(cx);
            (
                app.voice_settings.editor_value(field),
                status::input_placeholder(&app.voice_settings, field),
            )
        });
        self.sync_value_with_placeholder(value, placeholder, cx);
    }
}
