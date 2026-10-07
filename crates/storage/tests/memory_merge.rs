use agent::{MemoryCommand, MemoryScopeAlias};
use serde_json::json;
use storage::{DatabasePaths, MemoryAccess};

#[path = "support/memory_merge.rs"]
mod support;
use support::{context, merge, read, seed};

/// 后续对话合并重复主题后，当前库与新会话只看到完整的新记忆。
///
/// 数据构造：旧偏好包含中文与 cargo build，新偏好包含简洁中文，另有设备主题。
///   初始主题数=2 份重叠偏好+1 份设备=3；合并后=1 份偏好+1 份设备=2。
/// 执行过程：1. 保存历史重复内容。2. 完整读取两个偏好。3. 一次写入合并正文及删除目标。
///   4. 重开会话，检查目录、正文、搜索与来源。
/// 预期结果：新偏好及独立设备存在；旧主题不可读取/搜索；旧主题独有的构建命令保留。
#[test]
fn later_dialogue_atomically_merges_and_removes_duplicate_topics() {
    let temp = tempfile::tempdir().expect("独立数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = seed(&paths);
    read(&mut access, "old-preferences", "s:read-old");
    read(&mut access, "preferences", "s:read-current");
    let saved = access
        .execute(merge(), &context("s:merge"))
        .expect("合并应成功");
    assert!(saved.saved, "保存与删除均发布后才能返回 saved=true");
    // revision = seed(1) + atomic merge(1) = 2，不能先新增再另行删除。
    assert_eq!(saved.revision.number(), 2, "合并与删除必须只发布一个新版本");
    let mut reopened = MemoryAccess::new(paths, None);
    let catalog = reopened.activate().expect("新会话应加载当前目录");
    assert!(catalog.contains("preferences.md"), "新偏好必须进入目录");
    assert!(catalog.contains("device.md"), "独立设备主题必须保留");
    assert!(
        !catalog.contains("old-preferences"),
        "新会话不得注入重复旧主题"
    );
    let (_, entry) = reopened
        .settings_read(MemoryScopeAlias::Personal, "preferences")
        .expect("新正文");
    assert_eq!(
        entry.markdown, "请用简洁中文交流。构建命令为 cargo build。",
        "合并必须保留旧主题的独有事实"
    );
    assert!(
        reopened
            .settings_read(MemoryScopeAlias::Personal, "old-preferences")
            .is_err(),
        "旧主题不能继续被读取"
    );
    let sources = reopened.source_details(&entry.sources).expect("合并来源");
    assert_eq!(
        sources[0].text,
        context("s:merge").user_input,
        "来源必须绑定实际清理请求"
    );
    let found = reopened
        .execute(
            MemoryCommand::Search {
                scope: MemoryScopeAlias::Personal,
                query: "中文".into(),
                offset: 0,
            },
            &context("next:search"),
        )
        .expect("搜索合并后的偏好");
    assert_eq!(found.entries.len(), 1, "后续搜索只能返回一份偏好");
    assert_eq!(
        found.entries[0].id.as_str(),
        "preferences",
        "搜索必须返回保留主题"
    );
}

/// 已发布合并的回执丢失后，重开会话仍能核验删除及正文，不要求读取已经删除的主题。
///
/// 数据构造：seed 发布版本 1；SQLite trigger 只阻止回执更新，不阻止 Markdown 发布。
///   合并版本=初始发布(1)+一次合并(1)=2；无关设备编辑后当前版本=2+1=3。
/// 执行过程：1. 完整读取两个偏好。2. 注入回执故障并合并。3. 移除故障、重开会话。
///   4. 无关设备编辑发布版本 3。5. 用相同操作身份重试，再检查当前目录。
/// 预期结果：返回原版本 2 的 saved 回执；当前仍为版本 3，旧主题不存在；不重复递增目录代次。
#[test]
fn lost_merge_receipt_reconciles_without_rereading_deleted_topics() {
    let temp = tempfile::tempdir().expect("独立数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = seed(&paths);
    read(&mut access, "old-preferences", "s:read-old");
    read(&mut access, "preferences", "s:read-current");
    let database = rusqlite::Connection::open(paths.app_db_path()).expect("故障注入连接");
    database.execute_batch("CREATE TRIGGER lost_merge_receipt BEFORE UPDATE ON memory_operations BEGIN SELECT RAISE(FAIL, 'receipt unavailable'); END;").expect("注入回执故障");
    let failed = access
        .execute(merge(), &context("s:merge"))
        .expect_err("发布成功但回执失败");
    assert!(
        matches!(failed, agent::AdapterError::Unknown(_)),
        "发布后故障必须报告待核验"
    );
    database
        .execute_batch("DROP TRIGGER lost_merge_receipt;")
        .expect("恢复回执写入");
    let mut manual = MemoryAccess::new(paths.clone(), None);
    let current = manual
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("已发布合并")
        .revision();
    manual
        .settings_save(
            MemoryScopeAlias::Personal,
            current,
            vec![("device".into(), "使用 MacBook。".into())],
            "无关设备编辑",
            "device-edit",
        )
        .expect("发布不改变合并内容的后代版本");
    let mut reopened = MemoryAccess::new(paths, None);
    reopened.activate().expect("重开当前目录");
    let saved = reopened
        .execute(merge(), &context("s:merge"))
        .expect("应核验此前已发布合并");
    assert!(saved.saved, "正文及删除完整发布后应确认保存");
    assert_eq!(saved.revision.number(), 2, "重试必须返回原合并版本");
    let snapshot = reopened
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("当前快照");
    assert_eq!(
        snapshot.revision().number(),
        3,
        "核验应保留无关编辑后的当前版本 3，不能再发布版本 4"
    );
    assert!(
        !snapshot
            .entries()
            .any(|entry| entry.id.as_str() == "old-preferences"),
        "回执恢复不能恢复旧主题"
    );
    assert!(
        snapshot
            .entries()
            .any(|entry| entry.id.as_str() == "preferences"),
        "合并主题必须保留"
    );
    reopened
        .execute(merge(), &context("s:merge"))
        .expect("重复核验应返回已保存回执");
    // catalog_epoch = initial(0) + confirmed merge(1) = 1；同一回执重试不得再递增。
    assert_eq!(
        reopened.catalog_epoch().expect("目录更新代次"),
        1,
        "一次合并只能更新一次目录代次"
    );
}

/// 未读旧正文、保存关闭、取消与过期基线均不能删除旧记忆或部分写入合并正文。
///
/// 数据构造：两份偏好与独立设备共 3 项；人工第二次编辑在旧主题新增“必须跑 clippy”。
///   初始版本 1，人工新增事实后版本=1+1=2；所有被拒合并不得再递增。
/// 执行过程：1. 只读保留主题后尝试合并。2. 完整读旧主题后测试关闭保存及取消。
///   3. 另一服务发布新增事实，以旧基线合并。4. 只刷新目录再尝试合并。
/// 预期结果：独有新事实与两个原主题都存在，合并正文及额外版本不存在。
#[test]
fn unread_cancelled_disabled_or_stale_merge_preserves_all_topics() {
    let temp = tempfile::tempdir().expect("独立数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = seed(&paths);
    read(&mut access, "preferences", "s:read-current");
    let unread = access
        .execute(merge(), &context("s:unread"))
        .expect_err("未读旧主题不可删除");
    assert!(
        matches!(unread, agent::AdapterError::Rejected(_)),
        "合并必须要求读取旧主题全文"
    );
    read(&mut access, "old-preferences", "s:read-old");
    access.set_save_enabled(false).expect("关闭保存");
    assert!(
        matches!(
            access.execute(merge(), &context("s:off")),
            Err(agent::AdapterError::Rejected(_))
        ),
        "关闭保存必须同时阻止合并和删除"
    );
    access.set_save_enabled(true).expect("重开保存");
    let cancelled = context("s:cancelled");
    cancelled.cancellation.cancel();
    assert!(
        matches!(
            access.execute(merge(), &cancelled),
            Err(agent::AdapterError::Cancelled)
        ),
        "取消必须阻止整个合并"
    );
    let mut manual = MemoryAccess::new(paths, None);
    let version = manual
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("人工当前快照")
        .revision();
    assert_eq!(version.number(), 1, "未读、关闭和取消不能发布任何新版本");
    manual
        .settings_save(
            MemoryScopeAlias::Personal,
            version,
            vec![(
                "old-preferences".into(),
                "请用中文交流。构建命令为 cargo build。必须跑 clippy。".into(),
            )],
            "新增独有事实",
            "manual-correction",
        )
        .expect("并发人工编辑");
    assert!(
        access.execute(merge(), &context("s:stale")).is_err(),
        "过期基线不能丢掉新增事实"
    );
    access
        .execute(
            MemoryCommand::List {
                scope: MemoryScopeAlias::Personal,
                offset: 0,
            },
            &context("s:list"),
        )
        .expect("刷新目录");
    assert!(
        matches!(
            access.execute(merge(), &context("s:stale-read")),
            Err(agent::AdapterError::Rejected(_))
        ),
        "目录刷新不能冒充读取了新正文"
    );
    let (version, old) = manual
        .settings_read(MemoryScopeAlias::Personal, "old-preferences")
        .expect("旧主题仍可读");
    assert_eq!(version.number(), 2, "仅人工编辑应发布版本 2");
    assert!(
        old.markdown.contains("必须跑 clippy"),
        "过期合并不能删除新增的独有事实"
    );
    let (_, current) = manual
        .settings_read(MemoryScopeAlias::Personal, "preferences")
        .expect("保留主题仍可读");
    assert_eq!(
        current.markdown, "请用简洁中文交流。",
        "失败不能部分覆盖保留主题"
    );
    assert!(
        !current.markdown.contains("cargo build"),
        "被拒合并正文不能部分发布"
    );
    let (_, device) = manual
        .settings_read(MemoryScopeAlias::Personal, "device")
        .expect("独立设备信息");
    assert_eq!(device.markdown, "使用 Mac。", "失败不能影响无关主题");
}

/// 未确认操作只找到相同新正文时，不能冒充重复旧主题已经删除的完整合并。
///
/// 数据构造：seed 版本 1；锁阻止合并发布但保留待核验日志；外部只更新新正文到版本 2。
/// 执行过程：1. 完整读取两个偏好。2. 持有写锁使合并失败。3. 释放锁后只发布新正文。
///   4. 重开会话核验原合并，再用同一操作身份篡改删除列表。
/// 预期结果：核验返回 Unknown、篡改返回 Rejected；旧主题仍在，不得额外发布版本。
#[test]
fn reconciliation_requires_deletions_and_binds_their_operation_identity() {
    let temp = tempfile::tempdir().expect("独立数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = seed(&paths);
    read(&mut access, "old-preferences", "s:read-old");
    read(&mut access, "preferences", "s:read-current");
    let store = storage::MemoryStore::open(&paths).expect("共享存储");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(store.root().join("personal/LOCK"))
        .expect("范围锁");
    lock.try_lock().expect("注入写锁冲突");
    assert!(
        access.execute(merge(), &context("s:merge")).is_err(),
        "范围锁必须阻止合并发布"
    );
    drop(lock);
    store
        .commit(
            &agent::MemoryScope::Personal,
            agent::MemoryRevision::new(1), // 当前版本=seed 的唯一一次发布=1。
            &[agent::MemoryMutation::Upsert {
                id: agent::MemoryEntryId::parse("preferences").expect("保留主题 ID"),
                markdown: "请用简洁中文交流。构建命令为 cargo build。".into(),
                sources: vec![agent::MemorySourceReference::UserTurn {
                    session_id: "s".into(),
                    turn_id: "s:merge".into(),
                }],
            }],
        )
        .expect("外部只发布相同正文，保留旧主题");
    let mut reopened = MemoryAccess::new(paths, None);
    reopened.activate().expect("重开目录");
    assert!(
        matches!(
            reopened.execute(merge(), &context("s:merge")),
            Err(agent::AdapterError::Unknown(_))
        ),
        "只匹配新正文不能确认完整合并"
    );
    let changed = serde_json::from_value(json!({"action":"write", "scope":"personal", "id":"preferences", "markdown":"请用简洁中文交流。构建命令为 cargo build。", "supersedes":[]})).expect("篡改删除列表");
    assert!(
        matches!(
            reopened.execute(changed, &context("s:merge")),
            Err(agent::AdapterError::Rejected(_))
        ),
        "同操作身份不能改变删除目标"
    );
    let snapshot = reopened
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("当前快照");
    // revision = seed(1) + external update(1) = 2；核验不得重新执行合并。
    assert_eq!(snapshot.revision().number(), 2, "核验不能发布新版本");
    assert!(
        snapshot
            .entries()
            .any(|entry| entry.id.as_str() == "preferences"),
        "外部发布的新正文应存在"
    );
    assert!(
        snapshot
            .entries()
            .any(|entry| entry.id.as_str() == "old-preferences"),
        "未执行删除时旧主题仍存在，不能谎报成功"
    );
}

/// 未完整读取的合并明确拒绝后，补读可用原操作身份重试，不应被误认为结果待核验。
///
/// 数据构造：seed 版本 1，两个偏好；第一次只读保留主题，第二次补读旧主题。
/// 执行过程：1. 用 s:merge 发起未授权合并并收到 Rejected。2. 补读旧主题。
///   3. 用相同 s:merge 身份和相同候选重试，再核对当前主题。
/// 预期结果：重试保存到版本=seed(1)+merge(1)=2；新主题存在，旧主题不存在。
#[test]
fn rejected_unread_merge_can_retry_after_reading_with_the_same_identity() {
    let temp = tempfile::tempdir().expect("独立数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = seed(&paths);
    read(&mut access, "preferences", "s:read-current");
    assert!(
        matches!(
            access.execute(merge(), &context("s:merge")),
            Err(agent::AdapterError::Rejected(_))
        ),
        "未读旧主题时应明确拒绝"
    );
    read(&mut access, "old-preferences", "s:read-old");
    let saved = access
        .execute(merge(), &context("s:merge"))
        .expect("明确拒绝不得占用待核验身份");
    assert!(saved.saved, "补读后的合法合并应成功");
    assert_eq!(saved.revision.number(), 2, "合法合并只发布一次");
    let snapshot = access
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("当前主题");
    assert!(
        snapshot
            .entries()
            .any(|entry| entry.id.as_str() == "preferences"),
        "保留主题必须存在"
    );
    assert!(
        !snapshot
            .entries()
            .any(|entry| entry.id.as_str() == "old-preferences"),
        "合并成功后旧主题应不存在"
    );
}

/// 不存在的删除目标明确失败，其他写入发布相同正文后也不能让非法合并返回 saved。
///
/// 数据构造：seed 版本 1；非法合并删除 missing；同一 user turn 的另一操作只更新新正文。
/// 执行过程：1. 完整读保留主题。2. 发起删除不存在主题的合并。3. 另一个操作保存相同正文。
///   4. 重试原非法合并并检查目录。
/// 预期结果：非法合并始终拒绝；当前版本=seed(1)+合法写入(1)=2；原旧主题没有被删除。
#[test]
fn missing_deletion_target_cannot_be_reconciled_as_a_successful_merge() {
    let temp = tempfile::tempdir().expect("独立数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let mut access = seed(&paths);
    read(&mut access, "preferences", "s:read-current");
    let command = |supersedes: &[&str]| {
        serde_json::from_value(json!({
            "action":"write", "scope":"personal", "id":"preferences",
            "markdown":"请用简洁中文交流。构建命令为 cargo build。", "supersedes":supersedes
        }))
        .expect("写入参数")
    };
    assert!(
        access
            .execute(command(&["missing"]), &context("s:invalid"))
            .is_err(),
        "不存在的删除目标应明确失败"
    );
    assert!(
        matches!(
            access.execute(command(&["preferences"]), &context("s:self")),
            Err(agent::AdapterError::InvalidArguments(_))
        ),
        "删除目标不能包含保留主题"
    );
    assert!(
        matches!(
            access.execute(
                command(&["old-preferences", "old-preferences"]),
                &context("s:duplicate")
            ),
            Err(agent::AdapterError::InvalidArguments(_))
        ),
        "重复删除目标必须在预留操作之前拒绝"
    );
    let mut other = context("s:invalid");
    other.operation_id = Some("s:other-write".into());
    assert!(
        access
            .execute(command(&[]), &other)
            .expect("另一个合法写入")
            .saved,
        "同一原话的独立合法操作应保存正文"
    );
    assert!(
        access
            .execute(command(&["missing"]), &context("s:invalid"))
            .is_err(),
        "相同正文不能让非法删除返回 saved"
    );
    let snapshot = access
        .settings_snapshot(MemoryScopeAlias::Personal)
        .expect("当前目录");
    assert_eq!(
        snapshot.revision().number(),
        2,
        "只有合法写入可以发布版本 2"
    );
    assert!(
        snapshot
            .entries()
            .any(|entry| entry.id.as_str() == "preferences"),
        "合法保存的正文必须存在"
    );
    assert!(
        snapshot
            .entries()
            .any(|entry| entry.id.as_str() == "old-preferences"),
        "非法删除不能删除旧主题"
    );
    assert!(
        !snapshot
            .entries()
            .any(|entry| entry.id.as_str() == "missing"),
        "非法目标不能被创建"
    );
}
