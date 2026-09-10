use agent::ProjectPlanDraft;
use norma::mobile::CardSnapshot;

/// 指定优先事项只前移准确的同名原卡任务；缺失或歧义不能被当作成功。
/// 数据构造：同一Codex卡7上有alpha/beta两项；选择index1的beta；重复beta构造歧义。
/// 执行过程：模型给出alpha→beta → 指定beta → 再尝试缺失及重复的beta。
/// 预期结果：beta居首且alpha保留；缺失/重复返回错误，不改变待办内容和执行身份。
#[test]
fn priority_moves_only_the_selected_work_and_rejects_missing_or_duplicate_work() {
    let card: CardSnapshot = serde_json::from_value(serde_json::json!({
        "node_id":7,"profile":"Codex","title":"开发","mobile_control":true,"state":"ready",
        "run_id":"run-a","session_id":"session-a","turn_id":null,"task_title":null
    }))
    .expect("明确的原卡快照");
    let mut draft = ProjectPlanDraft::from_response(
        r#"{"summary":"按目标推进","todos":[
        {"node_id":7,"title":"alpha","instruction":"验证alpha"},
        {"node_id":7,"title":"beta","instruction":"验证beta并报告结果"}]}"#,
        &[card.clone()],
    )
    .expect("模型建议");
    let priority = draft.todos[1].clone();
    draft.prioritize(&priority).expect("准确优先事项");
    assert_eq!(
        draft.todos[0].title, "beta",
        "选择beta必须实际居首，不能只保存偏好文字"
    );
    assert_eq!(draft.todos[1].title, "alpha", "其余待办必须保留");
    assert_eq!(draft.todos[0].target, card, "排序不能替换原卡身份");
    assert_eq!(
        draft.todos[0].instruction, "验证beta并报告结果",
        "完整执行指令不得被标题替代"
    );
    let mut missing = draft.clone();
    missing.todos.remove(0);
    assert_eq!(
        missing.prioritize(&priority),
        Err("invalid_plan_priority"),
        "模型遗漏优先事项必须显式失败"
    );
    assert_eq!(missing.todos[0].title, "alpha", "失败不能伪造beta");
    let mut ambiguous = draft.clone();
    ambiguous.todos.push(priority.clone());
    assert_eq!(
        ambiguous.prioritize(&priority),
        Err("invalid_plan_priority"),
        "同名同卡重复不能任取一条"
    );
    assert_eq!(ambiguous.todos.len(), 3, "校验失败不能删除任何建议");
    let mut foreign = priority;
    foreign.target.node_id = 8; // 8 = 原卡7 + 1，模拟同名但属于另一张卡的事项。
    assert_eq!(
        draft.prioritize(&foreign),
        Err("invalid_plan_priority"),
        "同名事项不能跨卡匹配"
    );
}
