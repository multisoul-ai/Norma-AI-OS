use norma::mobile::{inbox_item_key, WorkspaceActivityTarget};

/// 场景：同项目绑定到两个工作空间时共享事项键，另一项目不能共享。
/// 数据构造：project-1的两个绑定area-a/area-b，其余节点与run/session/turn/request/指纹相同。
/// 执行过程：遍历三种事项类型，比较两个区域绑定以及project-2的键。
/// 预期结果：同项目不同区域键相同；不同项目键不同，跨空间放弃不会波及另一项目。
#[test]
fn same_project_item_has_one_key_across_workspace_area_bindings() {
    let a = WorkspaceActivityTarget {
        area_id: "area-a".into(),
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-1".into(),
        session_id: "session-1".into(),
        turn_id: "turn-1".into(),
        request_id: Some("request-1".into()),
        fingerprint: Some("a".repeat(64)),
    };
    let mut b = a.clone();
    b.area_id = "area-b".into();
    for kind in ["approval", "failed", "review"] {
        let key = inbox_item_key(kind, "project-1", &a);
        assert_eq!(
            key,
            inbox_item_key(kind, "project-1", &b),
            "{kind}跨工作空间绑定共享身份"
        );
        assert_ne!(
            key,
            inbox_item_key(kind, "project-2", &b),
            "{kind}不能跨项目合并"
        );
    }
}
