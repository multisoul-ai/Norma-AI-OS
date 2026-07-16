use super::*;
use crate::canvas::master_layout::MasterLayoutState;
use canvas::{AgentProfile, AgentSpec};

/// Norma Agent 生命周期：删除 Agent Card 时执行结果必须携带被绑定的 Definition id。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent Card = 1 张：node_id 由空 document 从 1 开始分配
///   definition_id = 41：模拟快速入口持久化的 Definition
///   删除后节点数 = 1 - 1 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 Runtime document 添加绑定 definition_id=41 的 Codex Agent Card。
///   2. 通过 Norma executor 执行 `Delete { node_id }`。
///   3. executor 在删除前读取 AgentSpec，并把 Definition id 带入执行副作用。
///
/// 预期结果：
///   - 正断言：删除返回目标 node_id，并携带 definition_id=41 供 App 清理 QuickCard Definition。
///   - 正断言：删除后 document 为空。
///   - 负断言：Definition id 不得丢失为 None，否则语音/Socket 删除会泄漏临时 Definition。
#[test]
fn test_norma_delete_agent_reports_removed_definition_id() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("Alan".to_string()),
            definition_id: Some(41),
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 360.0),
    );

    let execution = execute_card_command_for_area(
        &mut runtime,
        &mut motion,
        &mut master_layout,
        CardCommand::Delete { node_id },
        CanvasRect::new(0.0, 0.0, 800.0, 600.0),
        1.0,
    )
    .expect("Norma 应成功删除存在的 Agent Card");

    assert_eq!(
        execution.result,
        CardCommandResult::Deleted { node_id },
        "Norma 删除结果必须指向刚删除的 Agent Card"
    );
    assert_eq!(
        execution.removed_agent_definition_id,
        Some(41),
        "删除 Agent Card 时必须保留 definition_id=41 供上层清理临时 Definition"
    );
    assert!(
        runtime.document.nodes().is_empty(),
        "删除唯一 Agent Card 后 document 应为空"
    );
    assert_ne!(
        execution.removed_agent_definition_id, None,
        "语音或 Socket 删除不得丢失 Agent Definition 生命周期信息"
    );
}
