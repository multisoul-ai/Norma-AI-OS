//! Reattach an existing desktop entity without reopening SQLite or starting a second Runtime.
use crate::app::MultisoulApp;
use gpui::{px, size, AnyWindowHandle, App, AppContext, Bounds, Entity};
use ui::component::Root;

pub(crate) fn reopen_desktop_host(
    app: &Entity<MultisoulApp>,
    cx: &mut App,
) -> anyhow::Result<AnyWindowHandle> {
    if let Some(handle) =
        crate::workspace::desktop::sessions::window_for_source(app.entity_id().as_u64(), cx)
    {
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return Ok(handle);
        }
    }
    let display = super::initial_display(cx);
    let bounds = Bounds::centered(display, size(px(1280.), px(800.)), cx);
    let handle = cx.open_window(
        super::window_options("Norma OS".into(), bounds, display),
        |window, cx| {
            app.update(cx, |app, cx| app.attach_desktop_host_window(window, cx));
            MultisoulApp::guard_preview_window_close(app, window, cx);
            app.read(cx).focus_handle.focus(window);
            window.activate_window();
            cx.new(|cx| Root::new(app.clone(), window, cx))
        },
    )?;
    let database = app.read(cx).database_paths.app_db_path().to_owned();
    crate::workspace::desktop::sessions::register(database, handle.into(), app.clone(), cx);
    Ok(handle.into())
}
