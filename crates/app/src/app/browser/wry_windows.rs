//! Native WebView2 browser cards; the same navigation and local-resource protocol as macOS.
use super::{
    chrome_auth::{BrowserCookie, SameSitePolicy},
    load_timeline::BrowserLoadPhase,
    native::CookieImportStatus,
    BrowserEvent, BrowserViewBounds,
};
use canvas::NodeId;
use crossbeam_channel::Sender;
use gpui::Window;
use std::time::Instant;

pub(crate) struct WarmWebView {
    _webview: wry::WebView,
}
impl WarmWebView {
    pub(crate) fn create(window: &mut Window) -> wry::Result<Self> {
        wry::WebViewBuilder::new()
            .with_url("about:blank")
            .with_visible(false)
            .build_as_child(window)
            .map(|_webview| Self { _webview })
    }
}
pub(crate) struct WryBrowserView {
    webview: wry::WebView,
}
impl WryBrowserView {
    pub(crate) fn create(
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
        events: Sender<BrowserEvent>,
    ) -> wry::Result<Self> {
        let title = events.clone();
        let load = events.clone();
        let navigation = events.clone();
        let original = url
            .starts_with("norma-content:")
            .then(|| url.split('#').next().unwrap_or(url).to_string());
        let webview = wry::WebViewBuilder::new()
            .with_url(url)
            .with_bounds(rect(bounds))
            .with_asynchronous_custom_protocol("norma-content".into(), |_, request, responder| {
                std::thread::spawn(move || {
                    responder.respond(crate::preview::local_content::respond(request))
                });
            })
            .with_navigation_handler(move |url| {
                if let Some(original) = &original {
                    if url.split('#').next() != Some(original.as_str())
                        && !url.contains("/__norma_preview__")
                    {
                        let _ = navigation.send(BrowserEvent::PreviewNavigation { node_id, url });
                        return false;
                    }
                }
                true
            })
            .with_document_title_changed_handler(move |value| {
                let _ = title.send(BrowserEvent::TitleChanged {
                    node_id,
                    title: value,
                });
            })
            .with_on_page_load_handler(move |event, url| {
                let phase = match event {
                    wry::PageLoadEvent::Started => BrowserLoadPhase::Started,
                    wry::PageLoadEvent::Finished => BrowserLoadPhase::Finished,
                };
                let _ = load.send(BrowserEvent::PageLoad {
                    node_id,
                    phase,
                    at: Instant::now(),
                });
                let _ = load.send(BrowserEvent::LocationChanged { node_id, url });
            })
            .with_new_window_req_handler(move |url, _| {
                let _ = events.send(BrowserEvent::NavigateCurrentCard { node_id, url });
                wry::NewWindowResponse::Deny
            })
            .build_as_child(window)?;
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
    pub(crate) fn stop_loading(&self) {
        let _ = self.webview.evaluate_script("window.stop()");
    }
    pub(crate) fn set_bounds(&mut self, bounds: BrowserViewBounds) {
        let _ = self.webview.set_bounds(rect(bounds));
    }
    pub(crate) fn set_visible(&mut self, visible: bool) {
        let _ = self.webview.set_visible(visible);
    }
    pub(crate) fn release_input(&mut self) {
        let _ = self.webview.focus_parent();
    }
    pub(crate) fn present(
        &mut self,
        placement: gpui::ScenePresentation,
        interactive: bool,
        _window: &Window,
    ) {
        let bounds = placement.visible_bounds;
        let visible = bounds.size.width > gpui::px(0.)
            && bounds.size.height > gpui::px(0.)
            && placement.opacity > 0.;
        self.set_visible(visible);
        if visible {
            let _ = self.webview.set_bounds(wry::Rect {
                position: wry::dpi::LogicalPosition::new(
                    f64::from(bounds.origin.x),
                    f64::from(bounds.origin.y),
                )
                .into(),
                size: wry::dpi::LogicalSize::new(
                    f64::from(bounds.size.width),
                    f64::from(bounds.size.height),
                )
                .into(),
            });
            let _ = self.webview.zoom(f64::from(placement.scale));
        }
        if !interactive {
            self.release_input();
        }
    }
    pub(crate) fn report_resource_timing(&self, node_id: NodeId) {
        let _ = self.webview.evaluate_script_with_callback(
            super::resource_timing::RESOURCE_TIMING_SCRIPT,
            move |json| super::resource_timing::log_resource_timing(node_id, &json),
        );
    }
    pub(crate) fn import_cookies(
        &mut self,
        _node_id: NodeId,
        _url: &str,
        cookies: &[BrowserCookie],
        _skipped: usize,
    ) -> Result<CookieImportStatus, String> {
        for cookie in cookies {
            self.webview
                .set_cookie(&wry_cookie_from_browser_cookie(cookie)?)
                .map_err(|e| e.to_string())?;
        }
        Ok(CookieImportStatus::Complete(cookies.len()))
    }
}
fn rect(bounds: BrowserViewBounds) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::LogicalPosition::new(bounds.x, bounds.y).into(),
        size: wry::dpi::LogicalSize::new(bounds.width, bounds.height).into(),
    }
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
