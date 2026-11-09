//! Small AppKit boundary. Callbacks enqueue UI work; they never query storage.
use gpui::Window;
use objc2::{
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    MainThreadMarker,
};
use objc2_app_kit::{
    NSColor, NSEvent, NSEventMask, NSScreen, NSView, NSWindow, NSWindowAnimationBehavior,
    NSWindowButton, NSWindowCollectionBehavior, NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{
    NSNotificationCenter, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSValue,
};
use objc2_quartz_core::{
    CABasicAnimation, CALayer, CAMediaTiming, CAMediaTimingFunction, CATransaction,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use runtime::island::geometry::{IslandFrame, IslandLayout, IslandScreen};
use runtime::island::transition::Morph;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

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
            // AppKit's own window resize is main-thread and redraws every frame.
            panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
            panel.setTitle(&NSString::from_str(ui::i18n::text("island.title")));
        })
        .detach();
}

/// Window frame plus the visible black shape inside it. The shape is a layer mask animated by the
/// compositor, so a morph costs the main thread nothing after this call.
#[derive(Clone, Copy)]
pub(super) struct Shape {
    pub window: IslandFrame,
    pub surface: IslandLayout,
    pub morph: Option<Morph>,
}

/// Drops a mask animation that was queued and then replaced by a newer shape.
static MORPH_TICK: AtomicU64 = AtomicU64::new(0);

pub(super) fn apply_shape(window: &Window, shape: Shape, cx: &gpui::App) {
    let Some(panel) = panel(window) else { return };
    let tick = MORPH_TICK.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let reveal = shape
        .morph
        .filter(|morph| !morph.fade_only && morph.duration_ms > 0);
    let panel_for_reveal = panel.clone();
    let shape_for_reveal = shape;
    let reveal_for_snap = reveal;
    // setFrame synchronously calls GPUI's bounds callback. Match MacWindow::resize:
    // execute after the current App/Window lease is released.
    cx.foreground_executor()
        .spawn(async move {
            snap_window(&panel, shape.window);
            if let Some(layer) = panel.contentView().and_then(|view| view.layer()) {
                if let Some(morph) = reveal_for_snap {
                    // Hold the current silhouette over the resized window. The
                    // reveal starts once GPUI has drawn the larger content.
                    let mut held = shape;
                    held.surface = morph.from;
                    held.morph = None;
                    shape_layer(&layer, &held);
                } else {
                    shape_layer(&layer, &shape);
                }
            }
        })
        .detach();
    if reveal.is_none() {
        return;
    }
    let panel = panel_for_reveal;
    let shape = shape_for_reveal;
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(32))
            .await;
        if MORPH_TICK.load(Ordering::Relaxed) != tick {
            return;
        }
        cx.foreground_executor()
            .spawn(async move {
                if MORPH_TICK.load(Ordering::Relaxed) != tick {
                    return;
                }
                if let Some(layer) = panel.contentView().and_then(|view| view.layer()) {
                    shape_layer(&layer, &shape);
                }
            })
            .detach();
    })
    .detach();
}

fn snap_window(panel: &NSWindow, frame: IslandFrame) {
    let frame = NSRect::new(
        NSPoint::new(frame.x, frame.y),
        NSSize::new(frame.width, frame.height),
    );
    if panel.frame() == frame {
        return;
    }
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    panel.setFrame_display_animate(frame, false, false);
    CATransaction::commit();
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
    // `surface` at the moment a morph is created is still the start shape.
    // The animation has to end on `morph.to`, or the mask never moves.
    let to_layout = shape.morph.map(|morph| morph.to).unwrap_or(shape.surface);
    let to = geometry(layer, shape.window, &to_layout);
    let Some(morph) = shape.morph else {
        place(layer, &mask, to, 1.);
        return;
    };
    let seconds = morph.duration_ms as f64 / 1000.;
    // Model layer is the end shape. Explicit animations carry the first frame,
    // in this same transaction, so a draw never sees the end shape early.
    // `bounds` is one animation: width and height key paths both write it and stutter.
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    apply_geometry(&mask, to);
    layer.setOpacity(1.);
    if morph.fade_only {
        animate(
            layer,
            "opacity",
            &NSNumber::new_f64(f64::from(morph.opacity_from)),
            &NSNumber::new_f64(1.),
            seconds,
        );
    } else {
        let from = geometry(layer, shape.window, &morph.from);
        animate(&mask, "bounds", &rect_value(from), &rect_value(to), seconds);
        animate(
            &mask,
            "position",
            &point_value(from),
            &point_value(to),
            seconds,
        );
        animate(
            &mask,
            "cornerRadius",
            &NSNumber::new_f64(from[4]),
            &NSNumber::new_f64(to[4]),
            seconds,
        );
    }
    CATransaction::commit();
}

fn place(layer: &CALayer, mask: &CALayer, geometry: [f64; 5], opacity: f32) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    apply_geometry(mask, geometry);
    layer.setOpacity(opacity);
    CATransaction::commit();
}

fn animate(layer: &CALayer, key: &str, from: &AnyObject, to: &AnyObject, seconds: f64) {
    let key = NSString::from_str(key);
    let animation = CABasicAnimation::animationWithKeyPath(Some(&key));
    // SAFETY: the values are NSNumber/NSValue, which these key paths accept.
    unsafe {
        animation.setFromValue(Some(from));
        animation.setToValue(Some(to));
    }
    animation.setDuration(seconds);
    animation.setTimingFunction(Some(&CAMediaTimingFunction::functionWithControlPoints(
        0.215, 0.61, 0.355, 1.,
    )));
    layer.addAnimation_forKey(&animation, Some(&key));
}

fn rect_value(geometry: [f64; 5]) -> Retained<NSValue> {
    // SAFETY: NSValue copies the rect.
    unsafe {
        NSValue::valueWithRect(NSRect::new(
            NSPoint::new(0., 0.),
            NSSize::new(geometry[0], geometry[1]),
        ))
    }
}

fn point_value(geometry: [f64; 5]) -> Retained<NSValue> {
    // SAFETY: NSValue copies the point.
    unsafe { NSValue::valueWithPoint(NSPoint::new(geometry[2], geometry[3])) }
}

fn apply_geometry(mask: &CALayer, geometry: [f64; 5]) {
    let [w, h, cx, cy, radius] = geometry;
    mask.setBounds(NSRect::new(NSPoint::new(0., 0.), NSSize::new(w, h)));
    mask.setPosition(NSPoint::new(cx, cy));
    mask.setCornerRadius(radius);
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
