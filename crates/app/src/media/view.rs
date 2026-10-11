use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div};
use media_store::MediaAvailability;
use ui::component::button::{Button, ButtonVariants};
use ui::Theme;

/// Render only the process-shared snapshot; no filesystem or network IO here.
pub(crate) fn wallpaper_status(
    rel: &str,
    theme: Theme,
    cx: &gpui::App,
) -> Option<gpui::Stateful<Div>> {
    let state = super::availability(rel, cx)?;
    let (key, message, color, progress) = match state {
        MediaAvailability::Missing => (
            "missing",
            ui::i18n::format(
                "media.wallpaper_needs_download",
                &[(
                    "size",
                    size_label(super::download_bytes(rel, cx).unwrap_or(0)),
                )],
            ),
            theme.accent_blue,
            None,
        ),
        MediaAvailability::Downloading { received, total } => (
            progress_key(received, total),
            progress_message(received, total),
            theme.accent_blue,
            Some(if total == 0 {
                0.0
            } else {
                (received as f32 / total as f32).clamp(0.0, 1.0)
            }),
        ),
        MediaAvailability::Ready(_) => (
            "ready",
            ui::i18n::text("media.downloaded").into(),
            theme.text_secondary,
            None,
        ),
        MediaAvailability::Failed(_) => (
            "failed",
            ui::i18n::text("media.wallpaper_failed").into(),
            theme.destructive,
            None,
        ),
        MediaAvailability::Cancelled => (
            "cancelled",
            ui::i18n::text("media.cancelled").into(),
            theme.text_secondary,
            None,
        ),
    };
    let selector = format!("wallpaper-availability-{key}-{rel}");
    Some(
        div()
            .id(gpui::SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .w_full()
            .min_h(px(22.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .text_size(px(11.0))
            .text_color(rgba(color))
            .child(div().truncate().child(message))
            .when_some(progress, |row, progress| {
                row.child(
                    div()
                        .w_full()
                        .h(px(3.0))
                        .rounded(px(1.5))
                        .bg(rgba(theme.node_stroke))
                        .child(
                            div()
                                .w(gpui::relative(progress))
                                .h_full()
                                .rounded(px(1.5))
                                .bg(rgba(theme.accent_blue)),
                        ),
                )
            }),
    )
}

pub(crate) fn wallpaper_pending(rel: &str, cx: &gpui::App) -> bool {
    super::availability(rel, cx).is_some_and(|state| !matches!(state, MediaAvailability::Ready(_)))
}

fn size_label(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{} KB", bytes.div_ceil(1_000))
    } else {
        format!("{bytes} B")
    }
}

fn progress_key(received: u64, total: u64) -> &'static str {
    if received == 0 {
        "preparing"
    } else if total > 0 && received >= total {
        "verifying"
    } else {
        "downloading"
    }
}

fn progress_message(received: u64, total: u64) -> String {
    match progress_key(received, total) {
        "preparing" => ui::i18n::text("media.preparing").into(),
        "verifying" => ui::i18n::text("media.verifying").into(),
        _ => ui::i18n::format(
            "media.progress",
            &[(
                "percent",
                (received.saturating_mul(100) / total.max(1))
                    .min(99)
                    .to_string(),
            )],
        ),
    }
}

/// Selected media exposes cancellation/retry next to its current status.
pub(crate) fn status(rel: &str, theme: Theme, cx: &mut Context<MultisoulApp>) -> Option<Div> {
    let state = super::availability(rel, cx)?;
    let (message, cancel) = match state {
        MediaAvailability::Ready(_) => return None,
        MediaAvailability::Downloading { received, total } => {
            (progress_message(received, total), true)
        }
        MediaAvailability::Missing => (ui::i18n::text("media.waiting").to_string(), false),
        MediaAvailability::Failed(_) => (ui::i18n::text("media.failed").to_string(), false),
        MediaAvailability::Cancelled => (ui::i18n::text("media.cancelled").to_string(), false),
    };
    let label = if cancel {
        "common.cancel"
    } else if matches!(state, MediaAvailability::Missing) {
        "media.download"
    } else {
        "common.retry"
    };
    let rel = rel.to_string();
    let selector = format!("media-action-{rel}");
    let keyboard_rel = rel.clone();
    let button = Button::new(gpui::SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .ghost()
        .label(ui::i18n::text(label))
        .h(px(30.0))
        .px(px(10.0))
        .tab_index(0)
        .on_click(cx.listener(move |_, _, _, cx| {
            cx.stop_propagation();
            activate(&rel, cancel, cx);
            cx.notify();
        }))
        .on_key_down(cx.listener(move |_, event: &gpui::KeyDownEvent, _, cx| {
            if !event.keystroke.modifiers.modified()
                && matches!(event.keystroke.key.as_str(), "enter" | "space")
            {
                cx.stop_propagation();
                activate(&keyboard_rel, cancel, cx);
                cx.notify();
            }
        }));
    Some(
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .text_size(px(12.0))
            .text_color(rgba(theme.text_secondary))
            .child(message)
            .child(button),
    )
}

fn activate(rel: &str, cancel: bool, cx: &mut gpui::App) {
    if cancel {
        super::cancel(rel, cx);
    } else {
        super::request(rel, true, cx);
    }
}
