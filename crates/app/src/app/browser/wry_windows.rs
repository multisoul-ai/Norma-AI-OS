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
#[path = "wry_windows/deferred.rs"]
mod deferred;
use deferred::DeferredWebView;

pub(crate) struct WarmWebView {
    _webview: DeferredWebView,
}
impl WarmWebView {
    pub(crate) fn create(window: &mut Window, cx: &gpui::App) -> wry::Result<Self> {
        DeferredWebView::create(window, cx, None, None, |_| {
            wry::WebViewBuilder::new()
                .with_url("about:blank")
                .with_visible(false)
        })
        .map(|_webview| Self { _webview })
    }
}
pub(crate) struct WryBrowserView {
    webview: DeferredWebView,
}
impl WryBrowserView {
    pub(crate) fn create(
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
        events: Sender<BrowserEvent>,
        cx: &gpui::App,
    ) -> wry::Result<Self> {
        let original = url
            .starts_with("norma-content:")
            .then(|| url.split('#').next().unwrap_or(url).to_string());
        let url = url.to_owned();
        let webview =
            DeferredWebView::create(window, cx, Some(node_id), Some(events), move |events| {
                let title = events.clone();
                let load = events.clone();
                let navigation = events.clone();
                wry::WebViewBuilder::new()
                    .with_url(url)
                    .with_visible(false)
                    .with_bounds(rect(bounds))
                    .with_asynchronous_custom_protocol(
                        "norma-content".into(),
                        |_, request, responder| {
                            std::thread::spawn(move || {
                                responder.respond(crate::preview::local_content::respond(request))
                            });
                        },
                    )
                    .with_navigation_handler(move |url| {
                        if let Some(original) = &original {
                            if url.split('#').next() != Some(original.as_str())
                                && !url.contains("/__norma_preview__")
                            {
                                navigation.send(BrowserEvent::PreviewNavigation { node_id, url });
                                return false;
                            }
                        }
                        true
                    })
                    .with_document_title_changed_handler(move |value| {
                        title.send(BrowserEvent::TitleChanged {
                            node_id,
                            title: value,
                        });
                    })
                    .with_on_page_load_handler(move |event, url| {
                        let phase = match event {
                            wry::PageLoadEvent::Started => BrowserLoadPhase::Started,
                            wry::PageLoadEvent::Finished => BrowserLoadPhase::Finished,
                        };
                        load.send(BrowserEvent::PageLoad {
                            node_id,
                            phase,
                            at: Instant::now(),
                        });
                        load.send(BrowserEvent::LocationChanged { node_id, url });
                    })
                    .with_new_window_req_handler(move |url, _| {
                        events.send(BrowserEvent::NavigateCurrentCard { node_id, url });
                        wry::NewWindowResponse::Deny
                    })
            })?;
        Ok(Self { webview })
    }
    pub(crate) fn load_url(&mut self, url: &str) {
        let url = url.to_owned();
        self.webview.run(move |view| {
            let _ = view.load_url(&url);
        });
    }
    pub(crate) fn go_back(&mut self) {
        self.webview.run(|view| {
            let _ = view.evaluate_script("history.back()");
        });
    }
    pub(crate) fn go_forward(&mut self) {
        self.webview.run(|view| {
            let _ = view.evaluate_script("history.forward()");
        });
    }
    pub(crate) fn reload(&mut self) {
        self.webview.run(|view| {
            let _ = view.reload();
        });
    }
    pub(crate) fn stop_loading(&self) {
        self.webview.run(|view| {
            let _ = view.evaluate_script("window.stop()");
        });
    }
    pub(crate) fn set_bounds(&mut self, bounds: BrowserViewBounds) {
        self.webview.latest("bounds", move |view| {
            let _ = view.set_bounds(rect(bounds));
        });
    }
    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.webview.latest("visible", move |view| {
            let _ = view.set_visible(visible);
        });
    }
    pub(crate) fn release_input(&mut self) {
        self.webview.latest("focus-parent", |view| {
            let _ = view.focus_parent();
        });
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
        self.webview.latest("presentation", move |view| {
            let _ = view.set_visible(visible);
            if visible {
                let _ = view.set_bounds(wry::Rect {
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
                let _ = view.zoom(f64::from(placement.scale));
            }
            if !interactive {
                let _ = view.focus_parent();
            }
        });
    }

    pub(crate) fn report_resource_timing(&self, node_id: NodeId) {
        self.webview.run(move |view| {
            let _ = view.evaluate_script_with_callback(
                super::resource_timing::RESOURCE_TIMING_SCRIPT,
                move |json| super::resource_timing::log_resource_timing(node_id, &json),
            );
        });
    }
    pub(crate) fn import_cookies(
        &mut self,
        node_id: NodeId,
        url: &str,
        cookies: &[BrowserCookie],
        skipped: usize,
    ) -> Result<CookieImportStatus, String> {
        let cookies = cookies
            .iter()
            .map(wry_cookie_from_browser_cookie)
            .collect::<Result<Vec<_>, _>>()?;
        let url = url.to_owned();
        let events = self.webview.events.clone();
        self.webview.outcome(move |result| {
            let result = result.map_err(str::to_owned).and_then(|view| {
                for cookie in &cookies {
                    view.set_cookie(cookie).map_err(|error| error.to_string())?;
                }
                Ok(cookies.len())
            });
            match result {
                Ok(imported) => events.send(BrowserEvent::ChromeAuthImported {
                    node_id,
                    url,
                    imported,
                    skipped,
                }),
                Err(message) => events.send(BrowserEvent::ChromeAuthFailed { node_id, message }),
            }
        });
        Ok(CookieImportStatus::Pending)
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
