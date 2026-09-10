use super::*;
use norma::agent_hook::{parse_hook_event, HookSource};

/// Hook mailbox 隔离：每个 token 只能把事件送入自己的 Workspace 邮箱。
///
/// 数据构造（含关键数值的推导过程）：
///   registrations = alpha(1) + beta(1) = 2 个 token
///   events        = alpha/node 7(1) + beta/node 7(1) = 2 条
///   node collision = 两边 node_id 都为 7，刻意验证 token 隔离
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注册 alpha 与 beta 两个独立邮箱。
///   2. 使用各自 token 投递同 node_id 的不同 Provider 事件。
///   3. 分别从两个 receiver 读取结果。
///
/// 预期结果：
///   - 正断言：alpha 只收到 Codex 事件，beta 只收到 Claude 事件。
///   - 负断言：任一邮箱都不得收到第二条串线事件。
#[test]
fn hook_tokens_route_same_node_id_to_distinct_workspace_mailboxes() {
    let mut registry = AgentHookRegistry::default();
    let (alpha_token, alpha_receiver, _, _) = registry.register().expect("应可注册安全 token");
    let (beta_token, beta_receiver, _, _) = registry.register().expect("应可注册安全 token");
    let alpha_event = parse_hook_event(
        HookSource::Codex,
        7,
        serde_json::json!({"hook_event_name": "Stop"}),
    )
    .expect("alpha hook event 应可构造");
    let beta_event = parse_hook_event(
        HookSource::Claude,
        7,
        serde_json::json!({"hook_event_name": "Stop"}),
    )
    .expect("beta hook event 应可构造");

    let alpha_delivery = registry.deliver(&alpha_token, alpha_event);
    let beta_delivery = registry.deliver(&beta_token, beta_event);
    let alpha_received = alpha_receiver
        .try_recv()
        .expect("alpha 邮箱应收到自己的事件");
    let beta_received = beta_receiver.try_recv().expect("beta 邮箱应收到自己的事件");

    assert_eq!(
        alpha_delivery,
        HookDelivery::Accepted,
        "alpha token 的事件应被明确接受"
    );
    assert_eq!(
        beta_delivery,
        HookDelivery::Accepted,
        "beta token 的事件应被明确接受"
    );
    assert_eq!(
        alpha_received.profile,
        norma::protocol::AgentProfile::Codex,
        "alpha 邮箱只能收到 Codex 事件"
    );
    assert_eq!(
        beta_received.profile,
        norma::protocol::AgentProfile::Claude,
        "beta 邮箱只能收到 Claude 事件"
    );
    assert!(
        alpha_receiver.try_recv().is_err(),
        "alpha 邮箱不得收到 beta 的第二条事件"
    );
    assert!(
        beta_receiver.try_recv().is_err(),
        "beta 邮箱不得收到 alpha 的第二条事件"
    );
}

/// Hook 晚到事件：Workspace 注销后必须返回终态失败，不能虚假接受。
///
/// 数据构造（含关键数值的推导过程）：
///   registrations = 1 个 token
///   unregisters   = 1 次
///   late events   = 1 条 Stop 事件
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注册邮箱并立即注销 token。
///   2. 使用相同 token 投递一条晚到事件。
///   3. 检查 receiver 与投递状态。
///
/// 预期结果：
///   - 正断言：注销成功且晚到事件返回 Retired（HTTP 层映射 410）。
///   - 负断言：receiver 不得收到事件，系统不得返回 Accepted。
#[test]
fn retired_hook_token_rejects_late_event_without_false_acceptance() {
    let mut registry = AgentHookRegistry::default();
    let (token, receiver, _, _) = registry.register().expect("应可注册安全 token");
    let removed = registry.unregister(&token);
    let event = parse_hook_event(
        HookSource::Codex,
        9,
        serde_json::json!({"hook_event_name": "Stop"}),
    )
    .expect("晚到 hook event 应可构造");

    let delivery = registry.deliver(&token, event);

    assert!(removed, "活跃 Hook token 应成功注销");
    assert_eq!(
        delivery,
        HookDelivery::Retired,
        "已注销 token 的晚到事件必须返回 Retired"
    );
    assert_ne!(
        delivery,
        HookDelivery::Accepted,
        "已关闭 Workspace 的晚到事件不得虚假返回 Accepted"
    );
    assert!(
        receiver.try_recv().is_err(),
        "已注销 Workspace 的邮箱不得收到晚到事件"
    );
}
