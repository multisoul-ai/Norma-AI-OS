//! Return input only while the same native WebView still owns the user's focus.
use super::super::focus_policy::{may_return_focus, FocusSnapshot};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::GetFocus,
        WindowsAndMessaging::{GetAncestor, GetForegroundWindow, IsChild, GA_ROOT},
    },
};
use wry::WebViewExtWindows;

pub(super) fn snapshot() -> FocusSnapshot {
    // SAFETY: these getters have no pointer arguments and run on the owning UI thread.
    unsafe {
        FocusSnapshot {
            foreground: GetForegroundWindow().0 as isize,
            focused: GetFocus().0 as isize,
        }
    }
}

pub(super) fn release_if_current(view: &wry::WebView, expected: FocusSnapshot) {
    // SAFETY: the live COM controller supplies its container HWND. OS ownership and current
    // focus are checked immediately before returning input; no Rust Window is retained.
    unsafe {
        let mut container = HWND::default();
        if view.controller().ParentWindow(&mut container).is_err() || container.0.is_null() {
            return;
        }
        let parent = GetAncestor(container, GA_ROOT);
        if parent.0.is_null() {
            return;
        }
        let current = snapshot();
        let focused = HWND(current.focused as *mut std::ffi::c_void);
        let owned =
            !focused.0.is_null() && (focused == container || IsChild(container, focused).as_bool());
        if may_return_focus(expected, current, parent.0 as isize, owned) {
            let _ = view.focus_parent();
        }
    }
}
