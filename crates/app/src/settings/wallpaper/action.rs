use gpui::{App, ClickEvent, InteractiveElement, KeyDownEvent, Window};
use std::rc::Rc;
use ui::component::button::Button;

/// gpui-component 0.5.1 supplies focus styling but does not activate buttons with keys.
pub(super) trait WallpaperAction {
    fn on_wallpaper_activate(
        self,
        enabled: bool,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self;
}

impl WallpaperAction for Button {
    fn on_wallpaper_activate(
        self,
        enabled: bool,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        let handler = Rc::new(handler);
        let click = handler.clone();
        self.on_click(move |event, window, cx| {
            if enabled {
                click(event, window, cx);
            }
        })
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if enabled && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                handler(&ClickEvent::default(), window, cx);
                cx.stop_propagation();
            }
        })
    }
}
