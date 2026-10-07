use agent::{MemoryCommand, MemoryScopeAlias};
use storage::{DatabasePaths, MemoryAccess};

/// 写入发布后回执失败，重新核验能找到已发布版本，不重复发布。
///
/// 数据构造：独立数据根；SQLite fixture trigger 只阻止 memory_operations 的回执更新。
/// 执行过程：人工保存→MD 发布而回执失败→移除故障→重开同操作核验→读取。
/// 预期结果：失败报告“无法确认”；重试返回版本1，正文存在，未产生版本2。
#[test]
fn lost_receipt_reconciles_without_republishing() {
    let temp = tempfile::tempdir().expect("独立根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = MemoryAccess::new(paths.clone(), None);
    let initial = access
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("初始版本")
        .revision();
    access.preferences().expect("初始化宿主数据库");
    // The database trigger is external fault injection, not an assertion side channel.
    let database = rusqlite::Connection::open(paths.app_db_path()).expect("fixture故障连接");
    database.execute_batch("CREATE TRIGGER lost_receipt BEFORE UPDATE ON memory_operations BEGIN SELECT RAISE(FAIL, 'fixture receipt unavailable'); END;").expect("注入故障");
    let entries = vec![("preference".into(), "请用中文".into())];
    let failed = access
        .settings_save(
            MemoryScopeAlias::Personal,
            initial,
            entries.clone(),
            "人工记忆保存",
            "edit-1",
        )
        .expect_err("回执应失败");
    assert!(
        failed.to_string().contains("无法确认"),
        "MD已发布时不能假称未保存"
    );
    database
        .execute_batch("DROP TRIGGER lost_receipt;")
        .expect("恢复服务");
    let mut reopened = MemoryAccess::new(paths, None);
    let refreshed = reopened
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("重开最新版本")
        .revision();
    let receipt = reopened
        .settings_save(
            MemoryScopeAlias::Personal,
            refreshed,
            entries,
            "人工记忆保存",
            "edit-1",
        )
        .expect("核验发布祖先");
    assert!(receipt.saved, "核验应确认保存");
    assert_eq!(receipt.revision.number(), 1, "不能再次发布产生版本2");
    let (revision, entry) = reopened
        .settings_read(MemoryScopeAlias::Personal, "preference")
        .expect("公开读取");
    assert_eq!(revision.number(), 1, "CURRENT必须保持原发布版本");
    assert_eq!(entry.markdown, "请用中文", "真实发布正文应保留");
    assert!(!entry.markdown.contains("英文"), "核验不能产生其他候选");
}

/// 只读取长文首块不足以授权覆盖或合并删除，伪造跳过剩余正文也不能获得完整读取资格。
///
/// 数据构造：“中文”重复2000次=2字×3字节×2000=12000字节，超过8192字节回合工具预算。
/// 执行过程：保存长文→首块read→明确纠正write→合并删除长文→跳到最后字节读取。
/// 预期结果：三个请求均拒绝；原12000字节正文保留，没有“覆盖”正文或新合并主题。
#[test]
fn partial_read_cannot_authorize_overwriting_unseen_text() {
    let temp = tempfile::tempdir().expect("独立根");
    let mut access = MemoryAccess::new(DatabasePaths::isolated_test(temp.path()), None);
    let initial = access
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("空库")
        .revision();
    let markdown = "中文".repeat(2000);
    access
        .settings_save(
            MemoryScopeAlias::Personal,
            initial,
            vec![("long".into(), markdown.clone())],
            "人工长文",
            "edit-long",
        )
        .expect("保存长文");
    let context = |turn: &str| agent::ToolContext {
        project_id: String::new(),
        turn_id: turn.into(),
        user_input: "请纠正记忆：覆盖".into(),
        input_source: agent::AgentInputSource::Other,
        contextual_delete_targets: vec![],
        tool_call_id: "call".into(),
        operation_id: Some(turn.into()),
        cancellation: Default::default(),
    };
    access
        .execute(
            MemoryCommand::Read {
                scope: MemoryScopeAlias::Personal,
                id: "long".into(),
                offset: 0,
            },
            &context("s:1"),
        )
        .expect("首块");
    assert!(
        access
            .execute(
                MemoryCommand::Write {
                    scope: MemoryScopeAlias::Personal,
                    id: "long".into(),
                    markdown: "覆盖".into(),
                    supersedes: vec![],
                },
                &context("s:2")
            )
            .is_err(),
        "未读完不能覆盖"
    );
    assert!(
        access
            .execute(
                MemoryCommand::Write {
                    scope: MemoryScopeAlias::Personal,
                    id: "merged".into(),
                    markdown: "覆盖".into(),
                    supersedes: vec!["long".into()],
                },
                &context("s:merge"),
            )
            .is_err(),
        "未读完的长文不能通过合并被删除"
    );
    assert!(
        access
            .execute(
                MemoryCommand::Read {
                    scope: MemoryScopeAlias::Personal,
                    id: "long".into(),
                    offset: markdown.len() - 3
                },
                &context("s:3")
            )
            .is_err(),
        "不得跳过正文取得完整读取资格"
    );
    let (_, entry) = access
        .settings_read(MemoryScopeAlias::Personal, "long")
        .expect("读取最终正文");
    assert_eq!(entry.markdown, markdown, "失败后原文应保留");
    assert_ne!(entry.markdown, "覆盖", "不应有未授权覆盖");
    assert!(
        access
            .settings_read(MemoryScopeAlias::Personal, "merged")
            .is_err(),
        "未授权合并不能部分创建新主题"
    );
}
