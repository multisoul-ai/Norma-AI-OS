//! Chromium Embedded Framework backend boundary for Browser cards.
//!
//! The backend is feature-gated because CEF requires native framework resources
//! and helper processes that the default Wry/WebKit path must not depend on.

use super::chrome_auth::{BrowserCookie, SameSitePolicy};
use super::{BrowserEvent, BrowserViewBounds};
use crate::app::cef_process::{
    cef_framework_dir_cef_string, cef_resources_dir_cef_string, load_cef_library,
};
use canvas::NodeId;
use cef::{
    browser_host_create_browser_sync, cookie_manager_get_global_manager, initialize, Basetime,
    Browser, BrowserSettings, CefString, Cookie, CookieSameSite, ImplBrowser, ImplBrowserHost,
    ImplCookieManager, ImplFrame, Rect, Settings, WindowInfo,
};
use crossbeam_channel::Sender;
use gpui::Window;
use objc2_app_kit::NSView;
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::RawWindowHandle;
use std::sync::OnceLock;

static CEF_INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();

pub(crate) struct CefBrowserView {
    browser: Browser,
}

impl CefBrowserView {
    pub(crate) fn create(
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
        event_sender: Sender<BrowserEvent>,
    ) -> Result<Self, String> {
        let _ = (node_id, event_sender);
        initialize_cef()?;

        let parent_view = parent_view_handle(window)?;
        let window_info =
            WindowInfo::default().set_as_child(parent_view, &cef_rect_for_bounds(bounds));
        let url = CefString::from(url);
        let browser = browser_host_create_browser_sync(
            Some(&window_info),
            None,
            Some(&url),
            Some(&BrowserSettings::default()),
            None,
            None,
        )
        .ok_or_else(|| "CEF failed to create a browser host".to_string())?;

        Ok(Self { browser })
    }

    pub(crate) fn load_url(&mut self, url: &str) {
        if let Some(frame) = self.browser.main_frame() {
            frame.load_url(Some(&CefString::from(url)));
        }
    }

    pub(crate) fn current_url(&self) -> Option<String> {
        let frame = self.browser.main_frame()?;
        let url = frame.url();
        let url = CefString::from(&url).to_string();
        (!url.is_empty()).then_some(url)
    }

    pub(crate) fn go_back(&mut self) {
        self.browser.go_back();
    }

    pub(crate) fn go_forward(&mut self) {
        self.browser.go_forward();
    }

    pub(crate) fn reload(&mut self) {
        self.browser.reload();
    }

    pub(crate) fn set_bounds(&mut self, bounds: BrowserViewBounds) {
        if let Some(host) = self.browser.host() {
            let native_view = host.window_handle();
            if !native_view.is_null() {
                set_native_view_frame(native_view, bounds);
            }
            host.notify_move_or_resize_started();
            host.was_resized();
        }
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        if let Some(host) = self.browser.host() {
            host.was_hidden((!visible) as i32);
        }
    }

    pub(crate) fn release_input(&mut self) {
        if let Some(host) = self.browser.host() {
            host.set_focus(0);
        }
    }

    pub(crate) fn import_cookies(
        &mut self,
        source_url: &str,
        cookies: &[BrowserCookie],
    ) -> Result<usize, String> {
        let manager = cookie_manager_get_global_manager(None)
            .ok_or_else(|| "CEF CookieManager was unavailable".to_string())?;
        let source_url = CefString::from(source_url);
        for cookie in cookies {
            let cookie = cef_cookie_from_browser_cookie(cookie)?;
            if manager.set_cookie(Some(&source_url), Some(&cookie), None) == 0 {
                return Err("CEF rejected an imported Chrome Cookie".to_string());
            }
        }
        Ok(cookies.len())
    }
}

impl Drop for CefBrowserView {
    fn drop(&mut self) {
        if let Some(host) = self.browser.host() {
            host.close_browser(1);
        }
    }
}

fn initialize_cef() -> Result<(), String> {
    CEF_INITIALIZED
        .get_or_init(|| {
            load_cef_library()?;
            let settings = Settings {
                no_sandbox: 1,
                browser_subprocess_path: current_exe_cef_string()?,
                framework_dir_path: cef_framework_dir_cef_string()?,
                resources_dir_path: cef_resources_dir_cef_string()?,
                root_cache_path: cef_root_cache_path_cef_string()?,
                external_message_pump: 1,
                ..Settings::default()
            };
            let args = cef::args::Args::new();
            if initialize(
                Some(args.as_main_args()),
                Some(&settings),
                None,
                std::ptr::null_mut(),
            ) == 0
            {
                Err("CEF initialization failed".to_string())
            } else {
                Ok(())
            }
        })
        .clone()
}

pub(crate) fn pump_cef_message_loop_if_initialized() -> bool {
    if !matches!(CEF_INITIALIZED.get(), Some(Ok(()))) {
        return false;
    }
    cef::do_message_loop_work();
    true
}

fn current_exe_cef_string() -> Result<CefString, String> {
    let path = std::env::current_exe()
        .map_err(|err| format!("failed to resolve CEF subprocess path: {err}"))?;
    let path = path
        .to_str()
        .ok_or_else(|| format!("CEF subprocess path is not valid UTF-8: {}", path.display()))?;
    Ok(CefString::from(path))
}

fn cef_root_cache_path_cef_string() -> Result<CefString, String> {
    let path = storage::default_app_data_dir()
        .map_err(|err| format!("failed to resolve CEF cache root: {err}"))?
        .join("cef");
    std::fs::create_dir_all(&path)
        .map_err(|err| format!("failed to create CEF cache root {}: {err}", path.display()))?;
    let path = path
        .to_str()
        .ok_or_else(|| format!("CEF cache root is not valid UTF-8: {}", path.display()))?;
    Ok(CefString::from(path))
}

fn parent_view_handle(window: &Window) -> Result<cef::sys::cef_window_handle_t, String> {
    let handle = raw_window_handle::HasWindowHandle::window_handle(window)
        .map_err(|err| format!("failed to get GPUI window handle for CEF: {err}"))?;
    match handle.as_raw() {
        RawWindowHandle::AppKit(handle) => Ok(handle.ns_view.as_ptr().cast()),
        _ => Err("CEF Browser backend requires an AppKit window handle on macOS".to_string()),
    }
}

fn cef_rect_for_bounds(bounds: BrowserViewBounds) -> Rect {
    Rect {
        x: bounds.x.round() as i32,
        y: bounds.y.round() as i32,
        width: bounds.width.round().max(0.0) as i32,
        height: bounds.height.round().max(0.0) as i32,
    }
}

fn set_native_view_frame(native_view: cef::sys::cef_window_handle_t, bounds: BrowserViewBounds) {
    let frame = NSRect::new(
        NSPoint::new(bounds.x, bounds.y),
        NSSize::new(bounds.width.max(0.0), bounds.height.max(0.0)),
    );
    let native_view = native_view.cast::<NSView>();
    unsafe {
        (*native_view).setFrame(frame);
    }
}

fn cef_cookie_from_browser_cookie(cookie: &BrowserCookie) -> Result<Cookie, String> {
    let (has_expires, expires) = match cookie.expires_unix_seconds {
        Some(unix_seconds) => {
            // CEF basetime uses microseconds since 1601-01-01 UTC.
            // Unix epoch offset = 11_644_473_600 seconds.
            let windows_seconds = unix_seconds
                .checked_add(11_644_473_600)
                .ok_or_else(|| "Chrome Cookie expiry overflowed CEF basetime".to_string())?;
            let value = windows_seconds
                .checked_mul(1_000_000)
                .ok_or_else(|| "Chrome Cookie expiry overflowed CEF microseconds".to_string())?;
            (1, Basetime { val: value })
        }
        None => (0, Basetime::default()),
    };
    Ok(Cookie {
        name: CefString::from(cookie.name.as_str()),
        value: CefString::from(cookie.value.as_str()),
        domain: CefString::from(cef_cookie_domain(cookie)),
        path: CefString::from(cookie.path.as_str()),
        secure: cookie.secure as i32,
        httponly: cookie.http_only as i32,
        has_expires,
        expires,
        same_site: match cookie.same_site {
            SameSitePolicy::Unspecified => CookieSameSite::UNSPECIFIED,
            SameSitePolicy::NoRestriction => CookieSameSite::NO_RESTRICTION,
            SameSitePolicy::Lax => CookieSameSite::LAX_MODE,
            SameSitePolicy::Strict => CookieSameSite::STRICT_MODE,
        },
        ..Cookie::default()
    })
}

pub(super) fn cef_cookie_domain(cookie: &BrowserCookie) -> &str {
    if cookie.host_only {
        ""
    } else {
        cookie.domain.as_str()
    }
}
