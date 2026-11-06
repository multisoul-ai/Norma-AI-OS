use brain::goal::GoalStore;

/// 全局幕僚不能把同名 Agent 的目标和忽略记录混为同一个项目的数据。
/// 数据：alpha/beta 都有 Alan 和相同目标，再向 alpha 写入 6 条目标，超过每项目上限5。
/// 执行：记录、按项目读摘要、记录忽略，然后重新打开同一目录读取持久化目标。
/// 预期：beta 的目标不会被判重复或被 alpha 淘汰；两个项目的摘要、忽略记录彼此隔离。
#[test]
fn goals_and_declines_are_scoped_and_retention_is_per_workspace() {
    let dir = tempfile::tempdir().expect("创建隔离存储");
    let store = GoalStore::open(dir.path().into());
    assert!(
        store.record_for("alpha", "Alan", "完成登录功能并补充测试"),
        "记录 alpha 目标"
    );
    assert!(
        store.record_for("beta", "Alan", "完成登录功能并补充测试"),
        "beta 同名同内容仍是独立目标"
    );
    for n in 1..=6 {
        store.record_for("alpha", "Alan", &format!("实现 alpha 的第 {n} 项功能"));
    }
    store.decline_for("alpha", "Alan", "暂不做 alpha 部署");
    assert!(
        store
            .declined_summary_for("alpha")
            .expect("alpha 忽略摘要")
            .contains("alpha 部署"),
        "忽略记录保留归属"
    );
    assert!(
        store.declined_summary_for("beta").is_none(),
        "beta 不继承 alpha 的忽略"
    );
    let reopened = GoalStore::open(dir.path().into());
    let goals = reopened.goals();
    assert_eq!(goals.len(), 6, "alpha 最新5条加 beta 独立1条");
    assert_eq!(
        goals.iter().filter(|g| g.workspace_id == "alpha").count(),
        5,
        "每项目保留5条"
    );
    assert_eq!(
        goals.iter().filter(|g| g.workspace_id == "beta").count(),
        1,
        "alpha 新目标不得淘汰 beta"
    );
    let beta = reopened.summary_for("beta").expect("beta 摘要");
    assert!(beta.contains("完成登录功能"), "beta 原目标持久化保留");
    assert!(!beta.contains("alpha"), "beta 摘要不包含 alpha 目标");
    let alpha = reopened.summary_for("alpha").expect("alpha 摘要");
    assert!(alpha.contains("第 6 项"), "alpha 保留最新目标");
    assert!(!alpha.contains("完成登录功能"), "只淘汰 alpha 自己的旧目标");
}
