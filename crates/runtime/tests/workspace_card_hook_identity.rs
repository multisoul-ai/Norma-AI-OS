use canvas::{AgentProfile, CanvasRect};
use norma::agent_hook::{parse_hook_event, HookSource};
use runtime::Runtime;

/// 旧直接 start 不得回退当前 turn，包括同一卡片同 Provider session 被恢复到新 run 的情形。
/// 数据构造：1 张 Claude 录制卡片，turn-a/turn-b 两个真实标识；session 切换再恢复会旋转 run。
/// 执行过程：观察 A/B start，重到 A；新 SessionStart 后恢复原 session，再重到 A 和新 C。
/// 预期结果：重复 A 在 Runtime 改状态前拒绝，latest 始终保留 B；跨 run 不再认领 A，C 正常接受。
#[test]
fn repeated_start_never_rolls_back_trusted_latest_turn_across_run_changes() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        )))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("原生卡片");
    let event = |name: &str, session: &str, turn: Option<&str>| {
        parse_hook_event(
            HookSource::Claude,
            node,
            serde_json::json!({
                "hook_event_name": name, "session_id":session, "prompt_id":turn,
                "prompt":"相同正文"
            }),
        )
        .expect("CLI Hook 载荷")
    };
    assert!(
        runtime.observe_mobile_hook(&event("UserPromptSubmit", "session", Some("turn-a"))),
        "A 首次开始"
    );
    assert!(
        runtime.observe_mobile_hook(&event("UserPromptSubmit", "session", Some("turn-b"))),
        "B 是新直接回合"
    );
    let latest = runtime
        .mobile_latest_turn_key(node)
        .expect("B 当前可信身份");
    assert!(
        !runtime.observe_mobile_hook(&event("UserPromptSubmit", "session", Some("turn-a"))),
        "重复旧 A 在改 Runtime 之前必须拒绝"
    );
    assert_eq!(
        runtime.mobile_latest_turn_key(node),
        Some(latest.clone()),
        "重复 A 不能伪造当前 turn，用于精确取消"
    );
    assert!(
        runtime.observe_mobile_hook(&event("SessionStart", "other-session", None)),
        "切到其他 session"
    );
    assert!(
        runtime.observe_mobile_hook(&event("SessionStart", "session", None)),
        "恢复同一 Provider session 到新 run"
    );
    assert_ne!(
        runtime.mobile_agent_run_id(node),
        Some(latest.run_id.as_str()),
        "SessionStart 旋转 run，不能用当前 run 拼接旧身份绕过去重"
    );
    assert!(
        !runtime.observe_mobile_hook(&event("UserPromptSubmit", "session", Some("turn-a"))),
        "缺少 control_run_id 的旧 A 在新 run 也不可认领"
    );
    assert!(
        runtime.observe_mobile_hook(&event("UserPromptSubmit", "session", Some("turn-c"))),
        "同会话真正新 C 可接受"
    );
    assert_eq!(
        runtime.mobile_latest_turn_key(node).expect("C").turn_id,
        "turn-c",
        "正确的新 turn 更新可信最新身份"
    );
}
