use storage::orchestration_task_route_store::{
    NewOrchestrationTaskRoute, OrchestrationLineageRejection, OrchestrationTaskReservation,
    OrchestrationTaskRouteStore,
};

/// 跨 Workspace 同 definition 数字：不得仅凭 definition 数字相同误判为自派发。
///
/// 数据构造（含关键数值的推导过程）：
///   source = workspace-a / definition 11
///   target = workspace-b / definition 11（另一 Workspace 的独立实例）
///   Workspace 对比 = 不同 → 自派发判定条件（同 Workspace 且同 definition）必然失败
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 预留一条 source workspace-a/11 → target workspace-b/11 的路由。
///   2. 校验预留结果不是自派发拒绝。
///
/// 预期结果：
///   - 正断言：预留成功（取得预留权或非 SelfDispatch 拒绝）。
///   - 负断言：不得返回 SelfDispatch 拒绝（身份不变量 I-1 按 Workspace 判定）。
#[test]
fn cross_workspace_same_definition_is_not_self_dispatch() {
    let temp = tempfile::tempdir().expect("测试应创建隔离数据库目录");
    let store = OrchestrationTaskRouteStore::new(temp.path().join("app.sqlite"));
    let route = NewOrchestrationTaskRoute {
        task_ref: "cross-same-def".to_string(),
        operation_id: "operation-cross-same-def".to_string(),
        request_fingerprint: "hash-cross-same-def".to_string(),
        source_workspace_id: "workspace-a".to_string(),
        source_definition_id: 11,
        source_provider: "codex".to_string(),
        source_node_id_at_dispatch: 101,
        target_workspace_id: "workspace-b".to_string(),
        target_definition_id: 11,
        parent_task_ref: None,
        root_task_ref: "cross-same-def".to_string(),
    };
    let result = store
        .reserve(&route)
        .expect("跨 Workspace 路由预留不应抛异常");
    assert!(
        !matches!(
            result,
            OrchestrationTaskReservation::Rejected(OrchestrationLineageRejection::SelfDispatch)
        ),
        "跨 Workspace 同 definition 数字不得误判为自派发，实际：{result:?}"
    );
}
