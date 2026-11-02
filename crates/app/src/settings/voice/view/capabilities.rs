use super::VoiceSettingsInputs;
use crate::{
    app::MultisoulApp,
    settings::typography::{SettingsText, TextRole},
};
use gpui::{div, prelude::*, px, rgba, App, AppContext, Context, Div, Entity, Focusable, Window};
use ui::{component::input::InputState, Theme};
mod a1;
mod jev;
pub(in crate::settings::voice::view) use a1::messages::preparation_failure as a1_preparation_failure;
pub(in crate::settings::voice::view) use a1::A1Panel;
pub(in crate::settings::voice::view) use jev::JevPanel;

pub(super) fn field(
    theme: Theme,
    title: &'static str,
    input: &Entity<InputState>,
    id: &'static str,
    disabled: bool,
    cx: &App,
) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .settings_text(TextRole::RowTitle, theme)
                .child(ui::i18n::text(title)),
        )
        .child(
            crate::settings::norma::settings_editor(id, input, disabled, None, cx)
                .debug_selector(move || id.into()),
        )
}

pub(super) fn helper(theme: Theme, key: &'static str) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .settings_text(TextRole::Helper, theme)
        .child(ui::i18n::text(key))
}

pub(super) fn status(theme: Theme, text: String, is_error: bool, selector: &'static str) -> Div {
    div()
        .debug_selector(move || selector.into())
        .w_full()
        .min_w(px(0.0))
        .settings_font(TextRole::Helper)
        .text_color(rgba(if is_error {
            theme.destructive
        } else {
            theme.text_secondary
        }))
        .child(text)
}

impl VoiceSettingsInputs {
    pub(crate) fn prepare_capabilities(
        &mut self,
        state: &crate::settings::voice::VoiceSettingsState,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<MultisoulApp>,
    ) {
        if self.a1_panel.is_none() {
            let root = state.model_root().to_owned();
            let snapshot = state.a1_setup_snapshot();
            let host = cx.entity().downgrade();
            self.a1_panel =
                Some(cx.new(|cx| A1Panel::new(root, snapshot.configured, host, theme, cx)));
        }
        if self.jev_panel.is_none() {
            let store = state.credential_store();
            self.jev_panel = Some(cx.new(|cx| JevPanel::new(store, theme, window, cx)));
        }
        if let Some(view) = &self.a1_panel {
            view.update(cx, |view, cx| {
                view.theme = theme;
                if state.a1_requirement() == crate::settings::voice::VoiceRequirementState::Ready {
                    view.files_verified(cx);
                }
            });
        }
        if let Some(view) = &self.jev_panel {
            view.update(cx, |view, _| view.theme = theme);
        }
    }

    pub(in crate::settings::voice::view) fn capabilities_focused(
        &self,
        window: &Window,
        cx: &App,
    ) -> bool {
        self.jev_panel
            .as_ref()
            .is_some_and(|view| view.read(cx).input.focus_handle(cx).is_focused(window))
    }
}
