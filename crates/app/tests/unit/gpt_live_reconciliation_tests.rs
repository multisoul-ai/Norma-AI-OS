use super::*;

/// AC07/AC10：只读核对显示不可变原命令与当前画布事实，不能据卡片存在自动消除unknown。
/// 构造：真实SQLite中的未落账创建请求，当前画布确有node=9，但该节点没有匹配回执。
/// 执行：读取生产核对报告，再重新查询持久操作状态。
/// 预期：原命令和node9可见，明确待核对且不会自动重做；读取不修改原回执。
#[test]
fn gpt_live_reconciliation_keeps_unknown_and_displays_original_command_and_canvas() {
    let temp = tempfile::tempdir().expect("隔离DB");
    let store = storage::LiveOperationStore::new(temp.path().join("app.sqlite"));
    store
        .reserve(&storage::LiveOperationRequest {
            project_id: "/project",
            intent_id: "input-1",
            operation_id: "step-1",
            owner_id: "old-process",
            command: &agent::AdapterCommand::CreateBrowser {
                url: Some("https://example.com".into()),
            },
        })
        .expect("真实预留");
    let cards = vec![agent::CanvasCardContext {
        node_id: 9,
        position: 1,
        kind: agent::CanvasCardKind::Browser,
        title: "示例页面".into(),
        selected: false,
        focused: false,
    }];
    let text = load_review(&store, "/project", None, &cards).expect("核对报告");
    assert!(
        text.contains("CreateBrowser") && text.contains("https://example.com"),
        "显示不可变原命令"
    );
    assert!(
        text.contains("node=9") && text.contains("示例页面"),
        "显示当前画布真实卡片"
    );
    assert!(
        text.contains("待核对") && text.contains("不会自动重做"),
        "不虚构普遍exactly-once或自动恢复"
    );
    assert_eq!(
        store.recent("/project", 1).expect("读取不修改")[0].state,
        storage::LiveOperationState::OutcomeUnknown,
        "卡片存在不足以证明此请求已执行"
    );
}
