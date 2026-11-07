//! Small AppKit boundary. Callbacks enqueue UI work; they never query storage.
use gpui::Window;
use objc2::{
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    MainThreadMarker,
};
use objc2_app_kit::{
    NSEvent, NSEventMask, NSScreen, NSView, NSWindow, NSWindowButton, NSWindowCollectionBehavior,
    NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{
    NSNotificationCenter, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use runtime::island::geometry::{IslandFrame, IslandLayout, IslandScreen};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug)]
pub(super) enum NativeEvent {
    ScreensChanged,
    Pointer,
}

pub(super) fn screens() -> Vec<(u32, IslandScreen)> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    NSScreen::screens(mtm)
        .into_iter()
        .filter_map(|screen| {
            let id = screen
                .deviceDescription()
                .objectForKey(&NSString::from_str("NSScreenNumber"))?
                .downcast::<NSNumber>()
                .ok()?
                .unsignedIntValue();
            let frame = screen.frame();
            let visible = screen.visibleFrame();
            let left = screen.auxiliaryTopLeftArea();
            let right = screen.auxiliaryTopRightArea();
            Some((
                id,
                IslandScreen {
                    frame: IslandFrame {
                        x: frame.origin.x,
                        y: frame.origin.y,
                        width: frame.size.width,
                        height: frame.size.height,
                    },
                    visible_top: visible.origin.y + visible.size.height,
                    safe_top: screen.safeAreaInsets().top,
                    left_max_x: left.origin.x + left.size.width,
                    right_min_x: right.origin.x,
                },
            ))
        })
        .collect()
}

pub(super) fn panel(window: &Window) -> Option<Retained<NSWindow>> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: GPUI owns this live NSView; callers are on the AppKit thread.
    unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() }.window()
}

pub(super) fn configure(window: &Window, cx: &gpui::App) {
    let Some(panel) = panel(window) else { return };
    // AppKit can recreate the titlebar controls while the initial window is shown.
    // Remove the titlebar itself after GPUI releases its construction lease.
    cx.foreground_executor()
        .spawn(async move {
            panel.setStyleMask(NSWindowStyleMask::NonactivatingPanel);
            panel.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            panel.setHasShadow(false);
            panel.setTitleVisibility(NSWindowTitleVisibility::Hidden);
            panel.setTitlebarAppearsTransparent(true);
            for kind in [
                NSWindowButton::CloseButton,
                NSWindowButton::MiniaturizeButton,
                NSWindowButton::ZoomButton,
            ] {
                if let Some(button) = panel.standardWindowButton(kind) {
                    button.setHidden(true);
                }
            }
            panel.setHidesOnDeactivate(false);
            panel.setTitle(&NSString::from_str(ui::i18n::text("island.title")));
        })
        .detach();
}

pub(super) fn resize(window: &Window, layout: IslandLayout, cx: &gpui::App) {
    let Some(panel) = panel(window) else { return };
    let f = layout.frame;
    // setFrame synchronously calls GPUI's bounds callback. Match MacWindow::resize:
    // execute after the current App/Window lease is released.
    cx.foreground_executor()
        .spawn(async move {
            let frame = NSRect::new(NSPoint::new(f.x, f.y), NSSize::new(f.width, f.height));
            if panel.frame() != frame {
                panel.setFrame_display(frame, true);
            }
        })
        .detach();
}

pub(super) fn set_visible(panel: Retained<NSWindow>, visible: bool, cx: &gpui::App) {
    // AppKit visibility callbacks must run after GPUI releases its current leases.
    cx.foreground_executor()
        .spawn(async move {
            if visible {
                panel.orderFrontRegardless();
            } else {
                panel.setIgnoresMouseEvents(true);
                panel.orderOut(None);
            }
        })
        .detach();
}

pub(super) fn pointer() -> (f64, f64) {
    let p = NSEvent::mouseLocation();
    (p.x, p.y)
}

pub(super) fn pointer_screen() -> Option<u32> {
    let (x, y) = pointer();
    screens().into_iter().find_map(|(id, screen)| {
        let f = screen.frame;
        (x >= f.x && x < f.x + f.width && y >= f.y && y < f.y + f.height).then_some(id)
    })
}

pub(super) struct HitPanel {
    pub display: u32,
    pub panel: Retained<NSWindow>,
    pub layout: IslandLayout,
}

pub(super) struct Watch {
    observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    global: Option<Retained<AnyObject>>,
    local: Option<Retained<AnyObject>>,
    pub panels: Rc<RefCell<Vec<HitPanel>>>,
}

impl Watch {
    pub fn new(sender: async_channel::Sender<NativeEvent>) -> Self {
        let panels = Rc::new(RefCell::new(Vec::<HitPanel>::new()));
        let tx = sender.clone();
        let block = block2::RcBlock::new(move |_| {
            let _ = tx.try_send(NativeEvent::ScreensChanged);
        });
        // SAFETY: the block captures only a Send channel. The observer is removed on Drop.
        let observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(objc2_app_kit::NSApplicationDidChangeScreenParametersNotification),
                None,
                None,
                &block,
            )
        };
        let watched = panels.clone();
        let tx = sender.clone();
        let global = block2::RcBlock::new(move |_| {
            update_hits(&watched);
            let _ = tx.try_send(NativeEvent::Pointer);
        });
        let global = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::MouseMoved,
            &global,
        );
        let watched = panels.clone();
        let local = block2::RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| {
            update_hits(&watched);
            let _ = sender.try_send(NativeEvent::Pointer);
            event.as_ptr()
        });
        // SAFETY: returns the exact borrowed event; does not retain or alter the event stream.
        let local = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::MouseMoved, &local)
        };
        Self {
            observer,
            global,
            local,
            panels,
        }
    }

    pub fn pointer_display(&self) -> Option<u32> {
        update_hits(&self.panels);
        let (x, y) = pointer();
        self.panels
            .borrow()
            .iter()
            .find(|p| p.panel.isVisible() && p.layout.contains(x, y))
            .map(|p| p.display)
    }
}

fn update_hits(panels: &RefCell<Vec<HitPanel>>) {
    let (x, y) = pointer();
    for p in panels.borrow().iter() {
        p.panel
            .setIgnoresMouseEvents(!p.panel.isVisible() || !p.layout.contains(x, y));
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // SAFETY: all tokens came from the matching registrations above.
        unsafe {
            NSNotificationCenter::defaultCenter().removeObserver((*self.observer).as_ref());
            for monitor in [&self.global, &self.local].into_iter().flatten() {
                NSEvent::removeMonitor(monitor);
            }
        }
    }
}
