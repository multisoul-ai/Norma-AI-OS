use crate::island::preferences::{self, Setting};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::{
    component::{button::Button, switch::Switch, Disableable, Sizable, Size},
    i18n::text,
    Theme,
};

pub(super) fn section(theme: Theme, cx: &mut Context<crate::app::MultisoulApp>) -> Div {
    let state = preferences::state(cx);
    let ready = state.is_some();
    let preferences = preferences::current(cx);
    let error = state
        .as_ref()
        .and_then(|state| state.read(cx).error.clone());
    let notice = state.and_then(|state| state.read(cx).notice.clone());
    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .settings_text(TextRole::Headline, theme)
                .child(text("island.title")),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(text("island.settings_hint")),
        );
    for (id, setting, label, checked) in [
        (
            "island-setting-hidden",
            Setting::Hidden,
            "island.hide",
            preferences.hidden,
        ),
        (
            "island-setting-approvals",
            Setting::Approvals,
            "island.popup_approvals",
            preferences.approvals,
        ),
        (
            "island-setting-questions",
            Setting::Questions,
            "island.popup_questions",
            preferences.questions,
        ),
        (
            "island-setting-completions",
            Setting::Completions,
            "island.popup_completions",
            preferences.completions,
        ),
    ] {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .py(px(8.))
                .child(
                    div()
                        .settings_text(TextRole::Control, theme)
                        .child(text(label)),
                )
                .child(
                    Switch::new(id)
                        .checked(checked)
                        .disabled(!ready)
                        .with_size(Size::Medium)
                        .on_click(move |checked, _, cx| preferences::select(setting, *checked, cx)),
                ),
        );
    }
    section = section.child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .py(px(8.))
            .child(
                div()
                    .settings_text(TextRole::Control, theme)
                    .child(text("island.clear_remembered")),
            )
            .child(
                Button::new("island-clear-remembered")
                    .child(control_label(
                        text("island.clear_remembered_action").to_string(),
                    ))
                    .outline()
                    .disabled(!ready)
                    .on_click(|_, _, cx| preferences::clear_remembered(cx)),
            ),
    );
    section
        .when_some(notice, |view, notice| {
            view.child(
                div()
                    .settings_font(TextRole::Helper)
                    .text_color(rgba(theme.text_secondary))
                    .child(notice.to_string()),
            )
        })
        .when_some(error, |view, error| {
            view.child(
                div()
                    .settings_font(TextRole::Helper)
                    .text_color(rgba(theme.destructive))
                    .child(error.to_string()),
            )
        })
}
