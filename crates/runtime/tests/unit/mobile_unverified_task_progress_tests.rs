use super::*;
use serde_json::json;

/// 未验证的智能体即使收到合成的完整清单，也不产生待办进度。
///
/// 数据构造：除 Claude 外的 9 个 AgentProfile 各 1 张卡。清单是合成探针，不是捕获到的
/// Hook 样例：2 项，第 1 项 status=completed，第 2 项 status=in_progress，activeForm 为
/// "Checking B"。若被误接入，已完成数/总数应为 1/2，进行中项为 "Checking B"。
/// 执行过程：每张卡先接受 UserPromptSubmit（turn-a），再投递该 profile 候选工具的 PostToolUse。
/// 预期结果：
///   - 正断言：mobile_task_progress 与卡片快照 task_progress 都是 None。
///   - 负断言：不得出现 1/2 或进行中项 "Checking B"。
///   - Codex 与 Cursor 的 Hook 必须被接受，证明断言打在待办观测上，而不是事件被提前丢掉。
#[test]
fn unverified_profiles_do_not_publish_task_progress() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    // 9 = AgentProfile 全集 10 − Claude。Claude 的 TaskCreate/TaskUpdate/TodoWrite 不在本测试改动范围内。
    let cases = [
        (AgentProfile::Codex, HookSource::Codex, "update_plan"),
        (AgentProfile::Cursor, HookSource::Cursor, "TodoWrite"),
        (AgentProfile::OpenCode, HookSource::OpenCode, "todowrite"),
        (AgentProfile::Kimi, HookSource::Kimi, "TodoWrite"),
        (AgentProfile::Pi, HookSource::Pi, "todo"),
        (AgentProfile::Grok, HookSource::Grok, "TodoWrite"),
        (AgentProfile::InfCode, HookSource::InfCode, "todo_write"),
        (AgentProfile::ZCode, HookSource::ZCode, "TodoWrite"),
        (AgentProfile::Hermes, HookSource::Hermes, "todo"),
    ];
    assert_eq!(cases.len(), 9, "未验证 profile 应为 9 个");
    let forged = json!({
        "todos": [
            {"content": "Check A", "status": "completed", "activeForm": "Checking A"},
            {"content": "Check B", "status": "in_progress", "activeForm": "Checking B"}
        ]
    });
    let forged_progress = norma::mobile::TaskProgress {
        completed: 1,
        total: 2,
        current_step: Some("Checking B".into()),
    };
    for (profile, source, tool) in cases {
        let node = runtime
            .spawn_agent(profile, CanvasRect::new(0., 0., 640., 400.), 80, 24)
            .unwrap_or_else(|_| panic!("{profile:?} 原卡应能创建"));
        let submit = other_hook(node, source, "UserPromptSubmit", "turn-a", tool, json!({}));
        assert!(
            runtime.observe_mobile_hook(&submit),
            "{profile:?} 的回合起点应被接受"
        );
        let listed = other_hook(
            node,
            source,
            "PostToolUse",
            "turn-a",
            tool,
            json!({
                "tool_input": forged,
                "tool_response": forged,
            }),
        );
        let accepted = runtime.observe_mobile_hook(&listed);
        if matches!(profile, AgentProfile::Codex | AgentProfile::Cursor) {
            assert!(
                accepted,
                "{profile:?} 的 PostToolUse 应进入待办观测；拒绝意味着本测试没有覆盖解析路径"
            );
        }
        assert!(
            runtime.mobile_task_progress(node).is_none(),
            "{profile:?} 未验证，不得把合成清单显示为待办进度"
        );
        assert_ne!(
            runtime.mobile_task_progress(node),
            Some(forged_progress.clone()),
            "{profile:?} 不得显示合成清单的 1/2 与 Checking B"
        );
        let card = runtime
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == node)
            .unwrap_or_else(|| panic!("{profile:?} 应有卡片快照"));
        assert!(
            card.task_progress.is_none(),
            "{profile:?} 卡片快照 task_progress 也应为空"
        );
        assert_ne!(
            card.task_progress,
            Some(forged_progress.clone()),
            "{profile:?} 卡片快照不得显示合成的 1/2"
        );
    }
}

fn other_hook(
    node: NodeId,
    source: HookSource,
    name: &str,
    turn: &str,
    tool: &str,
    mut raw: Value,
) -> norma::agent_hook::AgentHookEvent {
    raw["hook_event_name"] = json!(name);
    raw["session_id"] = json!("session-a");
    raw["conversation_id"] = json!("session-a");
    raw["turn_id"] = json!(turn);
    raw["turnId"] = json!(turn);
    raw["prompt_id"] = json!(turn);
    raw["generation_id"] = json!(turn);
    raw["tool_name"] = json!(tool);
    raw["toolName"] = json!(tool);
    parse_hook_event(source, node, raw).unwrap_or_else(|_| panic!("{name} 应能解析"))
}
