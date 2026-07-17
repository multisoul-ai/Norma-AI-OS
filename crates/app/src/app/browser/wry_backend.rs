//! Wry/WebKit backend for Browser cards.

use super::chrome_auth::{BrowserCookie, SameSitePolicy};
use super::{BrowserEvent, BrowserViewBounds};
use canvas::NodeId;
use crossbeam_channel::Sender;
use gpui::Window;
use objc2::runtime::AnyObject;
use objc2::MainThreadMarker;
use objc2_foundation::{
    ns_string, NSArray, NSBundle, NSDate, NSHTTPCookie, NSHTTPCookieDomain, NSHTTPCookieExpires,
    NSHTTPCookieName, NSHTTPCookiePath, NSHTTPCookiePropertyKey, NSHTTPCookieSecure,
    NSHTTPCookieValue, NSMutableDictionary, NSString,
};
use objc2_web_kit::WKWebViewConfiguration;
use std::sync::OnceLock;
use wry::{WebViewBuilderExtMacos, WebViewExtMacOS};

use super::native::CookieImportStatus;

const BROWSER_PAGE_INTERACTION_MESSAGE: &str = "multisoul:browser-page-interaction";
static SAFARI_APPLICATION_NAME: OnceLock<Option<String>> = OnceLock::new();
const BROWSER_PAGE_INTERACTION_SCRIPT: &str = r#"
(() => {
    const notifyHost = () => window.ipc?.postMessage("multisoul:browser-page-interaction");
    window.addEventListener("pointerdown", notifyHost, true);
    window.addEventListener("focusin", notifyHost, true);
})();
"#;

pub(crate) struct WryBrowserView {
    webview: wry::WebView,
    event_sender: Sender<BrowserEvent>,
}

impl WryBrowserView {
    pub(crate) fn create(
        node_id: NodeId,
        url: &str,
        bounds: BrowserViewBounds,
        window: &mut Window,
        event_sender: Sender<BrowserEvent>,
    ) -> wry::Result<Self> {
        let browser_event_sender = event_sender.clone();
        let title_sender = event_sender.clone();
        let new_window_sender = event_sender.clone();
        let webview = with_safari_browser_identity(wry::WebViewBuilder::new())
            .with_url(url)
            .with_bounds(wry_rect(bounds))
            .with_initialization_script_for_main_only(BROWSER_PAGE_INTERACTION_SCRIPT, false)
            .with_ipc_handler(move |request| {
                if request.body() == BROWSER_PAGE_INTERACTION_MESSAGE {
                    let _ = event_sender.send(BrowserEvent::PageInteraction { node_id });
                }
            })
            .with_document_title_changed_handler(move |title| {
                let _ = title_sender.send(BrowserEvent::TitleChanged { node_id, title });
            })
            .with_new_window_req_handler(move |url, _features| {
                forward_new_window_to_current_card(&new_window_sender, node_id, url)
            })
            .build_as_child(window)?;
        clip_native_webview_to_bottom_corners(&webview, bounds.bottom_corner_radius_px);
        Ok(Self {
            webview,
            event_sender: browser_event_sender,
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

    pub(crate) fn set_bounds(&mut self, bounds: BrowserViewBounds) {
        let _ = self.webview.set_bounds(wry_rect(bounds));
        clip_native_webview_to_bottom_corners(&self.webview, bounds.bottom_corner_radius_px);
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        let _ = self.webview.set_visible(visible);
    }

    pub(crate) fn release_input(&mut self) {
        let _ = self.webview.focus_parent();
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

fn with_safari_browser_identity<'a>(builder: wry::WebViewBuilder<'a>) -> wry::WebViewBuilder<'a> {
    let Some(application_name) = SAFARI_APPLICATION_NAME.get().and_then(Option::as_deref) else {
        return builder;
    };
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("WKWebView configuration must be created on the main thread");
        return builder;
    };

    // SAFETY: WebKit configuration is created on the main thread, proven by `mtm` above.
    let configuration = unsafe { WKWebViewConfiguration::new(mtm) };
    let application_name = NSString::from_str(application_name);
    unsafe {
        configuration.setApplicationNameForUserAgent(Some(&application_name));
    }
    builder.with_webview_configuration(configuration)
}

pub(crate) fn initialize_safari_browser_identity() {
    let application_name = installed_safari_version()
        .as_deref()
        .and_then(safari_application_name_for_user_agent);
    if application_name.is_none() {
        tracing::warn!("Safari version unavailable; using the system WKWebView user agent");
    }
    let _ = SAFARI_APPLICATION_NAME.set(application_name);
}

fn installed_safari_version() -> Option<String> {
    let safari_path = NSString::from_str("/Applications/Safari.app");
    let bundle = NSBundle::bundleWithPath(&safari_path)?;
    let version_key = NSString::from_str("CFBundleShortVersionString");
    bundle
        .objectForInfoDictionaryKey(&version_key)?
        .downcast::<NSString>()
        .ok()
        .map(|version| version.to_string())
}

fn safari_application_name_for_user_agent(version: &str) -> Option<String> {
    let version = version.trim();
    if version.is_empty()
        || !version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    Some(format!("Version/{version} Safari/605.1.15"))
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
mod tests {
    use super::*;

    /// W-1: WKWebView 必须追加与本机 Safari 一致的产品标识，不能继续暴露裸 WebKit UA。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   safari_version = "18.6"，来自当前系统 Safari 的 CFBundleShortVersionString；
    ///   WebKit 产品版本 = 605.1.15，与 macOS WKWebView 默认 UA 的 AppleWebKit 版本一致；
    ///   product token = "Version/18.6 Safari/605.1.15"。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 将系统 Safari 版本交给浏览器身份策略。
    ///   2. 策略生成只追加到 WKWebView 默认 UA 尾部的 Safari 产品标识。
    ///   3. 检查结果没有引入 Chromium 或自动化浏览器标记。
    ///
    /// 预期结果：
    ///   - 正断言：产品标识精确包含本机 Safari 版本和 Safari WebKit token。
    ///   - 负断言：产品标识不包含 Chrome、Headless 或 Wry，避免制造矛盾指纹。
    #[test]
    fn test_safari_application_name_completes_wkwebview_identity() {
        let product = safari_application_name_for_user_agent("18.6")
            .expect("有效的 Safari 版本必须生成 WebView 产品标识");

        assert_eq!(
            product,
            "Version/18.6 Safari/605.1.15",
            "WKWebView 产品标识必须与本机 Safari 版本一致，不能继续发送缺少 Version/Safari 的裸 WebKit UA"
        );
        assert!(
            !product.contains("Chrome"),
            "Safari WebView 身份不能包含 Chrome，否则 User-Agent 与 WebKit 能力会互相矛盾"
        );
        assert!(
            !product.contains("Headless"),
            "Safari WebView 身份不能包含 Headless 自动化标记"
        );
        assert!(
            !product.contains("Wry"),
            "Safari WebView 身份不能暴露 Wry 嵌入层实现名"
        );
    }

    /// W-2: 网页请求新窗口时必须在当前 Browser 卡片中导航，不能静默吞掉点击。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   node_id = 42，代表发起点击的唯一 Browser 卡片；
    ///   target_url = B 站首页 `target="_blank"` 视频链接的真实 URL 形态；
    ///   expected_events = 1，新窗口请求只能转换成一次当前卡片导航。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 模拟 WKWebView 收到 B 站视频卡片的新窗口请求。
    ///   2. 新窗口策略拒绝创建脱离无限画布的原生窗口。
    ///   3. 策略把目标 URL 与原 Browser node_id 投递给 app 事件队列。
    ///
    /// 预期结果：
    ///   - 正断言：队列收到精确的 NavigateCurrentCard 事件和目标 URL。
    ///   - 正断言：原生新窗口仍返回 Deny，避免生成不受画布管理的窗口。
    ///   - 负断言：队列中不应存在第二个事件，避免一次点击重复导航。
    #[test]
    fn test_new_window_request_navigates_current_browser_card() {
        let node_id: NodeId = 42;
        let target_url = "https://www.bilibili.com/video/BV1r6NL6fECw";
        let (sender, receiver) = crossbeam_channel::unbounded();

        let response = forward_new_window_to_current_card(&sender, node_id, target_url.to_string());
        let event = receiver
            .try_recv()
            .expect("B 站视频的新窗口请求必须转换成当前 Browser 卡片导航事件");

        assert!(
            matches!(response, wry::NewWindowResponse::Deny),
            "Browser 卡片必须拒绝创建脱离画布管理的原生新窗口"
        );
        assert_eq!(
            event,
            BrowserEvent::NavigateCurrentCard {
                node_id,
                url: target_url.to_string(),
            },
            "新窗口请求必须携带原 Browser node_id 和完整视频 URL 导航当前卡片"
        );
        assert!(
            receiver.try_recv().is_err(),
            "一次网页点击只能生成一个导航事件，不能重复加载视频页面"
        );
    }
}
