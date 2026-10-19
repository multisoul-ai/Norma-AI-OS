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
