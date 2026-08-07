use super::*;

/// Browser 当前地址事件：Wry 必须上报主页面加载地址，CEF 必须提供无 Client 的安全轮询。
///
/// 数据构造（含关键数值的推导过程）：
///   backend 数量=2：Wry 事件回调 + CEF main_frame URL 轮询
///   BrowserEvent 地址事件数量=1：LocationChanged
///   CEF Client wrapper 数量=0：继续规避 invalid version 崩溃
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Browser 事件、Wry、CEF、NativeBrowserView 与 Store 源码。
///   2. 检查 Wry page-load handler 把主页面 URL 发为 LocationChanged。
///   3. 检查 CEF 从 main_frame 读取 current_url，且不重新引入 Client wrapper。
///   4. 检查 Store 接受地址变化并更新缓存 URL，供 Canvas 节点持久化。
///
/// 预期结果：
///   - 正断言：两种后端都能提供真实当前 URL。
///   - 正断言：Store 只在 URL 变化时发出规范化 LocationChanged。
///   - 负断言：CEF 不得为地址回调重新使用会导致崩溃的 wrap_client。
#[test]
fn test_browser_backends_report_current_main_frame_location_without_cef_client() {
    let browser = include_str!("../../src/app/browser.rs");
    let store = include_str!("../../src/app/browser/store.rs");
    let native = include_str!("../../src/app/browser/native.rs");
    let wry = include_str!("../../src/app/browser/wry_backend.rs");
    let cef = include_str!("../../src/app/browser/cef_backend.rs");

    assert!(
        browser.contains("LocationChanged"),
        "BrowserEvent 必须包含持久化真实当前地址的 LocationChanged 事件"
    );
    assert!(
        wry.contains("with_on_page_load_handler") && wry.contains("BrowserEvent::LocationChanged"),
        "Wry 主页面加载必须把当前 URL 上报为 LocationChanged"
    );
    assert!(
        cef.contains("fn current_url") && cef.contains("frame.url()"),
        "CEF 必须从 main_frame 读取当前 URL，覆盖站内导航与重定向"
    );
    assert!(
        native.contains("current_url_for_polling"),
        "NativeBrowserView 必须只为需要轮询的后端暴露当前 URL"
    );
    assert!(
        store.contains("accept_location_change")
            && store.contains("view.url = normalized_url.clone()"),
        "BrowserStore 必须更新 native view 缓存 URL，避免下帧把旧地址重新 load 回去"
    );
    assert!(
        !cef.contains("wrap_client!") && !cef.contains("Some(&mut client)"),
        "CEF 地址追踪不得重新引入已知会触发 invalid version 崩溃的 Client wrapper"
    );
}

/// Browser 地址规范化：网页回调的等价 URL 必须折叠为同一个持久化地址。
///
/// 数据构造（含关键数值的推导过程）：
///   输入 1=https://example.com（根路径无斜杠）
///   输入 2=https://example.com/（canonical 根路径）
///   输入 3=custom://host/path（自定义层级 scheme）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别调用 canonical_browser_location。
///   2. URL parser 规范化根路径与 scheme/host。
///   3. 比较两个 HTTP 根地址，并检查自定义 scheme 未被改写为 https。
///
/// 预期结果：
///   - 正断言：两个 HTTP 根地址都得到 https://example.com/。
///   - 正断言：自定义 scheme 保持 custom://。
///   - 负断言：等价根地址不得保留两种字符串表示。
#[test]
fn test_canonical_browser_location_collapses_equivalent_root_urls() {
    let without_slash = canonical_browser_location("https://example.com");
    let with_slash = canonical_browser_location("https://example.com/");
    let custom = canonical_browser_location("custom://host/path");

    assert_eq!(
        without_slash, "https://example.com/",
        "无尾斜杠 HTTP 根地址必须规范化为 canonical URL"
    );
    assert_eq!(
        with_slash, "https://example.com/",
        "已有尾斜杠的 canonical URL 必须保持稳定"
    );
    assert_eq!(
        custom, "custom://host/path",
        "网页上报的自定义层级 scheme 不得被改写为 https"
    );
    assert_eq!(
        without_slash, with_slash,
        "等价 HTTP 根地址不得保留两种持久化字符串"
    );
    assert!(
        !custom.starts_with("https://"),
        "自定义 scheme 的真实当前地址不得被错误补 https"
    );
}
