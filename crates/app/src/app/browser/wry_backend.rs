//! Wry/WebKit backend for Browser cards.

use super::{BrowserEvent, BrowserViewBounds};
use canvas::NodeId;
use crossbeam_channel::Sender;
use gpui::Window;

pub(crate) struct WryBrowserView {
    webview: wry::WebView,
}

impl WryBrowserView {
    pub(crate) fn create(
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
        event_sender: Sender<BrowserEvent>,
    ) -> wry::Result<Self> {
        let webview = wry::WebViewBuilder::new()
            .with_url(url)
            .with_bounds(wry_rect(bounds))
            .with_document_title_changed_handler(move |title| {
                let _ = event_sender.send(BrowserEvent::TitleChanged { node_id, title });
            })
            .with_new_window_req_handler(|_url, _features| wry::NewWindowResponse::Deny)
            .build_as_child(window)?;
        clip_native_webview_to_bottom_corners(&webview, bounds.bottom_corner_radius_px);
        Ok(Self { webview })
    }

    pub(crate) fn load_url(&mut self, url: &str) {
        let _ = self.webview.load_url(url);
    }

    pub(crate) fn go_back(&mut self) {
        let _ = self.webview.evaluate_script("history.back()");
    }

    pub(crate) fn go_forward(&mut self) {
        let _ = self.webview.evaluate_script("history.forward()");
    }

    pub(crate) fn reload(&mut self) {
        let _ = self.webview.reload();
    }

    pub(crate) fn set_bounds(&mut self, bounds: BrowserViewBounds) {
        let _ = self.webview.set_bounds(wry_rect(bounds));
        clip_native_webview_to_bottom_corners(&self.webview, bounds.bottom_corner_radius_px);
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        let _ = self.webview.set_visible(visible);
    }
}

fn clip_native_webview_to_bottom_corners(webview: &wry::WebView, radius_px: f64) {
    use objc2_quartz_core::CACornerMask;
    use wry::WebViewExtMacOS;

    let native_view = webview.webview();
    native_view.setWantsLayer(true);
    if let Some(layer) = native_view.layer() {
        layer.setCornerRadius(radius_px);
        layer.setMaskedCorners(
            CACornerMask::LayerMinXMaxYCorner | CACornerMask::LayerMaxXMaxYCorner,
        );
        layer.setMasksToBounds(true);
    }
}

fn wry_rect(bounds: BrowserViewBounds) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::LogicalPosition::new(bounds.x, bounds.y).into(),
        size: wry::dpi::LogicalSize::new(bounds.width, bounds.height).into(),
    }
}
