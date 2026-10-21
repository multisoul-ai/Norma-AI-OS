//! Which display a new Norma window opens on, and its window options.
use gpui::{App, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions};

/// Display new windows open on. Debug desktop E2E runs put them on a private virtual display.
pub(crate) fn initial_display(cx: &App) -> Option<gpui::DisplayId> {
    #[cfg(all(
        target_os = "macos",
        debug_assertions,
        not(feature = "production-data")
    ))]
    return crate::e2e_control::target_display(cx);
    #[cfg(not(all(
        target_os = "macos",
        debug_assertions,
        not(feature = "production-data")
    )))]
    {
        let _ = cx;
        None
    }
}

pub(crate) fn window_options(
    title: String,
    bounds: Bounds<gpui::Pixels>,
    display_id: Option<gpui::DisplayId>,
) -> WindowOptions {
    WindowOptions {
        display_id,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        window_background: WindowBackgroundAppearance::Blurred,
        ..Default::default()
    }
}
