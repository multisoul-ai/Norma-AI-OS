//! AppKit performs the native content transform and its inverse input mapping.
use gpui::{ScenePresentation, Window};
use objc2::{
    define_class, msg_send, rc::Retained, DefinedClass, MainThreadMarker, MainThreadOnly, Message,
};
use objc2_app_kit::NSView;
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::Cell;

define_class!(
    #[unsafe(super(NSView))]
    #[ivars = Cell<bool>]
    struct NormaSceneClipView;

    impl NormaSceneClipView {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            if self.ivars().get() {
                // SAFETY: NSView's implementation receives a point in its superview coordinates.
                unsafe { msg_send![super(self), hitTest: point] }
            } else { None }
        }
    }
);

pub(super) struct NativeSceneClip {
    container: Retained<NormaSceneClipView>,
    content: Retained<NSView>,
}

impl NativeSceneClip {
    pub fn new(content: &NSView) -> Self {
        let allocated = NormaSceneClipView::alloc(
            MainThreadMarker::new().expect("native scene on main thread"),
        )
        .set_ivars(Cell::new(false));
        // SAFETY: initializes an allocated NSView subclass on the AppKit thread.
        let container: Retained<NormaSceneClipView> = unsafe { msg_send![super(allocated), init] };
        container.setWantsLayer(true);
        container.setAutoresizesSubviews(false);
        container.setHidden(true);
        if let Some(layer) = container.layer() {
            layer.setMasksToBounds(true);
        }
        container.addSubview(content);
        Self {
            container,
            content: content.retain(),
        }
    }

    pub fn present(&self, placement: ScenePresentation, interactive: bool, window: &Window) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: GPUI's live AppKit window owns this NSView; it is accessed only on the main thread.
        let parent = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
        // SAFETY: the container and parent are live AppKit objects on this main thread.
        if unsafe { self.container.superview() }.as_deref() != Some(parent) {
            parent.addSubview(&self.container);
        }
        let number = |value: gpui::Pixels| f64::from(f32::from(value));
        let visible = placement.visible_bounds;
        let frame = placement.bounds;
        let scale = f64::from(placement.scale);
        let size = NSSize::new(number(visible.size.width), number(visible.size.height));
        if size.width <= 0.0 || size.height <= 0.0 || placement.opacity <= 0.0 {
            self.set_visible(false);
            return;
        }
        let y = if parent.isFlipped() {
            number(visible.origin.y)
        } else {
            parent.bounds().size.height - number(visible.origin.y) - size.height
        };
        self.container
            .setFrame(NSRect::new(NSPoint::new(number(visible.origin.x), y), size));
        let logical_clip = NSSize::new(size.width / scale, size.height / scale);
        self.container
            .setBounds(NSRect::new(NSPoint::new(0.0, 0.0), logical_clip));
        // The clipping host scales; WKWebView/CEF keeps its original document viewport.
        let logical_size = NSSize::new(
            number(placement.logical_size.width),
            number(placement.logical_size.height),
        );
        let origin = NSPoint::new(
            number(frame.origin.x - visible.origin.x) / scale,
            logical_clip.height
                - number(frame.origin.y - visible.origin.y) / scale
                - logical_size.height,
        );
        let content_frame = NSRect::new(origin, logical_size);
        if self.content.frame() != content_frame {
            self.content.setFrame(content_frame);
        }
        self.container.setAlphaValue(f64::from(placement.opacity));
        self.container.ivars().set(interactive);
        self.set_visible(true);
    }

    pub fn set_visible(&self, visible: bool) {
        self.container.setHidden(!visible);
        if !visible {
            self.container.ivars().set(false);
        }
    }

    pub fn release_input(&self) {
        let Some(window) = self.content.window() else {
            return;
        };
        let Some(responder) = window
            .firstResponder()
            .and_then(|view| view.downcast::<NSView>().ok())
        else {
            return;
        };
        if responder.isDescendantOf(&self.content) {
            // SAFETY: both views belong to the same live window on the AppKit thread.
            if let Some(parent) = unsafe { self.container.superview() } {
                window.makeFirstResponder(Some(&parent));
            }
        }
    }
}

impl Drop for NativeSceneClip {
    fn drop(&mut self) {
        self.container.removeFromSuperview();
    }
}
