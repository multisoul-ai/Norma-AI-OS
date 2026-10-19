//! Wry/WebKit backend for Browser cards.

use super::chrome_auth::{BrowserCookie, SameSitePolicy};
use super::load_timeline::{BrowserLoadPhase, CornerClipState};
use super::resource_timing::{log_resource_timing, RESOURCE_TIMING_SCRIPT};
use super::{BrowserEvent, BrowserViewBounds};
use canvas::NodeId;
use crossbeam_channel::Sender;
use gpui::Window;
use objc2::runtime::AnyObject;
use objc2_foundation::{
    ns_string, NSArray, NSDate, NSHTTPCookie, NSHTTPCookieDomain, NSHTTPCookieExpires,
    NSHTTPCookieName, NSHTTPCookiePath, NSHTTPCookiePropertyKey, NSHTTPCookieSecure,
    NSHTTPCookieValue, NSMutableDictionary, NSString,
};
use std::time::Instant;
use wry::WebViewExtMacOS;

use super::native::CookieImportStatus;
use super::safari_identity::with_safari_browser_identity;

const BROWSER_PAGE_INTERACTION_MESSAGE: &str = "multisoul:browser-page-interaction";
const BROWSER_PAGE_INTERACTION_SCRIPT: &str = r#"
(() => {
    const notifyHost = () => window.ipc?.postMessage("multisoul:browser-page-interaction");
    window.addEventListener("pointerdown", notifyHost, true);
    window.addEventListener("focusin", notifyHost, true);
})();
"#;

/// 隐藏的 1x1 预热视图：提前启动 WebContent / Networking 进程并常驻，
/// 让第一张真实 Browser 卡片不再承担 WebKit 子进程冷启动成本。
pub(crate) struct WarmWebView {
    _webview: wry::WebView,
}

impl WarmWebView {
    pub(crate) fn create(window: &mut Window) -> wry::Result<Self> {
        let started = Instant::now();
        let webview = with_safari_browser_identity(wry::WebViewBuilder::new())
            .with_url("about:blank")
            .with_visible(false)
            .with_bounds(wry_rect(BrowserViewBounds {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                bottom_corner_radius_px: 0.0,
            }))
            .build_as_child(window)?;
        tracing::info!(
            build_ms = started.elapsed().as_secs_f64() * 1000.0,
            "browser_warmup_created"
        );
        Ok(Self { _webview: webview })
    }
}

pub(crate) struct WryBrowserView {
    webview: wry::WebView,
    scene_clip: super::scene_clip::NativeSceneClip,
    event_sender: Sender<BrowserEvent>,
    corner_clip: CornerClipState,
}

impl WryBrowserView {
    pub(crate) fn create(
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
        event_sender: Sender<BrowserEvent>,
    ) -> wry::Result<Self> {
        let create_started = Instant::now();
        let browser_event_sender = event_sender.clone();
        let title_sender = event_sender.clone();
        let location_sender = event_sender.clone();
        let new_window_sender = event_sender.clone();
        let navigation_sender = event_sender.clone();
        let preview_url = url.starts_with("norma-content://").then(|| url.to_string());
        let webview = with_safari_browser_identity(wry::WebViewBuilder::new())
            .with_url(url)
            .with_asynchronous_custom_protocol("norma-content".into(), |_, request, responder| {
                std::thread::spawn(move || {
                    responder.respond(crate::preview::local_content::respond(request))
                });
            })
            .with_bounds(wry_rect(bounds))
            .with_navigation_handler(move |target| {
                if let Some(original) = &preview_url {
                    let preview_document = url::Url::parse(&target).is_ok_and(|u| {
                        u.scheme() == "norma-content" && u.path() == "/__norma_preview__"
                    });
                    if !preview_document && target.split('#').next() != original.split('#').next() {
                        let _ = navigation_sender.send(BrowserEvent::PreviewNavigation {
                            node_id,
                            url: target,
                        });
                        return false;
                    }
                }
                true
            })
            .with_initialization_script_for_main_only(BROWSER_PAGE_INTERACTION_SCRIPT, false)
            .with_ipc_handler(move |request| {
                if request.body() == BROWSER_PAGE_INTERACTION_MESSAGE {
                    let _ = event_sender.send(BrowserEvent::PageInteraction { node_id });
                }
            })
            .with_document_title_changed_handler(move |title| {
                let _ = title_sender.send(BrowserEvent::TitleChanged { node_id, title });
            })
            .with_on_page_load_handler(move |event, url| {
                let phase = match event {
                    wry::PageLoadEvent::Started => BrowserLoadPhase::Started,
                    wry::PageLoadEvent::Finished => BrowserLoadPhase::Finished,
                };
                let _ = location_sender.send(BrowserEvent::PageLoad {
                    node_id,
                    phase,
                    at: Instant::now(),
                });
                let _ = location_sender.send(BrowserEvent::LocationChanged { node_id, url });
            })
            .with_new_window_req_handler(move |url, _features| {
                forward_new_window_to_current_card(&new_window_sender, node_id, url)
            })
            .build_as_child(window)?;
        let build_ms = create_started.elapsed().as_secs_f64() * 1000.0;
        let mut corner_clip = CornerClipState::default();
        if corner_clip.needs_apply(bounds.bottom_corner_radius_px) {
            clip_native_webview_to_bottom_corners(&webview, bounds.bottom_corner_radius_px);
        }
        tracing::info!(
            node_id,
            build_ms,
            total_ms = create_started.elapsed().as_secs_f64() * 1000.0,
            width = bounds.width,
            height = bounds.height,
            "browser_wry_created"
        );
        let scene_clip = super::scene_clip::NativeSceneClip::new(&webview.webview());
        Ok(Self {
            webview,
            scene_clip,
            event_sender: browser_event_sender,
            corner_clip,
        })
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

    pub(crate) fn stop_loading(&self) {
        let _ = self.webview.evaluate_script("window.stop()");
    }

    pub(crate) fn set_bounds(&mut self, bounds: BrowserViewBounds) {
        let _ = self.webview.set_bounds(wry_rect(bounds));
        if self.corner_clip.needs_apply(bounds.bottom_corner_radius_px) {
            clip_native_webview_to_bottom_corners(&self.webview, bounds.bottom_corner_radius_px);
        }
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        if !visible {
            let _ = self
                .webview
                .evaluate_script("document.querySelectorAll('video,audio').forEach(m=>m.pause())");
        }
        self.scene_clip.set_visible(visible);
        let _ = self.webview.set_visible(visible);
    }

    pub(crate) fn report_resource_timing(&self, node_id: NodeId) {
        let result = self
            .webview
            .evaluate_script_with_callback(RESOURCE_TIMING_SCRIPT, move |json| {
                log_resource_timing(node_id, &json)
            });
        if let Err(error) = result {
            tracing::debug!(node_id, %error, "browser resource timing script failed");
        }
    }

    pub(crate) fn present(
        &mut self,
        placement: gpui::ScenePresentation,
        interactive: bool,
        window: &Window,
    ) {
        self.scene_clip.present(placement, interactive, window);
    }

    pub(crate) fn release_input(&mut self) {
        self.scene_clip.release_input();
    }

    pub(crate) fn import_cookies(
        &mut self,
        node_id: NodeId,
        source_url: &str,
        cookies: &[BrowserCookie],
        skipped: usize,
    ) -> Result<CookieImportStatus, String> {
        if cookies.is_empty() {
            return Ok(CookieImportStatus::Complete(0));
        }
        let native_cookies = cookies
            .iter()
            .map(wk_cookie_from_browser_cookie)
            .collect::<Result<Vec<_>, _>>()?;
        let native_cookies = NSArray::from_retained_slice(&native_cookies);
        let imported = cookies.len();
        let sender = self.event_sender.clone();
        let url = source_url.to_string();
        let completion = block2::RcBlock::new(move || {
            let _ = sender.send(BrowserEvent::ChromeAuthImported {
                node_id,
                url: url.clone(),
                imported,
                skipped,
            });
        });
        let webview = self.webview.webview();
        // SAFETY: Wry WebViews and their persistent data store are created and accessed on
        // the GPUI main thread. WebKit copies the completion block and invokes it after the
        // whole NSArray has been stored, so this call schedules work without waiting here.
        unsafe {
            webview
                .configuration()
                .websiteDataStore()
                .httpCookieStore()
                .setCookies_completionHandler(&native_cookies, Some(&completion));
        }
        Ok(CookieImportStatus::Pending)
    }
}

fn wk_cookie_from_browser_cookie(
    cookie: &BrowserCookie,
) -> Result<objc2::rc::Retained<NSHTTPCookie>, String> {
    let portable_cookie = wry_cookie_from_browser_cookie(cookie)?;
    let name = NSString::from_str(portable_cookie.name());
    let value = NSString::from_str(portable_cookie.value());
    let normalized_domain = cookie.domain.trim_start_matches('.');
    let native_domain = if cookie.host_only {
        normalized_domain.to_string()
    } else {
        format!(".{normalized_domain}")
    };
    let domain = NSString::from_str(&native_domain);
    let path = NSString::from_str(portable_cookie.path().unwrap_or("/"));
    // SAFETY: Foundation exports these immutable NSHTTPCookie property keys for the
    // lifetime of the process; reading their references does not mutate global state.
    let (name_key, value_key, domain_key, path_key, secure_key, expires_key) = unsafe {
        (
            NSHTTPCookieName,
            NSHTTPCookieValue,
            NSHTTPCookieDomain,
            NSHTTPCookiePath,
            NSHTTPCookieSecure,
            NSHTTPCookieExpires,
        )
    };
    let properties: objc2::rc::Retained<NSMutableDictionary<NSHTTPCookiePropertyKey, AnyObject>> =
        NSMutableDictionary::from_slices(
            &[name_key, value_key, domain_key, path_key],
            &[&name, &value, &domain, &path],
        );
    if portable_cookie.secure() == Some(true) {
        properties.insert(secure_key, ns_string!("TRUE"));
    }
    if portable_cookie.http_only() == Some(true) {
        properties.insert(ns_string!("HttpOnly"), ns_string!("TRUE"));
    }
    if let Some(expires) = portable_cookie.expires_datetime() {
        let expires = NSDate::dateWithTimeIntervalSince1970(expires.unix_timestamp() as f64);
        properties.insert(expires_key, &expires);
    }
    if let Some(same_site) = portable_cookie.same_site() {
        match same_site {
            wry::cookie::SameSite::Lax => {
                properties.insert(ns_string!("SameSite"), ns_string!("lax"));
            }
            wry::cookie::SameSite::Strict => {
                properties.insert(ns_string!("SameSite"), ns_string!("strict"));
            }
            wry::cookie::SameSite::None => {}
        }
    }
    // SAFETY: The dictionary contains Foundation objects for every required
    // NSHTTPCookie property and only documented string/date optional values.
    unsafe { NSHTTPCookie::cookieWithProperties(&properties) }
        .ok_or_else(|| "WKWebView rejected an imported Chrome Cookie".to_string())
}

pub(crate) fn wry_cookie_from_browser_cookie(
    cookie: &BrowserCookie,
) -> Result<wry::cookie::Cookie<'static>, String> {
    let mut builder = wry::cookie::CookieBuilder::new(cookie.name.clone(), cookie.value.clone())
        .path(cookie.path.clone())
        .secure(cookie.secure)
        .http_only(cookie.http_only);
    if !cookie.host_only {
        builder = builder.domain(cookie.domain.clone());
    }
    builder = match cookie.same_site {
        SameSitePolicy::Unspecified => builder,
        SameSitePolicy::NoRestriction => builder.same_site(wry::cookie::SameSite::None),
        SameSitePolicy::Lax => builder.same_site(wry::cookie::SameSite::Lax),
        SameSitePolicy::Strict => builder.same_site(wry::cookie::SameSite::Strict),
    };
    if let Some(expires) = cookie.expires_unix_seconds {
        let expires = wry::cookie::time::OffsetDateTime::from_unix_timestamp(expires)
            .map_err(|_| "Chrome Cookie expiry was outside the WebView time range".to_string())?;
        builder = builder.expires(expires);
    }
    Ok(builder.build())
}

fn forward_new_window_to_current_card(
    sender: &Sender<BrowserEvent>,
    node_id: NodeId,
    url: String,
) -> wry::NewWindowResponse {
    let _ = sender.send(BrowserEvent::NavigateCurrentCard { node_id, url });
    wry::NewWindowResponse::Deny
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

#[cfg(test)]
#[path = "../../../tests/unit/browser_wry_backend_tests.rs"]
mod tests;
