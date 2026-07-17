#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
use super::cef_backend::CefBrowserView;
use super::chrome_auth::BrowserCookie;
#[cfg(target_os = "macos")]
use super::wry_backend::WryBrowserView;
use super::{BrowserEngineKind, BrowserEvent};
use canvas::NodeId;
use crossbeam_channel::Sender;
use gpui::Window;

pub(crate) enum CookieImportStatus {
    Complete(usize),
    Pending,
}

// Browser 卡片在 selected/focused 时绘制 2px GPUI 边框；原生子视图必须完整避让它，
// 否则 AppKit 的子层会盖住蓝色选中态。
const BROWSER_NATIVE_BORDER_INSET_PX: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BrowserViewBounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) bottom_corner_radius_px: f64,
}

pub(crate) fn browser_native_view_bounds(
    card_x: f64,
    card_y: f64,
    card_width: f64,
    card_height: f64,
    header_height: f64,
    bottom_corner_radius_px: f64,
) -> BrowserViewBounds {
    BrowserViewBounds {
        x: card_x + BROWSER_NATIVE_BORDER_INSET_PX,
        y: card_y + header_height + BROWSER_NATIVE_BORDER_INSET_PX,
        width: (card_width - BROWSER_NATIVE_BORDER_INSET_PX * 2.0).max(0.0),
        height: (card_height - header_height - BROWSER_NATIVE_BORDER_INSET_PX * 2.0).max(0.0),
        bottom_corner_radius_px,
    }
}

pub(crate) enum NativeBrowserView {
    #[cfg(target_os = "macos")]
    Wry(WryBrowserView),
    #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
    Cef(CefBrowserView),
}

impl NativeBrowserView {
    pub(crate) fn create(
        engine: BrowserEngineKind,
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
        event_sender: Sender<BrowserEvent>,
    ) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        {
            match engine {
                BrowserEngineKind::Wry => {
                    WryBrowserView::create(node_id, url, bounds, window, event_sender)
                        .map(Self::Wry)
                        .map_err(|err| err.to_string())
                }
                #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
                BrowserEngineKind::Cef => {
                    create_cef_view(node_id, url, bounds, window, event_sender)
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (engine, node_id, url, bounds, window, event_sender);
            Err("native Browser views are only implemented on macOS".to_string())
        }
    }

    pub(crate) fn load_url(&mut self, url: &str) {
        match self {
            #[cfg(target_os = "macos")]
            Self::Wry(view) => view.load_url(url),
            #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
            Self::Cef(view) => view.load_url(url),
        }
    }

    pub(crate) fn go_back(&mut self) {
        match self {
            #[cfg(target_os = "macos")]
            Self::Wry(view) => view.go_back(),
            #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
            Self::Cef(view) => view.go_back(),
        }
    }

    pub(crate) fn go_forward(&mut self) {
        match self {
            #[cfg(target_os = "macos")]
            Self::Wry(view) => view.go_forward(),
            #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
            Self::Cef(view) => view.go_forward(),
        }
    }

    pub(crate) fn reload(&mut self) {
        match self {
            #[cfg(target_os = "macos")]
            Self::Wry(view) => view.reload(),
            #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
            Self::Cef(view) => view.reload(),
        }
    }

    pub(crate) fn set_bounds(&mut self, bounds: BrowserViewBounds) {
        match self {
            #[cfg(target_os = "macos")]
            Self::Wry(view) => view.set_bounds(bounds),
            #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
            Self::Cef(view) => view.set_bounds(bounds),
        }
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        match self {
            #[cfg(target_os = "macos")]
            Self::Wry(view) => view.set_visible(visible),
            #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
            Self::Cef(view) => view.set_visible(visible),
        }
    }

    pub(crate) fn release_input(&mut self) {
        match self {
            #[cfg(target_os = "macos")]
            Self::Wry(view) => view.release_input(),
            #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
            Self::Cef(view) => view.release_input(),
        }
    }

    pub(crate) fn import_cookies(
        &mut self,
        node_id: NodeId,
        source_url: &str,
        cookies: &[BrowserCookie],
        skipped: usize,
    ) -> Result<CookieImportStatus, String> {
        #[cfg(not(all(target_os = "macos", feature = "chromium-browser")))]
        let _ = source_url;
        match self {
            #[cfg(target_os = "macos")]
            Self::Wry(view) => view.import_cookies(node_id, source_url, cookies, skipped),
            #[cfg(all(target_os = "macos", feature = "chromium-browser"))]
            Self::Cef(view) => view
                .import_cookies(source_url, cookies)
                .map(CookieImportStatus::Complete),
        }
    }
}

#[cfg(all(target_os = "macos", feature = "chromium-browser"))]
fn create_cef_view(
    node_id: NodeId,
    url: &str,
    bounds: BrowserViewBounds,
    window: &mut Window,
    event_sender: Sender<BrowserEvent>,
) -> Result<NativeBrowserView, String> {
    CefBrowserView::create(node_id, url, bounds, window, event_sender).map(NativeBrowserView::Cef)
}
