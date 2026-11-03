//! Logical removal invalidates callbacks before the native HWND is destroyed.
use super::{Callbacks, WindowsWindowInner};
use ::util::ResultExt;
use windows::Win32::{System::Ole::RevokeDragDrop, UI::WindowsAndMessaging::DestroyWindow};

impl WindowsWindowInner {
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.get()
    }

    pub(crate) fn detach_callbacks(&self) -> Callbacks {
        self.closed.set(true);
        let (callbacks, restore, input) = {
            let mut state = self.state.borrow_mut();
            (
                std::mem::take(&mut state.callbacks),
                state.restore_from_minimized.take(),
                state.input_handler.take(),
            )
        };
        // Dropping user-owned handlers must not retain the state borrow.
        drop((restore, input));
        callbacks
    }

    pub(crate) fn restore_callback<T>(
        &self,
        select: impl FnOnce(&mut Callbacks) -> &mut Option<T>,
        callback: T,
    ) {
        let retired = if self.is_closed() {
            Some(callback)
        } else {
            let mut state = self.state.borrow_mut();
            select(&mut state.callbacks).replace(callback)
        };
        drop(retired);
    }

    fn revoke_drag_drop(&self) {
        if self.drag_drop_registered.replace(false) {
            // SAFETY: ownership is consumed before COM can reenter this STA.
            unsafe { RevokeDragDrop(self.hwnd) }.log_err();
        }
    }

    pub(crate) fn begin_native_destroy(&self) -> Option<Callbacks> {
        if self.native_destroyed.replace(true) {
            return None;
        }
        let callbacks = self.detach_callbacks();
        self.revoke_drag_drop();
        Some(callbacks)
    }

    pub(crate) fn destroy_native_if_owned(&self) {
        if self.native_destroyed.get() {
            return;
        }
        self.revoke_drag_drop();
        if !self.native_destroyed.get() {
            // SAFETY: only the original owner can destroy this still-live HWND.
            unsafe { DestroyWindow(self.hwnd) }.log_err();
        }
    }
}
