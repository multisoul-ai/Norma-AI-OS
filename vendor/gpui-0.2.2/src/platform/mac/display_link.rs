//! CoreVideo has no callback-join API: stopped native links remain owned by a
//! process-wide registry, one per encountered display ID. Window subscriptions
//! and dispatch sources are reclaimed. A late callback carries only a display
//! ID and cannot dereference a destroyed window/source.
//! Based on Zed's shared CVDisplayLink lifecycle; see NORMA_PATCH.md.
use crate::{
    dispatch_get_main_queue,
    dispatch_sys::{
        _dispatch_source_type_data_add, dispatch_release, dispatch_resume, dispatch_set_context,
        dispatch_source_cancel, dispatch_source_create, dispatch_source_merge_data,
        dispatch_source_set_event_handler_f, dispatch_source_t, dispatch_suspend,
    },
};
use anyhow::Result;
use core_graphics::display::CGDirectDisplayID;
use foreign_types::{ForeignType, ForeignTypeRef};
use std::{
    collections::BTreeMap,
    ffi::c_void,
    sync::{Mutex, MutexGuard, PoisonError},
};
use util::ResultExt;

static DISPLAYS: Mutex<BTreeMap<CGDirectDisplayID, DisplayEntry>> = Mutex::new(BTreeMap::new());

struct DisplayEntry {
    // Never removed: CVDisplayLinkStop does not join CoreVideo's IO thread.
    link: sys::DisplayLink,
    sources: Vec<dispatch_source_t>,
}

// SAFETY: native handles are thread-safe; the mutex serializes access to
// sources. Only the main thread mutates subscriptions or starts/stops links.
unsafe impl Send for DisplayEntry {}

fn displays() -> MutexGuard<'static, BTreeMap<CGDirectDisplayID, DisplayEntry>> {
    DISPLAYS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn assert_main_thread() {
    assert!(
        unsafe { libc::pthread_main_np() } != 0,
        "DisplayLink lifecycle must run on the macOS main thread"
    );
}

unsafe extern "C" fn display_link_callback(
    _link: *mut sys::CVDisplayLink,
    _current_time: *const sys::CVTimeStamp,
    _output_time: *const sys::CVTimeStamp,
    _flags_in: i64,
    _flags_out: *mut i64,
    display_id: *mut c_void,
) -> i32 {
    let registry = displays();
    if let Some(entry) = registry.get(&(display_id as usize as CGDirectDisplayID)) {
        // Keep the lock through merge_data: stop/drop removes the source under
        // this same lock before cancelling/releasing it.
        for &source in &entry.sources {
            unsafe { dispatch_source_merge_data(source, 1) };
        }
    }
    0
}

pub struct DisplayLink {
    display_id: CGDirectDisplayID,
    frame_requests: dispatch_source_t,
    running: bool,
}

impl DisplayLink {
    pub fn new(
        display_id: CGDirectDisplayID,
        data: *mut c_void,
        callback: unsafe extern "C" fn(*mut c_void),
    ) -> Result<DisplayLink> {
        assert_main_thread();
        let exists = displays().contains_key(&display_id);
        if !exists {
            // Never call CoreVideo with the registry locked: its IO thread may
            // hold internal CoreVideo locks while entering our callback.
            let link = unsafe {
                sys::DisplayLink::new(
                    display_id,
                    display_link_callback,
                    display_id as usize as *mut c_void,
                )?
            };
            displays().insert(
                display_id,
                DisplayEntry {
                    link,
                    sources: Vec::new(),
                },
            );
        }
        // Create the window resource only after fallible native initialization.
        let frame_requests = unsafe {
            let source = dispatch_source_create(
                &_dispatch_source_type_data_add,
                0,
                0,
                dispatch_get_main_queue(),
            );
            anyhow::ensure!(!source.is_null(), "could not create frame dispatch source");
            dispatch_set_context(crate::dispatch_sys::dispatch_object_t { _ds: source }, data);
            dispatch_source_set_event_handler_f(source, Some(callback));
            source
        };
        Ok(Self {
            display_id,
            frame_requests,
            running: false,
        })
    }

    pub fn display_id(&self) -> CGDirectDisplayID {
        self.display_id
    }

    /// Pause for synchronous painting; only a previously running link may resume.
    pub fn pause(&mut self) -> Result<bool> {
        let was_running = self.running;
        self.stop()?;
        Ok(was_running)
    }

    pub fn start(&mut self) -> Result<()> {
        assert_main_thread();
        if self.running {
            return Ok(());
        }
        unsafe {
            dispatch_resume(crate::dispatch_sys::dispatch_object_t {
                _ds: self.frame_requests,
            });
        }
        let link_to_start = {
            let mut registry = displays();
            let entry = registry
                .get_mut(&self.display_id)
                .expect("registered display must exist");
            let first = entry.sources.is_empty();
            entry.sources.push(self.frame_requests);
            first.then(|| entry.link.as_ptr())
        };
        if let Some(link) = link_to_start {
            // The registry owns this pointer for the process lifetime. Calls
            // are main-thread-only and occur outside the registry lock.
            if let Err(error) = unsafe { sys::DisplayLinkRef::from_ptr_mut(link).start() } {
                displays()
                    .get_mut(&self.display_id)
                    .expect("registered display must exist")
                    .sources
                    .retain(|&source| source != self.frame_requests);
                unsafe {
                    dispatch_suspend(crate::dispatch_sys::dispatch_object_t {
                        _ds: self.frame_requests,
                    });
                }
                return Err(error);
            }
        }
        self.running = true;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        assert_main_thread();
        if !self.running {
            return Ok(());
        }
        unsafe {
            dispatch_suspend(crate::dispatch_sys::dispatch_object_t {
                _ds: self.frame_requests,
            });
        }
        let link_to_stop = {
            let mut registry = displays();
            let entry = registry
                .get_mut(&self.display_id)
                .expect("registered display must exist");
            entry
                .sources
                .retain(|&source| source != self.frame_requests);
            entry.sources.is_empty().then(|| entry.link.as_ptr())
        };
        self.running = false;
        if let Some(link) = link_to_stop {
            unsafe {
                sys::DisplayLinkRef::from_ptr_mut(link).stop()?;
            }
        }
        Ok(())
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        self.stop().log_err();
        // Unsubscribed under the callback lock above. Late CoreVideo callbacks
        // now see no source. Cancel, balance its suspension, then release GCD's
        // owning reference; a suspended dispatch source must never be released.
        unsafe {
            dispatch_source_cancel(self.frame_requests);
            dispatch_resume(crate::dispatch_sys::dispatch_object_t {
                _ds: self.frame_requests,
            });
            dispatch_release(crate::dispatch_sys::dispatch_object_t {
                _ds: self.frame_requests,
            });
        }
    }
}

mod sys {
    //! Derived from display-link crate under the following license:
    //! <https://github.com/BrainiumLLC/display-link/blob/master/LICENSE-MIT>
    //! Apple docs: [CVDisplayLink](https://developer.apple.com/documentation/corevideo/cvdisplaylinkoutputcallback?language=objc)
    #![allow(dead_code, non_upper_case_globals)]

    use anyhow::Result;
    use core_graphics::display::CGDirectDisplayID;
    use foreign_types::{ForeignType, foreign_type};
    use std::{
        ffi::c_void,
        fmt::{self, Debug, Formatter},
    };

    #[derive(Debug)]
    pub enum CVDisplayLink {}

    foreign_type! {
        pub unsafe type DisplayLink {
            type CType = CVDisplayLink;
            fn drop = CVDisplayLinkRelease;
            fn clone = CVDisplayLinkRetain;
        }
    }

    impl Debug for DisplayLink {
        fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
            formatter
                .debug_tuple("DisplayLink")
                .field(&self.as_ptr())
                .finish()
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(crate) struct CVTimeStamp {
        pub version: u32,
        pub video_time_scale: i32,
        pub video_time: i64,
        pub host_time: u64,
        pub rate_scalar: f64,
        pub video_refresh_period: i64,
        pub smpte_time: CVSMPTETime,
        pub flags: u64,
        pub reserved: u64,
    }

    pub type CVTimeStampFlags = u64;

    pub const kCVTimeStampVideoTimeValid: CVTimeStampFlags = 1 << 0;
    pub const kCVTimeStampHostTimeValid: CVTimeStampFlags = 1 << 1;
    pub const kCVTimeStampSMPTETimeValid: CVTimeStampFlags = 1 << 2;
    pub const kCVTimeStampVideoRefreshPeriodValid: CVTimeStampFlags = 1 << 3;
    pub const kCVTimeStampRateScalarValid: CVTimeStampFlags = 1 << 4;
    pub const kCVTimeStampTopField: CVTimeStampFlags = 1 << 16;
    pub const kCVTimeStampBottomField: CVTimeStampFlags = 1 << 17;
    pub const kCVTimeStampVideoHostTimeValid: CVTimeStampFlags =
        kCVTimeStampVideoTimeValid | kCVTimeStampHostTimeValid;
    pub const kCVTimeStampIsInterlaced: CVTimeStampFlags =
        kCVTimeStampTopField | kCVTimeStampBottomField;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub(crate) struct CVSMPTETime {
        pub subframes: i16,
        pub subframe_divisor: i16,
        pub counter: u32,
        pub time_type: u32,
        pub flags: u32,
        pub hours: i16,
        pub minutes: i16,
        pub seconds: i16,
        pub frames: i16,
    }

    pub type CVSMPTETimeType = u32;

    pub const kCVSMPTETimeType24: CVSMPTETimeType = 0;
    pub const kCVSMPTETimeType25: CVSMPTETimeType = 1;
    pub const kCVSMPTETimeType30Drop: CVSMPTETimeType = 2;
    pub const kCVSMPTETimeType30: CVSMPTETimeType = 3;
    pub const kCVSMPTETimeType2997: CVSMPTETimeType = 4;
    pub const kCVSMPTETimeType2997Drop: CVSMPTETimeType = 5;
    pub const kCVSMPTETimeType60: CVSMPTETimeType = 6;
    pub const kCVSMPTETimeType5994: CVSMPTETimeType = 7;

    pub type CVSMPTETimeFlags = u32;

    pub const kCVSMPTETimeValid: CVSMPTETimeFlags = 1 << 0;
    pub const kCVSMPTETimeRunning: CVSMPTETimeFlags = 1 << 1;

    pub type CVDisplayLinkOutputCallback = unsafe extern "C" fn(
        display_link_out: *mut CVDisplayLink,
        // A pointer to the current timestamp. This represents the timestamp when the callback is called.
        current_time: *const CVTimeStamp,
        // A pointer to the output timestamp. This represents the timestamp for when the frame will be displayed.
        output_time: *const CVTimeStamp,
        // Unused
        flags_in: i64,
        // Unused
        flags_out: *mut i64,
        // A pointer to app-defined data.
        display_link_context: *mut c_void,
    ) -> i32;

    #[link(name = "CoreFoundation", kind = "framework")]
    #[link(name = "CoreVideo", kind = "framework")]
    #[allow(improper_ctypes, unknown_lints, clippy::duplicated_attributes)]
    unsafe extern "C" {
        pub fn CVDisplayLinkCreateWithActiveCGDisplays(
            display_link_out: *mut *mut CVDisplayLink,
        ) -> i32;
        pub fn CVDisplayLinkSetCurrentCGDisplay(
            display_link: &mut DisplayLinkRef,
            display_id: u32,
        ) -> i32;
        pub fn CVDisplayLinkSetOutputCallback(
            display_link: &mut DisplayLinkRef,
            callback: CVDisplayLinkOutputCallback,
            user_info: *mut c_void,
        ) -> i32;
        pub fn CVDisplayLinkStart(display_link: &mut DisplayLinkRef) -> i32;
        pub fn CVDisplayLinkStop(display_link: &mut DisplayLinkRef) -> i32;
        pub fn CVDisplayLinkRelease(display_link: *mut CVDisplayLink);
        pub fn CVDisplayLinkRetain(display_link: *mut CVDisplayLink) -> *mut CVDisplayLink;
    }

    impl DisplayLink {
        /// Apple docs: [CVDisplayLinkCreateWithCGDisplay](https://developer.apple.com/documentation/corevideo/1456981-cvdisplaylinkcreatewithcgdisplay?language=objc)
        pub unsafe fn new(
            display_id: CGDirectDisplayID,
            callback: CVDisplayLinkOutputCallback,
            user_info: *mut c_void,
        ) -> Result<Self> {
            unsafe {
                let mut display_link: *mut CVDisplayLink = 0 as _;

                let code = CVDisplayLinkCreateWithActiveCGDisplays(&mut display_link);
                anyhow::ensure!(code == 0, "could not create display link, code: {}", code);

                let mut display_link = DisplayLink::from_ptr(display_link);

                let code = CVDisplayLinkSetOutputCallback(&mut display_link, callback, user_info);
                anyhow::ensure!(code == 0, "could not set output callback, code: {}", code);

                let code = CVDisplayLinkSetCurrentCGDisplay(&mut display_link, display_id);
                anyhow::ensure!(
                    code == 0,
                    "could not assign display to display link, code: {}",
                    code
                );

                Ok(display_link)
            }
        }
    }

    impl DisplayLinkRef {
        /// Apple docs: [CVDisplayLinkStart](https://developer.apple.com/documentation/corevideo/1457193-cvdisplaylinkstart?language=objc)
        pub unsafe fn start(&mut self) -> Result<()> {
            unsafe {
                let code = CVDisplayLinkStart(self);
                anyhow::ensure!(code == 0, "could not start display link, code: {}", code);
                Ok(())
            }
        }

        /// Apple docs: [CVDisplayLinkStop](https://developer.apple.com/documentation/corevideo/1457281-cvdisplaylinkstop?language=objc)
        pub unsafe fn stop(&mut self) -> Result<()> {
            unsafe {
                let code = CVDisplayLinkStop(self);
                anyhow::ensure!(code == 0, "could not stop display link, code: {}", code);
                Ok(())
            }
        }
    }
}
