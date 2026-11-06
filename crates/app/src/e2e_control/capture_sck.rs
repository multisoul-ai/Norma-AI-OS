//! Capture only this process's window, including WebKit's remote video layers.
//! Unlike CGWindowListCreateImage, ScreenCaptureKit includes those media surfaces.
use block2::RcBlock;
use objc2::{
    msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
};
use objc2_foundation::{NSArray, NSError, NSRect};
use std::{ffi::c_void, sync::mpsc, time::Duration};

type Pixels = (u32, u32, Vec<u8>);

#[link(name = "ScreenCaptureKit", kind = "framework")]
extern "C" {}

/// Called on the control connection worker, never the GPUI/AppKit thread.
/// The macOS 14.4 current-process API deliberately avoids global capture consent.
pub(super) fn capture(window_number: u32) -> Result<Pixels, String> {
    let class = objc2::runtime::AnyClass::get(c"SCShareableContent")
        .ok_or("ScreenCaptureKit unavailable")?;
    let available: bool = unsafe {
        msg_send![class, respondsToSelector: objc2::sel!(getCurrentProcessShareableContentWithCompletionHandler:)]
    };
    if !available {
        return Err("current-process window capture requires macOS 14.4".into());
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    let completion = RcBlock::new(move |content: *mut AnyObject, error: *mut NSError| {
        if let Some(error) = unsafe { error.as_ref() } {
            let _ = sender.send(Err(error.localizedDescription().to_string()));
            return;
        }
        let Some(content) = (unsafe { content.as_ref() }) else {
            let _ = sender.send(Err("own window inventory unavailable".into()));
            return;
        };
        // SAFETY: SCShareableContent returns an NSArray<SCWindow *> retained here.
        let windows: Retained<NSArray<AnyObject>> = unsafe { msg_send![content, windows] };
        let window = windows.iter().find(|window| {
            let number: u32 = unsafe { msg_send![&**window, windowID] };
            number == window_number
        });
        let Some(window) = window else {
            let _ = sender.send(Err("target is not a current-process window".into()));
            return;
        };
        let result = begin_capture(&window, sender.clone());
        if let Err(error) = result {
            let _ = sender.send(Err(error));
        }
    });
    // SAFETY: framework copies this completion until the asynchronous call finishes.
    unsafe {
        let _: () =
            msg_send![class, getCurrentProcessShareableContentWithCompletionHandler: &*completion];
    }
    receiver
        .recv_timeout(Duration::from_secs(8))
        .map_err(|_| "own window capture timed out".to_string())?
}

fn begin_capture(
    window: &AnyObject,
    sender: mpsc::SyncSender<Result<Pixels, String>>,
) -> Result<(), String> {
    let filter_class =
        objc2::runtime::AnyClass::get(c"SCContentFilter").ok_or("capture filter unavailable")?;
    let config_class = objc2::runtime::AnyClass::get(c"SCStreamConfiguration")
        .ok_or("capture configuration unavailable")?;
    let manager = objc2::runtime::AnyClass::get(c"SCScreenshotManager")
        .ok_or("screenshot manager unavailable")?;
    // SAFETY: selectors and argument types follow ScreenCaptureKit's public SDK headers.
    unsafe {
        let allocated: Allocated<AnyObject> = msg_send![filter_class, alloc];
        let filter: Retained<AnyObject> =
            msg_send![allocated, initWithDesktopIndependentWindow: window];
        let config: Retained<AnyObject> = msg_send![config_class, new];
        let rect: NSRect = msg_send![&*filter, contentRect];
        let scale: f32 = msg_send![&*filter, pointPixelScale];
        let width = (rect.size.width * f64::from(scale)).round() as usize;
        let height = (rect.size.height * f64::from(scale)).round() as usize;
        let _: () = msg_send![&*config, setWidth: width];
        let _: () = msg_send![&*config, setHeight: height];
        let _: () = msg_send![&*config, setShowsCursor: false];
        let _: () = msg_send![&*config, setIgnoreShadowsSingleWindow: true];
        let completion = RcBlock::new(move |image: *mut c_void, error: *mut NSError| {
            let result = if let Some(error) = error.as_ref() {
                Err(error.localizedDescription().to_string())
            } else if image.is_null() {
                Err("window image unavailable".into())
            } else {
                // Copy while the callback owns the CGImage; no borrowed image escapes.
                super::capture::rgba_pixels(image)
            };
            let _ = sender.send(result);
        });
        let _: () = msg_send![manager, captureImageWithFilter: &*filter, configuration: &*config, completionHandler: &*completion];
    }
    Ok(())
}
