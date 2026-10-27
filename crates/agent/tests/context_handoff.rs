use agent::context::*;

fn state(area: &str) -> WorkingState {
    let reference = ContextRef {
        scope: ContextScope {
            workspace_id: "workspace".into(),
            area_id: area.into(),
            project: ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
        event_id: format!("event-{area}"),
        revision: 2,
    };
    let mut state = WorkingState {
        goal: Some(ContextFact {
            reference: reference.clone(),
            text: format!("goal-{area}"),
        }),
        ..Default::default()
    };
    state.constraints.insert(
        "scope".into(),
        ContextConstraint {
            reference,
            text: format!("constraint-{area}"),
            enforcement: ConstraintEnforcement::Communicated,
        },
    );
    state
}

/// 场景：跨画布交接不能把来源与目标约束扁平合并为同一身份。
/// 构造：两个 area 拥有同 key、同 revision、不同来源的目标和约束。
/// 执行：装配任务正文并逐段反序列化 JSON。
/// 预期：两块带角色与完整引用，原始任务保留，空状态时完全不变。
#[test]
fn context_handoff_retains_both_scopes_and_exact_empty_state_text() {
    let a = state("a");
    let b = state("b");
    let task = context_task_handoff("修复登录", &[("source", &a), ("destination", &b)]).unwrap();
    assert!(task.starts_with("修复登录"), "任务前缀保留");
    let blocks: Vec<serde_json::Value> = task
        .lines()
        .filter(|s| s.starts_with('{'))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(blocks.len(), 2, "应保留两个来源块");
    for (index, (role, area)) in [("source", "a"), ("destination", "b")].iter().enumerate() {
        assert_eq!(blocks[index]["role"], *role, "来源与目标角色明确");
        assert_eq!(
            blocks[index]["constraints"]["scope"]["reference"]["scope"]["area_id"], *area,
            "约束引用身份完整"
        );
        assert_eq!(
            blocks[index]["goal"]["text"],
            format!("goal-{area}"),
            "目标也必须交接"
        );
        assert!(
            blocks[index].get("reports").is_none(),
            "报告正文不得混入任务交接"
        );
    }
    assert_eq!(
        context_task_handoff(" 保留原文\n", &[("same", &WorkingState::default())]).unwrap(),
        " 保留原文\n",
        "无有效状态不改变原文"
    );
}

/// 场景：原始任务加多个长约束超限时不能静默裁剪或者无限分配。
/// 构造：8 条各 8 KiB 的约束及包含转义符的正文。
/// 执行：装配大状态、空白原文和边界大小的纯任务。
/// 预期：合成或原文超 32 KiB 明确失败，空原文拒绝；恰好等于上限可用。
#[test]
fn context_handoff_rejects_oversized_and_empty_tasks_without_truncation() {
    let mut large = state("a");
    let mut constraint = large.constraints["scope"].clone();
    constraint.text = "\\\"".repeat(4096);
    for index in 0..8 {
        large
            .constraints
            .insert(index.to_string(), constraint.clone());
    }
    assert_eq!(
        context_task_handoff("任务", &[("same", &large)]),
        Err("context_handoff_too_large"),
        "完整序列化字节超限时拒绝"
    );
    assert_eq!(
        context_task_handoff(" \n", &[("same", &large)]),
        Err("context_handoff_empty_task"),
        "空原文不能被约束补成任务"
    );
    assert!(
        context_task_handoff(&"x".repeat(32 * 1024), &[]).is_ok(),
        "边界以内原文可用"
    );
    assert!(
        context_task_handoff(&"x".repeat(32 * 1024 + 1), &[]).is_err(),
        "边界以外原文拒绝"
    );
}
