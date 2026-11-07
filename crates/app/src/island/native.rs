//! Small AppKit boundary. Callbacks enqueue UI work; they never query storage.
use gpui::Window;
use objc2::{
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    MainThreadMarker,
};
use objc2_app_kit::{
    NSColor, NSEvent, NSEventMask, NSScreen, NSView, NSWindow, NSWindowButton,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{
    NSNotificationCenter, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use objc2_quartz_core::{
    CABasicAnimation, CALayer, CAMediaTiming, CAMediaTimingFunction, CATransaction,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use runtime::island::geometry::{IslandFrame, IslandLayout, IslandScreen};
use runtime::island::transition::Morph;
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

/// Window frame plus the visible black shape inside it. The shape is a layer mask animated by the
/// compositor, so a morph costs the main thread nothing after this call.
pub(super) struct Shape {
    pub window: IslandFrame,
    pub surface: IslandLayout,
    pub morph: Option<Morph>,
}

pub(super) fn apply_shape(window: &Window, shape: Shape, cx: &gpui::App) {
    let Some(panel) = panel(window) else { return };
    let f = shape.window;
    // setFrame synchronously calls GPUI's bounds callback. Match MacWindow::resize:
    // execute after the current App/Window lease is released.
    cx.foreground_executor()
        .spawn(async move {
            let frame = NSRect::new(NSPoint::new(f.x, f.y), NSSize::new(f.width, f.height));
            if panel.frame() != frame {
                panel.setFrame_display(frame, true);
            }
            if let Some(layer) = panel.contentView().and_then(|view| view.layer()) {
                shape_layer(&layer, &shape);
            }
        })
        .detach();
}

/// `(width, height, centre x, centre y, radius)` of a surface in the layer's coordinates; every
/// surface is top-anchored in the window.
fn geometry(layer: &CALayer, window: IslandFrame, surface: &IslandLayout) -> [f64; 5] {
    let s = surface.frame;
    let up = s.y - window.y + s.height / 2.;
    let y = if layer.isGeometryFlipped() {
        window.height - up
    } else {
        up
    };
    [
        s.width,
        s.height,
        s.x - window.x + s.width / 2.,
        y,
        surface.radius,
    ]
}

fn shape_layer(layer: &CALayer, shape: &Shape) {
    let mask = layer.mask().unwrap_or_else(|| {
        let mask = CALayer::new();
        mask.setBackgroundColor(Some(&NSColor::blackColor().CGColor()));
        // SAFETY: the mask is a plain CALayer owned by `layer` afterwards.
        unsafe { layer.setMask(Some(&mask)) };
        mask
    });
    let [w, h, cx, cy, radius] = geometry(layer, shape.window, &shape.surface);
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    mask.setBounds(NSRect::new(NSPoint::new(0., 0.), NSSize::new(w, h)));
    mask.setPosition(NSPoint::new(cx, cy));
    mask.setCornerRadius(radius);
    layer.setOpacity(1.);
    CATransaction::commit();
    let Some(morph) = shape.morph else { return };
    let seconds = morph.duration_ms as f64 / 1000.;
    if morph.fade_only {
        animate(layer, "opacity", f64::from(morph.opacity_from), 1., seconds);
        return;
    }
    let from = geometry(layer, shape.window, &morph.from);
    let to = [w, h, cx, cy, radius];
    for (key, (from, to)) in [
        "bounds.size.width",
        "bounds.size.height",
        "position.x",
        "position.y",
        "cornerRadius",
    ]
    .into_iter()
    .zip(from.into_iter().zip(to))
    {
        animate(&mask, key, from, to, seconds);
    }
}

/// Ease-out cubic, the curve the pure transition tests assume.
fn animate(layer: &CALayer, key: &str, from: f64, to: f64, seconds: f64) {
    let key = NSString::from_str(key);
    let animation = CABasicAnimation::animationWithKeyPath(Some(&key));
    let (from, to) = (NSNumber::new_f64(from), NSNumber::new_f64(to));
    // SAFETY: the values are NSNumbers, which CABasicAnimation accepts for scalar key paths.
    unsafe {
        animation.setFromValue(Some(&from));
        animation.setToValue(Some(&to));
    }
    animation.setDuration(seconds);
    animation.setTimingFunction(Some(&CAMediaTimingFunction::functionWithControlPoints(
        0.215, 0.61, 0.355, 1.,
    )));
    layer.addAnimation_forKey(&animation, Some(&key));
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
