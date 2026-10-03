use agent::{MemoryEntryId, MemoryMutation, MemoryRevision, MemoryScope, MemorySourceReference};
use storage::{DatabasePaths, MemoryError, MemoryStore};

/// 中文偏好保存后，重新打开 Store 仍能读取正文和同版本目录。
///
/// 数据构造：
///   - 独占临时 App 数据根，个人范围包含一个 preferences 主题。
///   - frontmatter 名称“交流偏好”、描述“用户希望简洁中文”，正文保留原始 UTF-8。
///
/// 执行过程：
///   1. 取得空库快照，按该版本提交主题。
///   2. 丢弃 Store，重新打开并取得已发布快照。
///   3. 经公共 read 接口读取主题，检查目录名称、描述和链接。
///
/// 预期结果：
///   - 正断言：正文逐字保存，目录包含名称、描述和主题链接。
///   - 负断言：无关的 project-facts 主题不存在。
#[test]
fn chinese_memory_survives_reopening_with_its_generated_catalog() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let store = MemoryStore::open(&paths).expect("应打开个人记忆库");
    let scope = MemoryScope::Personal;
    let initial = store.snapshot(&scope).expect("应取得空快照");
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    let markdown = "---\nname: 交流偏好\ndescription: 用户希望简洁中文\ntype: user\nmodified: 2026-10-03\n---\n请用简洁中文交流。\n";
    let receipt = store
        .commit(
            &scope,
            initial.revision(),
            &[MemoryMutation::Upsert {
                id: id.clone(),
                markdown: markdown.into(),
                sources: vec![],
            }],
        )
        .expect("中文偏好应成功发布");
    drop(store);
    let reopened = MemoryStore::open(&paths).expect("应重开记忆库");
    let snapshot = reopened.snapshot(&scope).expect("应读取已发布快照");
    let entry = reopened
        .read(&snapshot, &id)
        .expect("应读取主题")
        .expect("偏好必须存在");
    assert_eq!(
        entry.markdown, markdown,
        "重开后必须逐字保留 UTF-8 正文和 frontmatter"
    );
    assert_eq!(
        snapshot.revision(),
        receipt.revision,
        "目录和正文必须来自已发布版本"
    );
    assert!(
        snapshot.catalog().contains("交流偏好"),
        "目录必须包含主题名称"
    );
    assert!(
        snapshot.catalog().contains("用户希望简洁中文"),
        "目录必须包含主题描述"
    );
    assert!(
        snapshot.catalog().contains("preferences.md"),
        "目录必须链接主题文件"
    );
    let absent = MemoryEntryId::parse("project-facts").expect("缺失主题 ID 应有效");
    assert!(
        reopened
            .read(&snapshot, &absent)
            .expect("缺失查询应成功")
            .is_none(),
        "未写入的项目主题不得出现"
    );
}

/// 旧快照保持完整，旧版本写入被拒绝，回滚以新版本恢复旧主题与目录。
///
/// 数据构造：
///   - V1 名称“旧交流”/正文“偏好中文”；V2 名称“新交流”/正文“偏好英文”。
///   - rollback 指向 V1，预期产生大于 V2 的新版本，不重新使用 V1 身份。
///
/// 执行过程：
///   1. 保存 V1 并持有旧快照，再通过另一 Store 实例保存 V2。
///   2. 用 V1 版本提交“迟到写入”，检查冲突且当前正文仍是 V2。
///   3. 基于 V2 回滚 V1，重新打开并读取；旧快照继续读取 V1。
///
/// 预期结果：
///   - 正断言：旧快照与新快照各自保持匹配的目录/正文；回滚恢复 V1。
///   - 负断言：迟到写入不得覆盖 V2，回滚不得复用 V1 或 V2 的版本。
#[test]
fn snapshots_conflicts_and_restore_preserve_complete_revisions() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let store = MemoryStore::open(&paths).expect("应打开记忆库");
    let scope = MemoryScope::Personal;
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    let old_markdown = "---\nname: 旧交流\ndescription: 中文偏好\n---\n偏好中文";
    let new_markdown = "---\nname: 新交流\ndescription: 英文偏好\n---\n偏好英文";
    let first = store
        .commit(
            &scope,
            MemoryRevision::INITIAL,
            &[upsert(&id, old_markdown)],
        )
        .expect("V1 应发布");
    let old = store.snapshot(&scope).expect("应固定 V1 快照");
    let second_store = MemoryStore::open(&paths).expect("第二实例应打开同一库");
    let second = second_store
        .commit(&scope, first.revision, &[upsert(&id, new_markdown)])
        .expect("V2 应发布");
    let conflict = store
        .commit(&scope, first.revision, &[upsert(&id, "迟到写入")])
        .expect_err("旧版本不得覆盖新版本");
    assert!(
        matches!(conflict, MemoryError::Conflict { expected, current } if expected == first.revision && current == second.revision),
        "冲突必须明确旧预期版本与当前版本"
    );
    let latest = second_store.snapshot(&scope).expect("应读取 V2");
    assert_eq!(
        store
            .read(&latest, &id)
            .expect("V2 应可读")
            .expect("主题应存在")
            .markdown,
        new_markdown,
        "冲突不得覆盖 V2 正文"
    );
    assert_eq!(
        store
            .read(&old, &id)
            .expect("旧快照应可读")
            .expect("主题应存在")
            .markdown,
        old_markdown,
        "固定快照必须保持 V1 正文"
    );
    assert!(old.catalog().contains("旧交流"), "V1 目录必须与旧正文匹配");
    assert!(!old.catalog().contains("新交流"), "V1 快照不得混入 V2 目录");
    assert!(
        latest.catalog().contains("新交流"),
        "V2 目录必须与新正文匹配"
    );
    assert!(
        !latest.catalog().contains("旧交流"),
        "V2 快照不得混入 V1 目录"
    );
    let restored = store
        .restore(&scope, second.revision, first.revision)
        .expect("应恢复 V1 为新版本");
    assert!(
        restored.revision.number() > second.revision.number(),
        "回滚必须发布新版本以免旧写入重新获得权限"
    );
    assert_ne!(restored.revision, first.revision, "回滚不得复用 V1 身份");
    let reopened = MemoryStore::open(&paths).expect("回滚后应重开");
    let recovered = reopened.snapshot(&scope).expect("应读取回滚版本");
    assert_eq!(
        recovered.revision(),
        restored.revision,
        "重开必须定位回滚的新版本"
    );
    assert_eq!(
        reopened
            .read(&recovered, &id)
            .expect("恢复主题应可读")
            .expect("主题应存在")
            .markdown,
        old_markdown,
        "回滚必须恢复 V1 原文"
    );
    assert!(
        recovered.catalog().contains("旧交流"),
        "回滚必须同时恢复 V1 目录"
    );
    assert!(
        !recovered.catalog().contains("新交流"),
        "回滚目录不得保留 V2 元数据"
    );
    assert!(
        !recovered.catalog().contains("迟到写入"),
        "拒绝的写入不得进入目录"
    );
}

fn upsert(id: &MemoryEntryId, markdown: &str) -> MemoryMutation {
    MemoryMutation::Upsert {
        id: id.clone(),
        markdown: markdown.into(),
        sources: vec![],
    }
}

/// 项目约定按宿主项目 ID 共享，工作树事实和其他项目保持隔离。
///
/// 数据构造：
///   - repo-a 共享主题 conventions；repo-a/worktree-a 主题 task-result。
///   - repo-b 和 repo-a/worktree-b 均未写入主题；同一数据根打开两个 Store。
///
/// 执行过程：
///   1. 分别提交共享约定和 worktree-a 的任务事实。
///   2. 第二实例读取 repo-a 共享范围，随后查询其他三个范围。
///
/// 预期结果：
///   - 正断言：同项目第二实例能读共享约定，worktree-a 能读自己的任务事实。
///   - 负断言：共享库没有任务事实；repo-b 和 worktree-b 没有另一范围的主题。
#[test]
fn project_shared_and_worktree_scopes_do_not_leak() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let store = MemoryStore::open(&paths).expect("应打开记忆库");
    let shared = MemoryScope::ProjectShared {
        project_id: "repo-a".into(),
    };
    let other = MemoryScope::ProjectShared {
        project_id: "repo-b".into(),
    };
    let tree_a = MemoryScope::Worktree {
        project_id: "repo-a".into(),
        worktree_id: "tree-a".into(),
    };
    let tree_b = MemoryScope::Worktree {
        project_id: "repo-a".into(),
        worktree_id: "tree-b".into(),
    };
    let convention = MemoryEntryId::parse("conventions").expect("约定 ID 应有效");
    let task = MemoryEntryId::parse("task-result").expect("任务 ID 应有效");
    store
        .commit(
            &shared,
            MemoryRevision::INITIAL,
            &[upsert(&convention, "项目使用 Rust")],
        )
        .expect("共享约定应发布");
    store
        .commit(
            &tree_a,
            MemoryRevision::INITIAL,
            &[upsert(&task, "此工作树测试通过")],
        )
        .expect("工作树事实应发布");
    let second = MemoryStore::open(&paths).expect("第二实例应打开同一库");
    let shared_snapshot = second.snapshot(&shared).expect("应读取共享库");
    assert_eq!(
        second
            .read(&shared_snapshot, &convention)
            .expect("共享查询应成功")
            .expect("约定应存在")
            .markdown,
        "项目使用 Rust",
        "同项目工作树应通过同一共享范围读约定"
    );
    assert!(
        second
            .read(&shared_snapshot, &task)
            .expect("共享查询应成功")
            .is_none(),
        "工作树测试结果不得进入共享约定"
    );
    let local = store.snapshot(&tree_a).expect("应读取工作树 A");
    assert_eq!(
        store
            .read(&local, &task)
            .expect("工作树查询应成功")
            .expect("任务应存在")
            .markdown,
        "此工作树测试通过",
        "任务事实必须留在所属工作树"
    );
    let other_snapshot = store.snapshot(&other).expect("应读取另一个项目");
    assert!(
        store
            .read(&other_snapshot, &convention)
            .expect("另一项目查询应成功")
            .is_none(),
        "repo-b 不得读取 repo-a 的约定"
    );
    let tree_b_snapshot = store.snapshot(&tree_b).expect("应读取工作树 B");
    assert!(
        store
            .read(&tree_b_snapshot, &task)
            .expect("工作树 B 查询应成功")
            .is_none(),
        "tree-b 不得读取 tree-a 的任务事实"
    );
    let personal = store
        .snapshot(&MemoryScope::Personal)
        .expect("应读取个人库");
    assert!(
        store
            .read(&personal, &convention)
            .expect("个人查询应成功")
            .is_none(),
        "项目约定不得自动升级为个人记忆"
    );
}

/// 来源引用随主题保存，但空来源标识在提交边界被拒绝。
///
/// 数据构造：
///   - 原始来源为 UserTurn(session-1, turn-1)。
///   - 无效候选来源为 SettingsEdit(edit_id="")，正文尝试覆盖为“错误更新”。
///
/// 执行过程：
///   1. 写入有效来源并重开读取。
///   2. 在当前版本提交无效来源，检查失败后原文与版本未改变。
///
/// 预期结果：
///   - 正断言：原始来源逐项保留，拒绝后仍能读取“偏好中文”。
///   - 负断言：空来源不得提交，错误更新不得发布。
#[test]
fn source_references_are_preserved_and_invalid_references_rejected() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let store = MemoryStore::open(&paths).expect("应打开记忆库");
    let scope = MemoryScope::Personal;
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    let source = MemorySourceReference::UserTurn {
        session_id: "session-1".into(),
        turn_id: "turn-1".into(),
    };
    let mutation = MemoryMutation::Upsert {
        id: id.clone(),
        markdown: "偏好中文".into(),
        sources: vec![source.clone()],
    };
    let receipt = store
        .commit(&scope, MemoryRevision::INITIAL, &[mutation])
        .expect("来源应保存");
    let reopened = MemoryStore::open(&paths).expect("应重开记忆库");
    let snapshot = reopened.snapshot(&scope).expect("应读取快照");
    assert_eq!(
        reopened
            .read(&snapshot, &id)
            .expect("应读主题")
            .expect("主题应存在")
            .sources,
        vec![source],
        "原始用户来源引用不得丢失"
    );
    let invalid = MemoryMutation::Upsert {
        id: id.clone(),
        markdown: "错误更新".into(),
        sources: vec![MemorySourceReference::SettingsEdit {
            edit_id: String::new(),
        }],
    };
    assert!(
        matches!(
            reopened.commit(&scope, receipt.revision, &[invalid]),
            Err(MemoryError::InvalidDocument(_))
        ),
        "空来源 ID 必须在提交边界被拒绝"
    );
    let unchanged = reopened.snapshot(&scope).expect("拒绝后应可读");
    assert_eq!(
        unchanged.revision(),
        receipt.revision,
        "拒绝后不得推进活动版本"
    );
    assert_eq!(
        reopened
            .read(&unchanged, &id)
            .expect("原主题应可读")
            .expect("原主题应存在")
            .markdown,
        "偏好中文",
        "无效来源不得覆盖原正文"
    );
}
