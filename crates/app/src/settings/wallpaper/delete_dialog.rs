//! Destructive library actions live in the window's dialog layer, outside the scrolled page.

use super::{action::WallpaperAction, scoped::WallpaperDeletePrompt};
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
    directory_uses_wallpaper: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let mut scopes = prompt.usage.workspaces;
    scopes.extend(prompt.usage.projects);
    if directory_uses_wallpaper {
        scopes.push(ui::i18n::text("settings.directory_background").into());
    }
    let cancel_host = host.clone();
    window.open_dialog(cx, move |dialog, window, _| {
        let cancel = host.clone();
        let confirm = host.clone();
        dialog
            .title(ui::i18n::format(
                "settings.delete_name",
                &[("name", prompt.filename.clone())],
            ))
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
                    .text_size(px(13.0))
                    .child(ui::i18n::text("settings.removes_shared_file"))
                    .when(!scopes.is_empty(), |body| {
                        body.child(
                            div()
                                .id("wallpaper-delete-affected")
                                .max_h(px(180.0))
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(ui::i18n::text("settings.used_by"))
                                .children(scopes.iter().map(|scope| div().child(scope.clone()))),
                        )
                        .child(
                            div()
                                .text_color(rgba(theme.text_secondary))
                                .child(ui::i18n::text("settings.delete_fallback")),
                        )
                    }),
            )
            .footer(move |_, _, _, _| {
                let cancel = cancel.clone();
                let confirm = confirm.clone();
                vec![
                    Button::new("wallpaper-delete-cancel")
                        .debug_selector(|| "wallpaper-delete-cancel".into())
                        .label(ui::i18n::text("common.cancel"))
                        .on_wallpaper_activate(true, move |_, window, cx| {
                            cancel.update(cx, |app, cx| app.finish_wallpaper_delete(false, cx));
                            window.close_dialog(cx);
                        }),
                    Button::new("wallpaper-delete-confirm")
                        .debug_selector(|| "wallpaper-delete-confirm".into())
                        .danger()
                        .label(ui::i18n::text("settings.delete_from_library"))
                        .on_wallpaper_activate(true, move |_, window, cx| {
                            confirm.update(cx, |app, cx| app.finish_wallpaper_delete(true, cx));
                            window.close_dialog(cx);
                        }),
                ]
            })
    });
}
