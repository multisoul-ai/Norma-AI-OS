//! Keep the isolated E2E window out of the user's way.
//!
//! The driver creates a private virtual display and passes its id in
//! `NORMA_E2E_DISPLAY_ID`; windows open there, so nothing appears on the user's
//! screens. Without one, the window is parked with only a sliver on screen so the
//! window server keeps painting it. The App never becomes the frontmost application.
use gpui::{App, DisplayId, Window};

const DISPLAY_ENV: &str = "NORMA_E2E_DISPLAY_ID";
const VISIBLE_ENV: &str = "NORMA_E2E_VISIBLE";

#[cfg(test)]
#[path = "../../tests/unit/e2e_placement_tests.rs"]
mod tests;

fn requested_display() -> Option<u32> {
    display_request(
        super::foreground_requested(),
        env_flag(VISIBLE_ENV),
        std::env::var(DISPLAY_ENV).ok().as_deref(),
    )
}

/// Foreground or visible runs keep the window on the user's screens; otherwise a
/// numeric display id from the driver selects the virtual display.
fn display_request(foreground: bool, visible: bool, display: Option<&str>) -> Option<u32> {
    if foreground || visible {
        return None;
    }
    display?.trim().parse().ok().filter(|id| *id != 0)
}

fn env_flag(name: &str) -> bool {
    flag_enabled(std::env::var(name).ok().as_deref())
}

/// Only the exact value `1` turns a switch on, so `0`, `true` or empty keep the default.
pub(super) fn flag_enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

/// GPUI display for the driver's virtual display, if it is attached.
pub(crate) fn target_display(cx: &App) -> Option<DisplayId> {
    let wanted = requested_display()?;
    cx.displays()
        .into_iter()
        .map(|display| display.id())
        .find(|id| u32::from(*id) == wanted)
}

/// An accessory app never becomes the frontmost application on its own, so launching the
/// isolated window and injecting input cannot take the keyboard or mouse from the user.
#[cfg(not(test))]
pub(super) fn stay_in_background() {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.deactivate();
}

#[cfg(test)]
pub(super) fn stay_in_background() {}

/// Move the window onto the virtual display, or park it with only a few pixels on the
/// current screen. It also ignores the real pointer; injected events are unaffected.
#[cfg(not(test))]
pub(super) fn park_window(window: &Window) {
    use objc2_foundation::NSPoint;
    if env_flag(VISIBLE_ENV) {
        return;
    }
    let Ok(native) = super::ns_input::native_window(window) else {
        return;
    };
    native.setIgnoresMouseEvents(true);
    let height = native.frame().size.height;
    if let Some(screen) = requested_display().and_then(|id| screen_for(id, &native)) {
        let frame = screen.frame();
        let top = frame.origin.y + frame.size.height;
        native.setFrameOrigin(NSPoint::new(frame.origin.x, top - height));
        return;
    }
    let Some(screen) = native.screen() else {
        return;
    };
    const SLIVER: f64 = 4.0;
    let screen = screen.frame();
    native.setFrameOrigin(NSPoint::new(
        screen.origin.x + screen.size.width - SLIVER,
        screen.origin.y + SLIVER - height,
    ));
}

#[cfg(not(test))]
fn screen_for(
    display: u32,
    window: &objc2_app_kit::NSWindow,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSScreen>> {
    use objc2_app_kit::NSScreen;
    use objc2_foundation::{NSNumber, NSString};
    let mtm = objc2::MainThreadMarker::from(window);
    let key = NSString::from_str("NSScreenNumber");
    NSScreen::screens(mtm).into_iter().find(|screen| {
        screen
            .deviceDescription()
            .objectForKey(&key)
            .and_then(|value| value.downcast::<NSNumber>().ok())
            .is_some_and(|number| number.unsignedIntValue() == display)
    })
}

#[cfg(test)]
pub(super) fn park_window(_: &Window) {}
