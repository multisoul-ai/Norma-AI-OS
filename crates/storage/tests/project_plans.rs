use agent::ProjectPlanDraft;
use storage::{AppStore, ProjectPlanStore};

/// 方向调整生成新计划，旧建议和原文保留，读取按项目隔离并有明确分页。
///
/// 数据构造：项目A/B各有独立workspace_id；A先发布plan-1，再发布plan-2；每页1项。
/// 执行过程：发布 → 同ID同文重试 → 新方向 → 分页回读旧文 → 错项目和ID冲突。
/// 预期结果：两份计划准确保留、重试不增加记录；B无A内容，冲突不覆盖旧建议。
#[test]
fn plans_preserve_directions_and_page_without_cross_project_reads() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).expect("A");
    std::fs::create_dir(&b).expect("B");
    let app = AppStore::new(dir.path().join("app.db"));
    let wa = app
        .workspace_identity_store()
        .resolve_or_create(&a, "A")
        .expect("A身份")
        .workspace_id;
    let wb = app
        .workspace_identity_store()
        .resolve_or_create(&b, "B")
        .expect("B身份")
        .workspace_id;
    let plans = ProjectPlanStore::new(app.path().to_path_buf());
    let draft = ProjectPlanDraft {
        summary: "等待用户确定下一步".into(),
        todos: vec![],
    };
    let first = plans
        .publish("plan-1", &wa, "route-a", "先稳定登录", &draft)
        .expect("发布建议");
    let replay = plans
        .publish("plan-1", &wa, "route-a", "先稳定登录", &draft)
        .expect("重复同文");
    assert_eq!(first, replay, "同ID重试不得创建第二份计划");
    let second = plans
        .publish("plan-2", &wa, "route-a", "优先改善可访问性", &draft)
        .expect("新方向");
    let page = plans.list(&wa, None, 1).expect("新计划页");
    assert_eq!(page.items.len(), 1, "每页预算1必须生效");
    assert_eq!(page.items[0].plan_id, "plan-2", "最新方向先显示");
    assert_eq!(
        page.next_cursor,
        Some(second.sequence),
        "旧记录不能被静默丢弃"
    );
    let older = plans.list(&wa, page.next_cursor, 1).expect("旧计划页");
    assert_eq!(
        older.items[0].direction, "先稳定登录",
        "调整方向不得覆盖用户原文"
    );
    assert!(older.next_cursor.is_none(), "最后一页不能产生无限游标");
    assert!(
        plans.get(&wb, "plan-1").expect("B回读").is_none(),
        "不能按ID越权读取A计划"
    );
    assert!(
        plans.list(&wb, None, 1).expect("B列表").items.is_empty(),
        "B列表不得混入A"
    );
    assert!(
        plans
            .publish("plan-1", &wa, "route-a", "覆盖原文", &draft)
            .is_err(),
        "已发布计划不可被同ID改写"
    );
    assert_eq!(
        plans.get(&wa, "plan-1").expect("再次读取").expect("仍存在"),
        first,
        "冲突不得损失原始建议"
    );
    assert!(
        plans.list(&wa, None, 5).is_err(),
        "超过4份完整计划的单页预算应明确拒绝"
    );
}

/// 较晚完成的旧方向不得覆盖另一端已发布的新方向，原计划始终保留。
/// 数据构造：base→new两份计划；同ID重试new；late仍基于base；每页上限4覆盖全部2项。
/// 执行过程：基于base发布new → 重试同请求 → 基于过期base发布late → 回读列表与原文。
/// 预期结果：new仍是最新，base保留且late不存在；幂等重试不增加记录。
#[test]
fn direction_publication_checks_latest_plan_atomically() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let app = AppStore::new(dir.path().join("app.sqlite"));
    let workspace = app
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "计划")
        .expect("项目")
        .workspace_id;
    let plans = ProjectPlanStore::new(app.path().to_path_buf());
    let draft = ProjectPlanDraft {
        summary: "建议".into(),
        todos: vec![],
    };
    let base = plans
        .publish("base", &workspace, "route-a", "原方向", &draft)
        .expect("原方向");
    let next = plans
        .publish_after("new", &base, "新方向", &draft)
        .expect("当前计划可调整");
    assert_eq!(
        plans
            .publish_after("new", &base, "新方向", &draft)
            .expect("同请求重试"),
        next,
        "已成功请求须幂等，即使base已不是最新"
    );
    assert!(
        plans
            .publish_after("late", &base, "迟到方向", &draft)
            .is_err(),
        "旧计划生成结果不能覆盖新方向"
    );
    assert!(
        plans
            .get(&workspace, "late")
            .expect("核对迟到结果")
            .is_none(),
        "失败不能残留新计划"
    );
    assert_eq!(
        plans
            .get(&workspace, "base")
            .expect("原计划")
            .expect("存在"),
        base,
        "原计划不可修改"
    );
    let page = plans.list(&workspace, None, 4).expect("全部计划");
    assert_eq!(page.items[0].plan_id, "new", "新方向必须仍为最新");
    assert_eq!(page.items.len(), 2, "重试和冲突均不得新增记录");
}
