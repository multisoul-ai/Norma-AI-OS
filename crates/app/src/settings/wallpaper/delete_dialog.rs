//! Destructive library actions live in the window's dialog layer, outside the scrolled page.
use crate::settings::typography::{control_label, SettingsText, TextRole};

use super::{action::WallpaperAction, preferences::WallpaperDeletePrompt};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, App, Entity, Window};
use ui::{
    component::{
        button::{Button, ButtonVariants},
        WindowExt,
    },
    Theme,
};

pub(crate) fn open(
    host: Entity<MultisoulApp>,
    prompt: WallpaperDeletePrompt,
    theme: Theme,
    uses_wallpaper: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let cancel_host = host.clone();
    window.open_dialog(cx, move |dialog, window, _| {
        let cancel = host.clone();
        let confirm = host.clone();
        dialog
            .title(
                div()
                    .settings_font(TextRole::Headline)
                    .child(ui::i18n::format(
                        "settings.delete_name",
                        &[("name", prompt.filename.clone())],
                    )),
            )
            .width(px(480.0).min(window.viewport_size().width - px(48.0)))
            .margin_top((window.viewport_size().height / 2.0 - px(150.0)).max(px(24.0)))
            .max_h(window.viewport_size().height - px(48.0))
            .rounded(px(16.0))
            .bg(rgba(ui::icons::with_alpha(theme.panel_high, 0xff)))
            .border_color(rgba(theme.node_stroke))
            .text_color(rgba(theme.text_primary))
            .close_button(false)
            .overlay_closable(false)
            // Enter on the dialog itself must not confirm a destructive action.
            .on_ok(|_, _, _| false)
            .on_cancel({
                let host = cancel_host.clone();
                move |_, _, cx| {
                    host.update(cx, |app, cx| app.finish_wallpaper_delete(false, cx));
                    true
                }
            })
            .child(
                div()
                    .debug_selector(|| "wallpaper-delete-dialog".into())
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .settings_font(TextRole::Body)
                    .child(ui::i18n::text("settings.removes_shared_file"))
                    .when(uses_wallpaper, |body| {
                        body.child(
                            div()
                                .text_color(rgba(theme.text_secondary))
                                .child(ui::i18n::text("settings.global_wallpaper_delete_fallback")),
                        )
                    }),
            )
            .footer(move |_, _, _, _| {
                let cancel = cancel.clone();
                let confirm = confirm.clone();
                vec![
                    Button::new("wallpaper-delete-cancel")
                        .debug_selector(|| "wallpaper-delete-cancel".into())
                        .child(control_label(ui::i18n::text("common.cancel")))
                        .on_wallpaper_activate(true, move |_, window, cx| {
                            cancel.update(cx, |app, cx| app.finish_wallpaper_delete(false, cx));
                            window.close_dialog(cx);
                        }),
                    Button::new("wallpaper-delete-confirm")
                        .debug_selector(|| "wallpaper-delete-confirm".into())
                        .danger()
                        .child(control_label(ui::i18n::text(
                            "settings.delete_from_library",
                        )))
                        .on_wallpaper_activate(true, move |_, window, cx| {
                            confirm.update(cx, |app, cx| app.finish_wallpaper_delete(true, cx));
                            window.close_dialog(cx);
                        }),
                ]
            })
    });
}
