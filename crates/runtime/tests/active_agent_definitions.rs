use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect};
use runtime::Runtime;

fn add_agent(runtime: &mut Runtime, definition_id: Option<i64>, x: f32) {
    runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("Alan".into()),
            definition_id,
            mobile_control: false,
        }),
        CanvasRect::new(x, 0.0, 640.0, 400.0),
    );
}

/// Settings 左栏需要一次拿到画布上所有已绑定 Agent 的 definition id 来分组。
///
/// 数据构造：
///   Agent 卡片 = 3 张：definition_id = 7、9，另有 1 张未绑定（None）。
///   Terminal 不参与；x 坐标 0/660/1320 仅用于避免重叠，640×400 是常规卡片尺寸。
///   期望集合 = {7, 9}，共 2 个，未绑定卡片不计入。
///
/// 执行过程：
///   1. 依次放入三张卡片。
///   2. 调用 `active_agent_definition_ids()`。
///
/// 预期结果：
///   - 正断言：包含 7 与 9，且总数为 2。
///   - 负断言：不包含从未绑定的 8。
#[test]
fn active_agent_definition_ids_lists_only_bound_cards() {
    let mut runtime = Runtime::new();
    add_agent(&mut runtime, Some(7), 0.0);
    add_agent(&mut runtime, Some(9), 660.0);
    add_agent(&mut runtime, None, 1320.0);

    let ids = runtime.active_agent_definition_ids();

    assert!(ids.contains(&7), "已绑定的 7 必须出现在画布 Agent 集合中");
    assert!(ids.contains(&9), "已绑定的 9 必须出现在画布 Agent 集合中");
    assert_eq!(ids.len(), 2, "未绑定卡片不得被计入，应恰好 2 个");
    assert!(!ids.contains(&8), "画布上不存在的 8 不得出现");
}
