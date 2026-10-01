//! Headless GPUI E2E：关闭快速 Agent Card 后，即使残留 conversation tail，内置名称也必须归还。

use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use storage::{StoredAgentConversationTail, StoredConversationAvailability};

use crate::harness::{echo_factory, enter_canvas, make_app, project_store, read_runtime};

/// E2E-AGENT-LIFECYCLE-03：关闭带残留 tail 的快速 Card 后再次 Cmd+3，仍得到同一个内置名称。
///
/// 数据构造（含关键数值的推导过程）：
///   Cmd+3 第一次创建 = 1 条 QuickCard Definition（自动名称 = 名称池第 1 项 Alan）
///   残留 tail        = 1 条，definition_id = 该 Definition，node_id = 9999（与真实卡片 id 不一致，
///                      模拟按 node_id 清理永远删不到的残留，复现历史外键失败）
///   关闭后 Definition = 1 - 1 = 0；第二次 Cmd+3 后 = 0 + 1 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入空 workspace，Cmd+3 创建第一张 Codex 快速 Card。
///   2. 通过公共 store 写入引用该 Definition 的残留 tail。
///   3. Cmd+D 关闭 Card，走真实的 Card 关闭与 QuickCard 清理链路。
///   4. 再次 Cmd+3，读取 Definition 与 Card。
///
/// 预期结果：
///   - 正断言：关闭后 Definition 与 tail 均为 0 条。
///   - 正断言：第二次创建的名称与第一次相同（内置名称被归还）。
///   - 负断言：第二次名称不得是 AgentA 等后备名称。
#[test]
fn closing_quick_card_with_lingering_tail_returns_builtin_name() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("名称归还 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let project = project_store(&app, cx, workspace.path());
    let store = project.agent_definition_store();

    cx.simulate_keystrokes("cmd-3");
    cx.run_until_parked();
    let first = store.list().expect("第一次创建后应读取 Definition");
    assert_eq!(first.len(), 1, "Cmd+3 后应恰好 1 条 QuickCard Definition");
    let first_name = first[0].name.clone();

    let tails = project.agent_conversation_tail_store();
    tails
        .upsert(&StoredAgentConversationTail {
            definition_id: first[0].id,
            node_id: 9999,
            session_id: "s1".to_string(),
            agent_name: first_name.clone(),
            profile: "codex".to_string(),
            last_user_message: None,
            last_agent_message: None,
            updated_at_unix: 1_700_000_000,
            availability: StoredConversationAvailability::Available,
            user_message_truncated: false,
            agent_message_truncated: false,
        })
        .expect("应写入残留 tail");

    cx.simulate_keystrokes("cmd-d");
    cx.run_until_parked();
    let after_close = store.list().expect("关闭后应读取 Definition");
    let tails_after_close = tails.list().expect("关闭后应读取 tail");

    cx.simulate_keystrokes("cmd-3");
    cx.run_until_parked();
    let second = store.list().expect("第二次创建后应读取 Definition");
    let cards = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });

    assert!(
        after_close.is_empty(),
        "关闭快速 Card 后 QuickCard Definition 必须被清理，不得被残留 tail 阻塞"
    );
    assert!(
        tails_after_close.is_empty(),
        "引用被清理 Definition 的 tail 必须一并删除"
    );
    assert_eq!(cards, 1, "第二次 Cmd+3 后应恰好 1 张 Agent Card");
    assert_eq!(second.len(), 1, "第二次创建后应恰好 1 条 Definition");
    assert_eq!(
        second[0].name, first_name,
        "内置名称应被归还并再次分配给新 Card"
    );
    assert_ne!(
        second[0].name, "AgentA",
        "不得在内置名称未用完时出现后备名称"
    );
}
