use super::*;

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
