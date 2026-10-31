//! Shared inline headers and secondary originals. Actions live beside the header, never inside it.
use super::overview_details::{compact_title, DetailKey};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Stateful};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Sizable,
};

pub(super) fn header(
    id: String,
    key: DetailKey,
    title: String,
    meta: String,
    glyph: &'static str,
    color: u32,
    app: &MultisoulApp,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let theme = app.desktop_theme();
    let feedback = super::super::overview_rows::overview_feedback(theme);
    let expanded = app.desktop_workspace.attention.is_open(&key);
    let selector = id.clone();
    div()
        .id(ElementId::Name(id.into()))
        .debug_selector(move || selector)
        .flex_1()
        .min_w(px(0.))
        .min_h(px(50.))
        .px(px(5.))
        .py(px(5.))
        .flex()
        .items_center()
        .gap(px(7.))
        .rounded(px(6.))
        .border_1()
        .border_color(rgba(0))
        .tab_index(0)
        .cursor_pointer()
        .hover(move |s| s.bg(rgba(feedback.hover)))
        .active(move |s| s.bg(rgba(feedback.pressed)))
        .focus(move |s| s.border_color(rgba(feedback.focus)))
        .on_click(cx.listener(move |app, _, _, cx| {
            cx.stop_propagation();
            toggle(app, key.clone(), cx);
        }))
        .on_key_down(|event, _, cx| {
            if !event.keystroke.modifiers.modified()
                && matches!(event.keystroke.key.as_str(), "enter" | "space")
            {
                // GPUI synthesizes on_click on key-up; only isolate key-down from overview navigation.
                cx.stop_propagation();
            }
        })
        .child(
            div()
                .flex_none()
                .text_size(px(13.))
                .text_color(rgba(color))
                .child(glyph),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgba(theme.text_primary))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgba(theme.text_secondary))
                        .child(meta),
                ),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(10.))
                .text_color(rgba(theme.text_tertiary))
                .child(if expanded { "▾" } else { "▸" }),
        )
}

fn toggle(app: &mut MultisoulApp, key: DetailKey, cx: &mut Context<MultisoulApp>) {
    app.desktop_workspace.attention.toggle(key.clone());
    if app.desktop_workspace.attention.is_open(&key) {
        if let DetailKey::Approval {
            area_id,
            node_id,
            run_id,
            request_id,
        } = &key
        {
            app.desktop_workspace.attention.approval_details =
                app.overview_approval_details(area_id, *node_id, run_id, request_id, cx);
        }
    }
    cx.notify();
}

pub(super) fn title(text: &str, fallback: &'static str) -> String {
    compact_title(text, ui::i18n::text(fallback)).to_owned()
}

pub(super) fn isolate_navigation(
    event: &gpui::KeyDownEvent,
    _: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    if !event.keystroke.modifiers.modified()
        && matches!(
            event.keystroke.key.as_str(),
            "enter" | "space" | "up" | "down" | "home" | "end"
        )
    {
        cx.stop_propagation();
    }
}

pub(super) fn body(theme: ui::Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .pl(px(27.))
        .pr(px(9.))
        .pb(px(10.))
        .text_size(px(12.))
        .line_height(px(20.))
        .text_color(rgba(theme.text_secondary))
}

pub(super) fn original(
    id: String,
    label: &'static str,
    text: String,
    app: &MultisoulApp,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let theme = app.desktop_theme();
    let open = app.desktop_workspace.attention.original_open;
    let glyph = if open { "▾" } else { "▸" };
    let debug = format!("{id}-text");
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            Button::new(ElementId::Name(id.into()))
                .small()
                .h(px(24.))
                .px(px(3.))
                .label(format!("{glyph} {}", ui::i18n::text(label)))
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(rgba(0).into())
                        .foreground(rgba(theme.text_secondary).into())
                        .border(rgba(0).into())
                        .hover(rgba(ui::icons::with_alpha(theme.text_primary, 0x15)).into())
                        .active(rgba(ui::icons::with_alpha(theme.text_primary, 0x25)).into()),
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.desktop_workspace.attention.toggle_original();
                    cx.notify();
                })),
        )
        .when(open, |view| {
            view.child(
                div()
                    .id(ElementId::Name(debug.clone().into()))
                    .debug_selector(move || debug)
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .p(px(8.))
                    .rounded(px(5.))
                    .bg(rgba(ui::icons::with_alpha(theme.text_primary, 0x09)))
                    .child(text),
            )
        })
}
