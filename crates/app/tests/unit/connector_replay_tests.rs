use super::test_support::Fixture;
use serde_json::json;

/// 场景：建卡回执完成后 Provider 首次绑定会话，目录 revision 更新而回执被冻结。
/// 构造：临时 Codex 卡片，原回执无会话绑定；使用生产 Runtime Hook 更新 session-a。
/// 执行：建卡 → 会话绑定 → directory → 同键同参重放。
/// 预期：按 AC-76 重放原回执，仅 replayed 改变；当前目录 revision 随真实绑定更新。
#[test]
fn connector_create_replay_revision_after_provider_binding() {
    let mut f = Fixture::new();
    let created = f.create("replay-binding", 0, None);
    let node = f
        .repo
        .signer
        .verify(created["card_id"].as_str().expect("卡片"))
        .expect("身份")
        .node_id;
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            app.runtime
                .apply_agent_task_update(runtime::AgentTaskUpdate {
                    node_id: node,
                    profile: canvas::AgentProfile::Codex,
                    provider_event: "SessionStart".into(),
                    to: runtime::AgentTaskState::Ready,
                    task_title: None,
                    session_id: Some("session-a".into()),
                });
        })
    });
    let directory = f.call("directory", json!({}));
    let current = &directory["workspaces"][0]["cards"][0];
    assert_ne!(
        created["card_revision"], current["card_revision"],
        "持久会话绑定确实更新 revision"
    );
    let replay = f.create("replay-binding", 0, None);
    let mut frozen = created;
    frozen["replayed"] = json!(true);
    assert_eq!(replay, frozen, "AC-76 要求原回执，不能单独刷新 revision");
    assert_ne!(
        replay["card_revision"], current["card_revision"],
        "最新可写目标应查 directory，不改变幂等原回执"
    );
}
