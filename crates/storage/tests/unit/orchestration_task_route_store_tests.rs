use super::{
    NewOrchestrationTaskRoute, OrchestrationLineageRejection,
    OrchestrationTaskOperationReservation, OrchestrationTaskReservation,
    OrchestrationTaskRouteStore,
};

/// 全局任务幂等：相同 operation/参数只占一个 task ref，不同参数明确冲突。
///
/// 数据构造（含关键数值的推导过程）：
///   operation IDs = 1 个固定 operation-7
///   fingerprints  = same(hash-a) × 2 + conflict(hash-b) × 1 = 3 次预留
///   expected rows = 1 行（UNIQUE operation_id）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 operation-7/hash-a 预留 global-task-7。
///   2. 完成记录，写入目标本地 task ref 与序列化结果。
///   3. 用相同 operation/hash-a 重放，再用 operation/hash-b 尝试冲突提交。
///   4. 重新构造 Store，按 global task ref 验证持久结果。
///
/// 预期结果：
///   - 正断言：首次为 New，相同参数为 Replay，重启后仍可读取目标路由。
///   - 正断言：不同参数为 Conflict，不产生第二个 global task ref。
///   - 负断言：重放结果不得丢失 target task ref 或退回 pending。
#[test]
fn operation_id_reservation_is_idempotent_persistent_and_conflict_safe() {
    let temp = tempfile::tempdir().expect("测试应创建隔离数据库目录");
    let database_path = temp.path().join("app.sqlite");
    let store = OrchestrationTaskRouteStore::new(database_path.clone());
    let route = NewOrchestrationTaskRoute {
        task_ref: "global-task-7".to_string(),
        operation_id: "operation-7".to_string(),
        request_fingerprint: "hash-a".to_string(),
        source_workspace_id: "workspace-a".to_string(),
        source_definition_id: 11,
        source_provider: "codex".to_string(),
        source_node_id_at_dispatch: 101,
        target_workspace_id: "workspace-b".to_string(),
        target_definition_id: 22,
        parent_task_ref: None,
        root_task_ref: "global-task-7".to_string(),
    };

    let first = store.reserve(&route).expect("首次 operation 应成功预留");
    store
        .complete(
            "operation-7",
            "runtime-b:3",
            22,
            "Bella",
            202,
            "queued",
            "managed",
            r#"{"task_id":"global-task-7"}"#,
        )
        .expect("首次派发结果应持久化");
    let replay = store.reserve(&route).expect("相同参数重放应可读取已有记录");
    let conflicting = NewOrchestrationTaskRoute {
        task_ref: "global-task-8".to_string(),
        request_fingerprint: "hash-b".to_string(),
        ..route.clone()
    };
    let conflict = store
        .reserve(&conflicting)
        .expect("幂等冲突应作为领域结果返回而非 SQLite 异常");
    let reopened = OrchestrationTaskRouteStore::new(database_path);
    let persisted = reopened
        .get("global-task-7")
        .expect("重启后查询不应失败")
        .expect("重启后全局 task ref 应仍存在");

    assert!(
        matches!(first, OrchestrationTaskReservation::New(_)),
        "首次 operation ID 必须创建新任务预留"
    );
    assert!(
        matches!(replay, OrchestrationTaskReservation::Replay(_)),
        "相同 operation ID 与参数必须返回第一次记录"
    );
    assert!(
        matches!(conflict, OrchestrationTaskReservation::Conflict),
        "相同 operation ID 配不同参数必须返回 Conflict"
    );
    assert_eq!(
        persisted.target_task_ref.as_deref(),
        Some("runtime-b:3"),
        "重启后必须保留目标 Workspace 的本地 task ref"
    );
    assert_eq!(
        persisted.result_json.as_deref(),
        Some(r#"{"task_id":"global-task-7"}"#),
        "重放必须使用首次持久化结果，不能重新派发"
    );
    assert_ne!(
        persisted.task_ref, "global-task-8",
        "冲突提交不得覆盖首次 global task ref"
    );
}

/// 跨 Workspace lineage：目标本地 task ref 必须反查全局父任务并持久化同一根。
///
/// 数据构造（含关键数值的推导过程）：
///   global tasks = root(1) + child(1) = 2 条
///   Workspace hops = A→B(1) + B→C(1) = 2 跳
///   root refs = 1 个 global-root，child parent refs = 1 个 global-root
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 预留 A→B 根任务，并绑定 B 的本地 task ref `runtime-b:3`。
///   2. 用 B + `runtime-b:3` 反查来源 Agent 当前全局任务。
///   3. 预留 B→C 子任务，写入 parent=global-root、root=global-root。
///   4. 重新读取子任务并检查 lineage。
///
/// 预期结果：
///   - 正断言：本地 task ref 只命中 B 中的 global-root。
///   - 正断言：child 保留父任务与根任务，供循环和后代预算校验。
///   - 负断言：child 不得把自己重置为新 root，也不得跨 Workspace 误命中。
#[test]
fn target_runtime_task_lookup_preserves_cross_workspace_parent_and_root() {
    let temp = tempfile::tempdir().expect("测试应创建隔离数据库目录");
    let database_path = temp.path().join("app.sqlite");
    let store = OrchestrationTaskRouteStore::new(database_path);
    let root = NewOrchestrationTaskRoute {
        task_ref: "global-root".to_string(),
        operation_id: "operation-root".to_string(),
        request_fingerprint: "hash-root".to_string(),
        source_workspace_id: "workspace-a".to_string(),
        source_definition_id: 11,
        source_provider: "codex".to_string(),
        source_node_id_at_dispatch: 101,
        target_workspace_id: "workspace-b".to_string(),
        target_definition_id: 22,
        parent_task_ref: None,
        root_task_ref: "global-root".to_string(),
    };
    store.reserve(&root).expect("根任务预留应成功");
    store
        .complete(
            "operation-root",
            "runtime-b:3",
            22,
            "Bella",
            202,
            "submitted",
            "managed",
            r#"{"task_id":"global-root"}"#,
        )
        .expect("根任务目标路由应完成绑定");

    let parent = store
        .find_by_target_task("workspace-b", "runtime-b:3")
        .expect("按目标运行时任务反查不应失败")
        .expect("B 中正在运行的本地 task ref 应命中全局父任务");
    let child = NewOrchestrationTaskRoute {
        task_ref: "global-child".to_string(),
        operation_id: "operation-child".to_string(),
        request_fingerprint: "hash-child".to_string(),
        source_workspace_id: "workspace-b".to_string(),
        source_definition_id: 22,
        source_provider: "codex".to_string(),
        source_node_id_at_dispatch: 202,
        target_workspace_id: "workspace-c".to_string(),
        target_definition_id: 33,
        parent_task_ref: Some(parent.task_ref.clone()),
        root_task_ref: parent.root_task_ref.clone(),
    };
    store.reserve(&child).expect("子任务预留应成功");
    let persisted = store
        .get("global-child")
        .expect("子任务查询不应失败")
        .expect("子任务预留后必须存在");
    let wrong_workspace = store
        .find_by_target_task("workspace-a", "runtime-b:3")
        .expect("错误 Workspace 的反查也应确定性完成");

    assert_eq!(
        parent.task_ref, "global-root",
        "B 的本地 task ref 必须反查到原 global-root"
    );
    assert_eq!(
        persisted.parent_task_ref.as_deref(),
        Some("global-root"),
        "跨 Workspace child 必须持久化全局父任务"
    );
    assert_eq!(
        persisted.root_task_ref, "global-root",
        "跨 Workspace child 必须继承祖先 root，不能重置任务树"
    );
    assert_ne!(
        persisted.root_task_ref, persisted.task_ref,
        "有父任务的 child 不得错误地把自己设成新 root"
    );
    assert!(
        wrong_workspace.is_none(),
        "相同本地 task ref 不得跨 Workspace 误命中其他来源"
    );
}

/// 全局 lineage 门禁：自派发、祖先循环和第 33 个未完成后代必须在预留阶段拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   root = A11→B22（1 条）
///   safe descendants = 32 条（预算上限 32）
///   overflow attempt = 32 + 1 = 第 33 条
///   cycle attempt = B22→A11（目标回到祖先来源）
///   self attempt = B22→B22（来源与目标同一主体）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 预留根任务，建立 A11、B22 的祖先主体。
///   2. 尝试 B22→B22 与 B22→A11，分别触发自派发和祖先循环。
///   3. 以 root 为父任务预留 32 个不同目标的安全后代。
///   4. 尝试第 33 个未完成后代。
///
/// 预期结果：
///   - 正断言：自派发、循环、超预算分别返回稳定领域拒绝。
///   - 正断言：前 32 个安全后代都能预留。
///   - 负断言：三种拒绝不得插入任务行或退化为 SQLite 异常。
#[test]
fn lineage_reservation_rejects_self_cycle_and_descendant_budget_before_side_effects() {
    let temp = tempfile::tempdir().expect("测试应创建隔离数据库目录");
    let store = OrchestrationTaskRouteStore::new(temp.path().join("app.sqlite"));
    let root = NewOrchestrationTaskRoute {
        task_ref: "root-a-b".to_string(),
        operation_id: "operation-root-a-b".to_string(),
        request_fingerprint: "hash-root-a-b".to_string(),
        source_workspace_id: "workspace-a".to_string(),
        source_definition_id: 11,
        source_provider: "codex".to_string(),
        source_node_id_at_dispatch: 101,
        target_workspace_id: "workspace-b".to_string(),
        target_definition_id: 22,
        parent_task_ref: None,
        root_task_ref: "root-a-b".to_string(),
    };
    store.reserve(&root).expect("根任务预留应成功");

    let self_dispatch = NewOrchestrationTaskRoute {
        task_ref: "self-b-b".to_string(),
        operation_id: "operation-self-b-b".to_string(),
        request_fingerprint: "hash-self-b-b".to_string(),
        source_workspace_id: "workspace-b".to_string(),
        source_definition_id: 22,
        source_provider: "codex".to_string(),
        source_node_id_at_dispatch: 202,
        target_workspace_id: "workspace-b".to_string(),
        target_definition_id: 22,
        parent_task_ref: Some("root-a-b".to_string()),
        root_task_ref: "root-a-b".to_string(),
    };
    let cycle = NewOrchestrationTaskRoute {
        task_ref: "cycle-b-a".to_string(),
        operation_id: "operation-cycle-b-a".to_string(),
        request_fingerprint: "hash-cycle-b-a".to_string(),
        source_workspace_id: "workspace-b".to_string(),
        source_definition_id: 22,
        source_provider: "codex".to_string(),
        source_node_id_at_dispatch: 202,
        target_workspace_id: "workspace-a".to_string(),
        target_definition_id: 11,
        parent_task_ref: Some("root-a-b".to_string()),
        root_task_ref: "root-a-b".to_string(),
    };

    let self_result = store
        .reserve(&self_dispatch)
        .expect("自派发应返回领域拒绝而非存储异常");
    let cycle_result = store
        .reserve(&cycle)
        .expect("祖先循环应返回领域拒绝而非存储异常");
    for index in 0..32 {
        let descendant = NewOrchestrationTaskRoute {
            task_ref: format!("descendant-{index}"),
            operation_id: format!("operation-descendant-{index}"),
            request_fingerprint: format!("hash-descendant-{index}"),
            source_workspace_id: "workspace-b".to_string(),
            source_definition_id: 22,
            source_provider: "codex".to_string(),
            source_node_id_at_dispatch: 202,
            target_workspace_id: "workspace-c".to_string(),
            target_definition_id: 100 + i64::from(index),
            parent_task_ref: Some("root-a-b".to_string()),
            root_task_ref: "root-a-b".to_string(),
        };
        assert!(
            matches!(
                store.reserve(&descendant).expect("预算内后代预留不应失败"),
                OrchestrationTaskReservation::New(_)
            ),
            "第 {index} 个预算内后代必须成功预留"
        );
    }
    let overflow = NewOrchestrationTaskRoute {
        task_ref: "descendant-overflow".to_string(),
        operation_id: "operation-descendant-overflow".to_string(),
        request_fingerprint: "hash-descendant-overflow".to_string(),
        source_workspace_id: "workspace-b".to_string(),
        source_definition_id: 22,
        source_provider: "codex".to_string(),
        source_node_id_at_dispatch: 202,
        target_workspace_id: "workspace-c".to_string(),
        target_definition_id: 999,
        parent_task_ref: Some("root-a-b".to_string()),
        root_task_ref: "root-a-b".to_string(),
    };
    let overflow_result = store
        .reserve(&overflow)
        .expect("超预算应返回领域拒绝而非存储异常");

    assert_eq!(
        self_result,
        OrchestrationTaskReservation::Rejected(OrchestrationLineageRejection::SelfDispatch),
        "同一 Workspace/definition 的自派发必须在任务插入前拒绝"
    );
    assert_eq!(
        cycle_result,
        OrchestrationTaskReservation::Rejected(OrchestrationLineageRejection::Cycle),
        "目标回到祖先来源主体时必须返回 delegation cycle"
    );
    assert_eq!(
        overflow_result,
        OrchestrationTaskReservation::Rejected(
            OrchestrationLineageRejection::DescendantBudgetExceeded { limit: 32 },
        ),
        "第 33 个未完成后代必须被全局预算拒绝"
    );
    assert!(
        store
            .get("self-b-b")
            .expect("拒绝后的查询不应失败")
            .is_none(),
        "自派发被拒绝后不得留下 accepted 任务行"
    );
    assert!(
        store
            .get("cycle-b-a")
            .expect("拒绝后的循环查询不应失败")
            .is_none(),
        "循环被拒绝后不得留下 accepted 任务行"
    );
    assert!(
        store
            .get("descendant-overflow")
            .expect("拒绝后的超预算查询不应失败")
            .is_none(),
        "超预算被拒绝后不得留下 accepted 任务行"
    );
}

/// 取消幂等账本：同一 operation/fingerprint 只执行一次，不同参数冲突并可重放首次结果。
///
/// 数据构造（含关键数值的推导过程）：
///   operation IDs = 1 个 cancel-operation-7
///   fingerprints = same(cancel-task-7) × 2 + conflict(cancel-task-8) × 1 = 3 次预留
///   persisted results = 1 个 cancelled JSON
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 首次预留 cancel-operation-7/cancel-task-7。
///   2. 持久化第一次取消响应。
///   3. 用相同 fingerprint 重放，再用不同 fingerprint 尝试冲突。
///   4. 检查 replay 返回首次结果且没有第二次副作用身份。
///
/// 预期结果：
///   - 正断言：首次为 New、同参数为 Replay、不同参数为 Conflict。
///   - 正断言：Replay 保留首次 cancelled JSON。
///   - 负断言：冲突不得覆盖首次结果或伪装成新的取消操作。
#[test]
fn task_control_operation_reservation_replays_cancel_and_rejects_conflicts() {
    let temp = tempfile::tempdir().expect("测试应创建隔离数据库目录");
    let store = OrchestrationTaskRouteStore::new(temp.path().join("app.sqlite"));

    let first = store
        .reserve_operation("cancel-operation-7", "cancel-task-7")
        .expect("首次取消 operation 预留应成功");
    store
        .complete_operation(
            "cancel-operation-7",
            r#"{"ok":{"task_id":"task-7","status":"cancelled"}}"#,
        )
        .expect("首次取消结果应持久化");
    let replay = store
        .reserve_operation("cancel-operation-7", "cancel-task-7")
        .expect("相同取消 operation 应可重放");
    let conflict = store
        .reserve_operation("cancel-operation-7", "cancel-task-8")
        .expect("不同取消目标应返回领域冲突");

    assert!(
        matches!(first, OrchestrationTaskOperationReservation::New),
        "首次取消 operation 必须取得唯一副作用执行权"
    );
    assert_eq!(
        replay,
        OrchestrationTaskOperationReservation::Replay(Some(
            r#"{"ok":{"task_id":"task-7","status":"cancelled"}}"#.to_string(),
        )),
        "相同 operation/fingerprint 必须返回第一次取消结果"
    );
    assert!(
        matches!(conflict, OrchestrationTaskOperationReservation::Conflict),
        "相同 operation ID 配不同 task ref 必须返回幂等冲突"
    );
    assert_ne!(
        replay,
        OrchestrationTaskOperationReservation::New,
        "完成后的重放不得再次取得取消副作用执行权"
    );
}
