use super::*;
use norma::agent_hook::{parse_hook_event, HookSource};
use serde_json::json;

/// Cursor Hook必须把会话、generation和transcript引用绑定到同一张原生卡，供iOS回执与历史读取。
///
/// 数据构造：1张Cursor原生卡；conversation_id=session-a、generation_id=turn-a、路径=/tmp/session-a.jsonl。
/// 执行过程：启动卡片 → 接收beforeSubmitPrompt → 读取当前turn和待持久化原生事件。
/// 预期结果：运行身份为Cursor；turn-a成为准确回合；transcript路径原样保留；不能误标为Claude。
#[test]
fn cursor_native_hook_binds_turn_and_transcript_for_mobile_history() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Cursor,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原生Cursor卡");
    let event = parse_hook_event(
        HookSource::Cursor,
        node,
        json!({
            "hook_event_name":"beforeSubmitPrompt",
            "conversation_id":"session-a",
            "generation_id":"turn-a",
            "transcript_path":"/tmp/session-a.jsonl",
            "prompt":"从iPhone继续"
        }),
    )
    .expect("Cursor Hook");

    assert!(
        runtime.observe_mobile_hook(&event),
        "Cursor Hook必须由原卡接收"
    );
    let key = runtime
        .mobile_latest_turn_key(node)
        .expect("准确Cursor回合");
    assert_eq!(key.profile, "Cursor", "回合不能冒充Claude或Codex");
    assert_eq!(key.session_id, "session-a", "会话必须来自conversation_id");
    assert_eq!(key.turn_id, "turn-a", "回合必须来自generation_id");
    let (session, transcript, outcome) = runtime
        .native_mobile_event(&event)
        .expect("Cursor原生事件可持久化");
    assert_eq!(session.key.profile, "Cursor", "持久化身份必须保留Provider");
    assert_eq!(
        transcript.as_deref(),
        Some(std::path::Path::new("/tmp/session-a.jsonl")),
        "Hook提供的本地历史引用不能丢失"
    );
    assert!(outcome.is_none(), "提交事件不能伪造完成结果");
}
