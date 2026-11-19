//! AppKit-level input for the debug control channel.
//!
//! GPUI's `dispatch_event` never reaches native subviews such as WKWebView.
//! These events are built in-process and handed to `NSWindow::sendEvent`, so
//! AppKit hit-tests them like real input. No system permission is involved
//! because the process only talks to its own window.
use super::protocol::{MousePhase, Response};
use gpui::Window;
use objc2::rc::Retained;
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWindow};
use objc2_foundation::{NSPoint, NSProcessInfo, NSString};

pub(super) fn native_window(window: &Window) -> Result<Retained<NSWindow>, &'static str> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).map_err(|_| "no native window")?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("not an AppKit window");
    };
    // SAFETY: GPUI owns this live view and we are on the AppKit main thread.
    let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
    view.window().ok_or("view has no window")
}

/// Window-local top-left coordinates to AppKit's bottom-left content coordinates.
fn location(window: &NSWindow, x: f32, y: f32) -> NSPoint {
    let height = window.contentView().map_or_else(
        || window.contentLayoutRect().size.height,
        |view| view.frame().size.height,
    );
    NSPoint::new(f64::from(x), height - f64::from(y))
}

fn mouse_type(phase: MousePhase, pressed: bool) -> NSEventType {
    match phase {
        MousePhase::Move if pressed => NSEventType::LeftMouseDragged,
        MousePhase::Move => NSEventType::MouseMoved,
        MousePhase::Down => NSEventType::LeftMouseDown,
        MousePhase::Up => NSEventType::LeftMouseUp,
    }
}

/// Must run outside `App::update`: GPUI's native event callback re-enters the
/// app, and while the window is leased it silently drops the event.
pub(super) fn mouse(
    native: &NSWindow,
    phase: MousePhase,
    x: f32,
    y: f32,
    click_count: usize,
    pressed: bool,
) -> Response {
    let pressure = if matches!(phase, MousePhase::Down) || pressed {
        1.0
    } else {
        0.0
    };
    let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        mouse_type(phase, pressed),
        location(native, x, y),
        NSEventModifierFlags::empty(),
        NSProcessInfo::processInfo().systemUptime(),
        native.windowNumber(),
        None,
        0,
        click_count as isize,
        pressure,
    );
    let Some(event) = event else {
        return Response::error("NSEvent mouse creation failed");
    };
    if matches!(phase, MousePhase::Down) {
        focus_native_hit_view(native, x, y);
    }
    native.sendEvent(&event);
    Response::ok()
}

/// 真实点击会让命中的 WKWebView 成为第一响应者；隔离窗口常常不是 key window，
/// 此时 `sendEvent` 不会移交响应者，网页就收不到后续的选择和按键。这里显式补上。
fn focus_native_hit_view(native: &NSWindow, x: f32, y: f32) {
    let Some(hit) = native
        .contentView()
        .and_then(|view| view.hitTest(location(native, x, y)))
    else {
        return;
    };
    let is_gpui = hit.class().name().to_string_lossy().contains("GPUIView");
    if !is_gpui && hit.acceptsFirstResponder() {
        native.makeFirstResponder(Some(&hit));
    }
}

pub(super) fn text(native: &NSWindow, text: &str) -> Response {
    for character in text.chars() {
        let chars = NSString::from_str(&character.to_string());
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind,
                NSPoint::new(0.0, 0.0),
                NSEventModifierFlags::empty(),
                NSProcessInfo::processInfo().systemUptime(),
                native.windowNumber(),
                None,
                &chars,
                &chars,
                false,
                0,
            );
            let Some(event) = event else {
                return Response::error("NSEvent key creation failed");
            };
            native.sendEvent(&event);
        }
    }
    Response::ok()
}

fn modifier_flags(names: &[String]) -> NSEventModifierFlags {
    names
        .iter()
        .fold(NSEventModifierFlags::empty(), |flags, name| {
            flags
                | match name.as_str() {
                    "cmd" => NSEventModifierFlags::Command,
                    "shift" => NSEventModifierFlags::Shift,
                    "alt" => NSEventModifierFlags::Option,
                    "ctrl" => NSEventModifierFlags::Control,
                    _ => NSEventModifierFlags::empty(),
                }
        })
}

/// Key equivalents (Cmd/Ctrl combos) go through `performKeyEquivalent:` first,
/// like `NSApplication::sendEvent` does for real key presses.
pub(super) fn key(native: &NSWindow, key_code: u16, chars: &str, modifiers: &[String]) -> Response {
    let flags = modifier_flags(modifiers);
    let chars = NSString::from_str(chars);
    for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            kind,
            NSPoint::new(0.0, 0.0),
            flags,
            NSProcessInfo::processInfo().systemUptime(),
            native.windowNumber(),
            None,
            &chars,
            &chars,
            false,
            key_code,
        );
        let Some(event) = event else {
            return Response::error("NSEvent key creation failed");
        };
        let equivalent = matches!(kind, NSEventType::KeyDown)
            && flags.intersects(NSEventModifierFlags::Command | NSEventModifierFlags::Control);
        // NSApplication.sendEvent 先让窗口、再让主菜单处理 Cmd/Ctrl 组合键；这里照做，
        // 否则菜单里的快捷键（如复制）在 E2E 里永远到不了。
        if equivalent && (native.performKeyEquivalent(&event) || main_menu_handles(&event)) {
            continue;
        }
        native.sendEvent(&event);
    }
    Response::ok()
}

/// 与真实按键一致：窗口不处理的 Cmd/Ctrl 组合键交给 App 主菜单。
fn main_menu_handles(event: &NSEvent) -> bool {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    // SAFETY: 控制通道在 AppKit 主线程执行；NSApplication 是进程级单例，
    // mainMenu 为空时向 nil 发消息返回 NO。
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let menu: *mut AnyObject = msg_send![app, mainMenu];
        let handled: Bool = msg_send![menu, performKeyEquivalent: event];
        handled.as_bool()
    }
}
