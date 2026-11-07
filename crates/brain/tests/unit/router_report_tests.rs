use super::*;

fn t0() -> Instant {
    Instant::now()
}

/// 岛上的“汇报”按钮：宿主把有界概览交给主会话，要求以 reply 回传。
///
/// 数据构造：ReportRequested{overview:"Alan 完成了登录"}。
/// 预期结果：
///   - 正断言：走主会话；提示包含概览原文与 `--kind reply`；汇报的触发种类可持久化往返
///   - 负断言：汇报不走审批会话；未知持久化键退回 Unknown
#[test]
fn report_requests_ask_the_brain_for_a_reply_report() {
    let now = t0();
    let mut router = Router::new(RouterConfig::default(), now);
    let actions = router.push(
        BrainEventKind::ReportRequested {
            request_id: 7,
            overview: "Alan 完成了登录".into(),
        },
        now,
    );
    assert_eq!(actions.len(), 1, "汇报请求应立即产生一条动作");
    assert_eq!(actions[0].lane, Lane::Main, "走主会话而不是审批会话");
    assert!(
        actions[0].text.contains("Alan 完成了登录"),
        "{}",
        actions[0].text
    );
    assert!(
        actions[0].text.contains("--kind reply"),
        "{}",
        actions[0].text
    );
    assert_eq!(
        actions[0].trigger,
        Trigger::new(TriggerKind::ReportRequest, "")
    );
    assert_eq!(
        TriggerKind::parse(TriggerKind::ReportRequest.as_str()),
        TriggerKind::ReportRequest
    );
    assert_eq!(TriggerKind::parse("future_kind"), TriggerKind::Unknown);
}

/// 场景：项目 Dock 的用户请求与岛内幕僚的用户请求必须分开：只有岛内的请求要求 `--kind reply`，
/// 否则项目 A 的私有对话回复会串进全局岛对话。
/// 预期：UserRequest 提示不含 `--kind reply`；StaffRequest 提示含原文与 `--kind reply`，走主会话。
#[test]
fn only_staff_requests_ask_for_reply_kind_reports() {
    let now = t0();
    let mut router = Router::new(RouterConfig::default(), now);
    let dock = router.push(
        BrainEventKind::UserRequest {
            text: "补文档".into(),
        },
        now,
    );
    assert!(
        !dock[0].text.contains("--kind reply"),
        "Dock 请求不得被引到岛：{}",
        dock[0].text
    );
    let staff = router.push(
        BrainEventKind::StaffRequest {
            text: "谁卡住了".into(),
        },
        now,
    );
    assert_eq!(staff.len(), 1);
    assert_eq!(staff[0].lane, Lane::Main);
    assert!(
        staff[0].text.contains("谁卡住了") && staff[0].text.contains("--kind reply"),
        "{}",
        staff[0].text
    );
    assert_eq!(staff[0].trigger, Trigger::new(TriggerKind::UserRequest, ""));
}
