use objc2::MainThreadMarker;
use objc2_foundation::{NSBundle, NSString};
use objc2_web_kit::WKWebViewConfiguration;
use std::sync::OnceLock;
use wry::WebViewBuilderExtMacos;

static SAFARI_APPLICATION_NAME: OnceLock<Option<String>> = OnceLock::new();

pub(super) fn with_safari_browser_identity<'a>(
    builder: wry::WebViewBuilder<'a>,
) -> wry::WebViewBuilder<'a> {
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
}
