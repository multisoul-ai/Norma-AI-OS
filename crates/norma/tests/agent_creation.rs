use norma::mobile::WorkspaceAgentCreateRequest;

/// 场景：快捷创建必须有固定区域和支持的类型，不能携带任务或用名称猜目标。
/// 数据构造：空间 a、区域 p、操作 op、版本 1、Codex；另外两条非法请求。
/// 执行过程：解析合法请求并校验 → 缺失区域 → 携带任务 → 不支持的类型。
/// 预期结果：合法请求通过；其余请求全部拒绝，不隐式创建或派工。
#[test]
fn create_request_requires_exact_scope_and_never_includes_a_task() {
    let value = serde_json::json!({"desktop_workspace_id":"a","area_id":"p","operation_id":"op","expected_revision":1,"profile":"Codex"});
    let request: WorkspaceAgentCreateRequest = serde_json::from_value(value.clone()).unwrap();
    assert!(request.validate().is_ok(), "完整创建请求应可执行");
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("area_id");
    assert!(
        serde_json::from_value::<WorkspaceAgentCreateRequest>(missing).is_err(),
        "不能从桌面当前选择补区域"
    );
    let mut task = value.clone();
    task["task"] = serde_json::json!("修改代码");
    assert!(
        serde_json::from_value::<WorkspaceAgentCreateRequest>(task).is_err(),
        "创建不能偷偷携带任务"
    );
    let mut unsupported: WorkspaceAgentCreateRequest = serde_json::from_value(value).unwrap();
    unsupported.profile = "OpenCode".into();
    assert!(
        unsupported.validate().is_err(),
        "不能创建尚未支持的手机执行类型"
    );
}
