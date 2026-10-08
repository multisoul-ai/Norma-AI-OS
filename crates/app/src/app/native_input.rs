//! Keep AppKit's keyboard responder aligned with explicit GPUI input intent.
use gpui::{prelude::*, DispatchPhase, MouseDownEvent, Window};

pub(crate) fn mouse_focus_guard() -> impl IntoElement {
    // No hitbox: Settings occludes the root, but its clicks still need native focus.
    // Only events delivered to GPUI reach this listener; native browser clicks do not.
    gpui::canvas(
        |_, _, _| (),
        |_, _, window, _| {
            window.on_mouse_event(|_: &MouseDownEvent, phase, window, _| {
                if phase == DispatchPhase::Capture {
                    focus_gpui(window, "gpui_mouse_down");
                }
            });
        },
    )
    .absolute()
}

pub(crate) fn focus_gpui(window: &Window, reason: &'static str) {
    // GPUI's unit-test platform has no native handle (and panics if asked for one).
    // The separate main-thread AppKit regression exercises focus_host directly.
    #[cfg(all(target_os = "macos", not(test)))]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: GPUI owns this live view and calls us on the AppKit main thread.
        let view = unsafe { &*handle.ns_view.as_ptr().cast::<objc2_app_kit::NSView>() };
        focus_host(view, reason);
    }
    #[cfg(any(not(target_os = "macos"), test))]
    let _ = (window, reason);
}

#[cfg(target_os = "macos")]
pub(crate) fn focus_host(view: &objc2_app_kit::NSView, reason: &'static str) {
    use objc2::{msg_send, rc::Retained, runtime::AnyObject};
    let Some(window) = view.window() else {
        return;
    };
    let previous = window.firstResponder();
    if previous
        .as_deref()
        .and_then(|responder| responder.downcast_ref())
        == Some(view)
    {
        tracing::debug!(target: "norma::input", reason, window = window.windowNumber(),
            "native input focus already on host");
        return;
    }
    let previous_class = previous.as_ref().map(|responder| responder.class().name());
    let accepted = window.makeFirstResponder(Some(view));
    let current = window.firstResponder();
    let restored = current
        .as_deref()
        .and_then(|responder| responder.downcast_ref())
        == Some(view);
    // SAFETY: NSView implements inputContext; only its presence is logged, never text.
    let context: Option<Retained<AnyObject>> = unsafe { msg_send![view, inputContext] };
    if accepted && restored {
        tracing::info!(target: "norma::input", reason, window = window.windowNumber(),
            ?previous_class, current_class = ?current.as_ref().map(|responder| responder.class().name()),
            input_context = context.is_some(), "native input focus restored");
    } else {
        tracing::warn!(target: "norma::input", reason, window = window.windowNumber(),
            ?previous_class, accepted, restored, input_context = context.is_some(),
            "native input focus recovery failed");
    }
}
