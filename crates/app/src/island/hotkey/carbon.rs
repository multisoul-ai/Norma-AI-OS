//! Narrow Carbon `RegisterEventHotKey` boundary (no Accessibility permission needed).
use super::chord::Chord;
use std::ffi::c_void;
use std::sync::OnceLock;

type Ref = *mut c_void;
type Handler = unsafe extern "C" fn(Ref, Ref, Ref) -> i32;

#[repr(C)]
struct EventTypeSpec {
    class: u32,
    kind: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct HotKeyId {
    signature: u32,
    id: u32,
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn GetApplicationEventTarget() -> Ref;
    fn InstallEventHandler(
        target: Ref,
        handler: Handler,
        count: u32,
        types: *const EventTypeSpec,
        user_data: Ref,
        handler_ref: *mut Ref,
    ) -> i32;
    fn RegisterEventHotKey(
        code: u32,
        modifiers: u32,
        id: HotKeyId,
        target: Ref,
        options: u32,
        hotkey_ref: *mut Ref,
    ) -> i32;
    fn UnregisterEventHotKey(hotkey_ref: Ref) -> i32;
}

static PRESSED: OnceLock<async_channel::Sender<()>> = OnceLock::new();
static HANDLER: OnceLock<i32> = OnceLock::new();

unsafe extern "C" fn pressed(_call: Ref, _event: Ref, _data: Ref) -> i32 {
    // The handler runs on the main run loop; only enqueue, GPUI handles it on its own turn.
    if let Some(sender) = PRESSED.get() {
        let _ = sender.try_send(());
    }
    0
}

/// Installs the one process-wide handler; presses arrive on the returned receiver.
pub(super) fn install() -> Result<async_channel::Receiver<()>, i32> {
    let (sender, receiver) = async_channel::unbounded();
    if PRESSED.set(sender).is_err() {
        return Err(-1);
    }
    let status = *HANDLER.get_or_init(|| {
        let event = EventTypeSpec {
            class: u32::from_be_bytes(*b"keyb"),
            kind: 5, // kEventHotKeyPressed
        };
        let mut handler = std::ptr::null_mut();
        // SAFETY: the handler is a static fn with no borrowed data, and the handler
        // reference lives for the whole process (never removed).
        unsafe {
            InstallEventHandler(
                GetApplicationEventTarget(),
                pressed,
                1,
                &event,
                std::ptr::null_mut(),
                &mut handler,
            )
        }
    });
    if status == 0 {
        Ok(receiver)
    } else {
        Err(status)
    }
}

/// One live system registration; dropping it unregisters the chord.
pub(super) struct Registration(Ref);

impl Registration {
    pub fn new(chord: Chord) -> Result<Self, i32> {
        let mut key = std::ptr::null_mut();
        // SAFETY: `key` is a valid out-pointer; Carbon owns the returned reference until
        // `UnregisterEventHotKey` runs in `Drop`.
        let status = unsafe {
            RegisterEventHotKey(
                chord.code,
                chord.modifiers,
                HotKeyId {
                    signature: u32::from_be_bytes(*b"NIsl"),
                    id: 1,
                },
                GetApplicationEventTarget(),
                0,
                &mut key,
            )
        };
        if status == 0 && !key.is_null() {
            Ok(Self(key))
        } else {
            Err(status)
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: the reference came from a successful `RegisterEventHotKey` and is released once.
        unsafe { UnregisterEventHotKey(self.0) };
    }
}
