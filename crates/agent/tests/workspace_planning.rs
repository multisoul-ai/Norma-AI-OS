use agent::{WorkspacePlanningError, WorkspaceTaskPlan};
use norma::workspace_dispatch::WorkspaceCardRef;

fn targets() -> Vec<WorkspaceCardRef> {
    (1..=2)
        .map(|node_id| WorkspaceCardRef {
            workspace_id: "workspace-a".into(),
            area_id: format!("area-{node_id}"),
            node_id,
            card_instance_id: format!("card-{node_id}"),
        })
        .collect()
}

const COMPLETE: &str = r#"{"assignments":[{"target_index":1,"text":"检查界面"},{"target_index":0,"text":"修复登录"}]}"#;

/// 场景：两个明确目标的完整响应只绑定代码持有的持久卡片，模型输出顺序不改变原目标顺序。
/// 数据构造：空间A有两区域、两持久卡片，JSON将索引1放在0前，形成2项完整计划。
/// 执行过程：严格解析完整JSON，遍历计划的不可变条目并核对目标和正文。
/// 预期结果：返回恰好2项，按可信目标顺序绑定，正文来自相应索引；不能交换卡片或生成新卡片。
#[test]
fn complete_plan_binds_all_original_cards_in_canonical_order() {
    let targets = targets();
    let plan = WorkspaceTaskPlan::from_response(COMPLETE, &targets).expect("完整计划");
    assert_eq!(plan.tasks().len(), 2, "两个明确目标必须一起形成计划");
    assert_eq!(plan.tasks()[0].card(), &targets[0], "首项绑定原卡片1");
    assert_eq!(plan.tasks()[0].text(), "修复登录", "索引0正文不可串到卡片2");
    assert_eq!(plan.tasks()[1].card(), &targets[1], "第二项绑定原卡片2");
    assert_eq!(plan.tasks()[1].text(), "检查界面", "索引1正文保留");
    assert_ne!(
        plan.tasks()[0].card(),
        plan.tasks()[1].card(),
        "不同区域实例保持独立"
    );
    let consumed = plan.into_tasks();
    assert_eq!(consumed.len(), 2, "一次消费必须交付完整不可变批次");
}

/// 场景：模型遗漏第二目标、添加额外目标或重复目标均不能产出可提交计划。
/// 数据构造：可信目标固定为2张卡；分别返回1项、3项和2项重复索引，完整基准响应为2项。
/// 执行过程：逐一严格解析反例，再解析合法完整计划，核对正负路径。
/// 预期结果：反例统一拒绝目标全集，合法全集仍成功；不能把第一项提前当成已接受任务。
#[test]
fn omitted_extra_and_duplicate_targets_reject_the_whole_plan() {
    let targets = targets();
    let invalid = [
        r#"{"assignments":[{"target_index":0,"text":"修复登录"}]}"#,
        r#"{"assignments":[{"target_index":0,"text":"修复登录"},{"target_index":1,"text":"检查界面"},{"target_index":2,"text":"其他工作"}]}"#,
        r#"{"assignments":[{"target_index":0,"text":"修复登录"},{"target_index":0,"text":"重复工作"}]}"#,
    ];
    for response in invalid {
        assert_eq!(
            WorkspaceTaskPlan::from_response(response, &targets).err(),
            Some(WorkspacePlanningError::TargetSetMismatch),
            "全集异常不能返回部分可提交计划：{response}"
        );
    }
    assert!(
        WorkspaceTaskPlan::from_response(COMPLETE, &targets).is_ok(),
        "完整正例可形成一个批次"
    );
}

/// 场景：模型不能伪造可信source、operation、执行fingerprint、force或已接受状态。
/// 数据构造：两项完整基准JSON；分别增加顶层operation/source/accepted以及单项execution/card/force。
/// 执行过程：按字段生成7个非法响应并解析，最后核对基准响应仍有效。
/// 预期结果：所有额外字段严格拒绝，不允许模型获得或修改任何执行授权。
#[test]
fn model_cannot_supply_authority_or_execution_fields() {
    let targets = targets();
    for (field, value) in [
        ("operation_id", serde_json::json!("fake")),
        ("source", serde_json::json!("desktop")),
        ("accepted", serde_json::json!(true)),
    ] {
        let mut response: serde_json::Value = serde_json::from_str(COMPLETE).expect("基准JSON");
        response[field] = value;
        assert_eq!(
            WorkspaceTaskPlan::from_response(&response.to_string(), &targets).err(),
            Some(WorkspacePlanningError::InvalidResponse),
            "顶层可信字段不得来自模型：{field}"
        );
    }
    for field in ["execution", "card", "force", "mode"] {
        let mut response: serde_json::Value = serde_json::from_str(COMPLETE).expect("基准JSON");
        response["assignments"][0][field] = serde_json::json!("fake");
        assert_eq!(
            WorkspaceTaskPlan::from_response(&response.to_string(), &targets).err(),
            Some(WorkspacePlanningError::InvalidResponse),
            "条目可信字段不得来自模型：{field}"
        );
    }
    assert!(
        WorkspaceTaskPlan::from_response(COMPLETE, &targets).is_ok(),
        "仅索引和正文的完整响应允许通过"
    );
}

/// 场景：截断、Markdown包裹、空正文和无效可信身份都必须整组拒绝。
/// 数据构造：2项合法计划；删除最后括号、添加围栏、空白正文、控制符正文；另构造重复或node_id=0目标。
/// 执行过程：逐一解析异常响应，再核对异常目标；同时验证正常响应与目标可解析。
/// 预期结果：没有自动修补部分JSON或身份，没有因第一项有效而返回半个计划。
#[test]
fn incomplete_or_invalid_input_never_produces_a_partial_plan() {
    let targets = targets();
    for response in [
        COMPLETE[..COMPLETE.len() - 1].to_string(),
        format!("```json\n{COMPLETE}\n```"),
        COMPLETE.replace("检查界面", " "),
        COMPLETE.replace("检查界面", "\\u0000"),
    ] {
        assert!(
            WorkspaceTaskPlan::from_response(&response, &targets).is_err(),
            "不完整/无效正文整组拒绝：{response}"
        );
    }
    let duplicate = vec![targets[0].clone(), targets[0].clone()];
    assert_eq!(
        WorkspaceTaskPlan::from_response(COMPLETE, &duplicate).err(),
        Some(WorkspacePlanningError::InvalidInput),
        "可信清单也不能重复同一持久卡片"
    );
    let mut invalid = targets.clone();
    invalid[1].node_id = 0;
    assert_eq!(
        WorkspaceTaskPlan::from_response(COMPLETE, &invalid).err(),
        Some(WorkspacePlanningError::InvalidInput),
        "不合法持久身份必须在规划前拒绝"
    );
    assert!(
        WorkspaceTaskPlan::from_response(COMPLETE, &targets).is_ok(),
        "有效完整计划通过"
    );
}
