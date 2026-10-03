use agent::{MemoryEntryId, MemoryMutation, MemoryRevision, MemoryScope};
use std::os::unix::fs::{symlink, PermissionsExt};
use storage::{DatabasePaths, MemoryError, MemoryStore};

fn put(id: &MemoryEntryId, body: &str) -> MemoryMutation {
    MemoryMutation::Upsert {
        id: id.clone(),
        markdown: body.into(),
        sources: vec![],
    }
}

/// 发布前文件系统拒绝写指针时，旧活动版本完整保留，重开后可继续提交。
///
/// 数据构造：
///   - 个人库 V1 保存“旧偏好”。scope 目录从 0700 改为 0500：保留读取/遍历，移除写权限。
///   - revisions 子目录仍为 0700，因此候选可准备，最终 CURRENT 临时文件不能创建。
///
/// 执行过程：
///   1. 去除 scope 写权限，基于 V1 提交“未发布偏好”，预期 IO 失败。
///   2. 恢复 0700 并重开，读取旧快照，再提交“新偏好”。
///
/// 预期结果：
///   - 正断言：发布失败后 V1 原文与目录可读，恢复权限后新提交成功。
///   - 负断言：失败的候选不得成为活动版本，不得复用失败候选的版本号。
#[test]
fn publication_failure_keeps_previous_revision_and_allows_retry() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let store = MemoryStore::open(&paths).expect("应打开记忆库");
    let scope = MemoryScope::Personal;
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    let first = store
        .commit(&scope, MemoryRevision::INITIAL, &[put(&id, "旧偏好")])
        .expect("V1 应发布");
    let personal = store.root().join("personal");
    std::fs::set_permissions(&personal, std::fs::Permissions::from_mode(0o500))
        .expect("应移除发布目录写权限");
    let failed = store.commit(&scope, first.revision, &[put(&id, "未发布偏好")]);
    std::fs::set_permissions(&personal, std::fs::Permissions::from_mode(0o700))
        .expect("应恢复发布目录写权限");
    assert!(
        matches!(failed, Err(MemoryError::Io(ref error)) if error.kind() == std::io::ErrorKind::PermissionDenied),
        "指针发布权限失败必须明确返回 IO 错误"
    );
    let reopened = MemoryStore::open(&paths).expect("失败后应重开");
    let active = reopened.snapshot(&scope).expect("旧活动版本应仍可读");
    assert_eq!(
        active.revision(),
        first.revision,
        "失败候选不得替换 CURRENT"
    );
    assert_eq!(
        reopened
            .read(&active, &id)
            .expect("旧主题应可读")
            .expect("主题应存在")
            .markdown,
        "旧偏好",
        "失败不得覆盖原正文"
    );
    let retried = reopened
        .commit(&scope, active.revision(), &[put(&id, "新偏好")])
        .expect("恢复权限后应可提交");
    // V1=1，失败准备占用 V2=2，重试必须分配 >2，以免旧请求复用候选身份。
    assert!(
        retried.revision.number() > first.revision.number() + 1,
        "重试不得复用未发布目录的身份"
    );
    let latest = reopened.snapshot(&scope).expect("重试后应可读");
    assert_eq!(
        reopened
            .read(&latest, &id)
            .expect("新主题应可读")
            .expect("主题应存在")
            .markdown,
        "新偏好",
        "重试应发布正确正文"
    );
}

/// 中断留下的半成品目录不能参与普通读取或回滚，重开后跳过其版本号。
///
/// 数据构造：
///   - V1 是已发布主题；磁盘手工构造 V2/topics/orphan.md，模拟候选准备中断。
///   - V2 缺少 manifest 和 MEMORY.md，CURRENT 仍指向 V1。
///
/// 执行过程：
///   1. 重开 Store，读取普通快照并尝试恢复未发布 V2。
///   2. 基于 V1 提交新正文，检查新版本跨过 V2。
///
/// 预期结果：
///   - 正断言：V1 仍完整可读，新提交成功。
///   - 负断言：orphan 不得出现，未发布 V2 不得成为回滚来源。
#[test]
fn interrupted_preparation_is_never_a_read_or_restore_source() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let store = MemoryStore::open(&paths).expect("应打开记忆库");
    let scope = MemoryScope::Personal;
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    let first = store
        .commit(&scope, MemoryRevision::INITIAL, &[put(&id, "已发布偏好")])
        .expect("V1 应发布");
    let orphan_number = first.revision.number() + 1; // 下一次准备先预留 V1 + 1 = V2。
    let orphan = store
        .root()
        .join("personal/revisions")
        .join(format!("{orphan_number:020}"))
        .join("topics");
    std::fs::create_dir_all(&orphan).expect("应构造中断的准备目录");
    std::fs::write(orphan.join("orphan.md"), "中断文件").expect("应构造未发布正文");
    let reopened = MemoryStore::open(&paths).expect("应模拟重启后的 Store");
    let active = reopened.snapshot(&scope).expect("应读取最后发布版本");
    assert_eq!(
        active.revision(),
        first.revision,
        "普通快照不得选取目录中最大的未发布版本"
    );
    let orphan_id = MemoryEntryId::parse("orphan").expect("主题 ID 应有效");
    assert!(
        reopened
            .read(&active, &orphan_id)
            .expect("查询应成功")
            .is_none(),
        "中断正文不得参与有效记忆读取"
    );
    assert!(
        matches!(
            reopened.restore(
                &scope,
                active.revision(),
                MemoryRevision::new(orphan_number)
            ),
            Err(MemoryError::RevisionNotFound)
        ),
        "未发布候选不得成为恢复来源"
    );
    let next = reopened
        .commit(&scope, active.revision(), &[put(&id, "重启后的偏好")])
        .expect("重启后应能继续发布");
    assert!(
        next.revision.number() > orphan_number,
        "新提交必须跳过中断版本号"
    );
    assert!(
        !reopened
            .snapshot(&scope)
            .expect("新快照应可读")
            .catalog()
            .contains("orphan"),
        "中断文件不得进入新目录"
    );
}

/// 超预算、畸形主题和清空候选均不能破坏已有集合，明确删除一个主题则有效。
///
/// 数据构造：
///   - 初始集合含 preferences 与 conventions 两个主题。
///   - 超大正文长度 = MAX_TOPIC_BYTES(256000) + 1 = 256001 ASCII 字节。
///   - 清空候选显式删除两个主题；后续仅删除 conventions。
///
/// 执行过程：
///   1. 分别提交超大正文、缺失结束 frontmatter 的正文、空集合。
///   2. 检查版本未变化，再删除 conventions，读取新目录及 preferences。
///
/// 预期结果：
///   - 正断言：preferences 保留，单主题删除发布新版本。
///   - 负断言：失败候选不推进版本，conventions 从有效快照和目录消失。
#[test]
fn invalid_and_empty_candidates_preserve_live_memory() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let store =
        MemoryStore::open(&DatabasePaths::isolated_test(temp.path())).expect("应打开记忆库");
    let scope = MemoryScope::Personal;
    let preferences = MemoryEntryId::parse("preferences").expect("偏好 ID 应有效");
    let conventions = MemoryEntryId::parse("conventions").expect("约定 ID 应有效");
    let first = store
        .commit(
            &scope,
            MemoryRevision::INITIAL,
            &[
                put(&preferences, "偏好中文"),
                put(&conventions, "使用 Rust"),
            ],
        )
        .expect("初始集合应发布");
    let oversized = "x".repeat(storage::memory::MAX_TOPIC_BYTES + 1);
    assert!(
        matches!(
            store.commit(&scope, first.revision, &[put(&preferences, &oversized)]),
            Err(MemoryError::TooLarge)
        ),
        "单主题超过字节预算必须被拒绝"
    );
    assert!(
        matches!(
            store.commit(
                &scope,
                first.revision,
                &[put(&preferences, "---\nname: 未完成\n")]
            ),
            Err(MemoryError::InvalidDocument(_))
        ),
        "未完成 frontmatter 必须被拒绝"
    );
    let empty = [
        MemoryMutation::Delete {
            id: preferences.clone(),
        },
        MemoryMutation::Delete {
            id: conventions.clone(),
        },
    ];
    assert!(
        matches!(
            store.commit(&scope, first.revision, &empty),
            Err(MemoryError::EmptyCandidate)
        ),
        "空候选不得清空既有记忆"
    );
    assert_eq!(
        store.snapshot(&scope).expect("失败后应可读").revision(),
        first.revision,
        "三个失败候选均不得推进版本"
    );
    let deletion = store
        .commit(
            &scope,
            first.revision,
            &[MemoryMutation::Delete {
                id: conventions.clone(),
            }],
        )
        .expect("删除一个主题应成功");
    let active = store.snapshot(&scope).expect("删除后应可读");
    assert_eq!(
        active.revision(),
        deletion.revision,
        "删除后的目录和正文应属于新版本"
    );
    assert_eq!(
        store
            .read(&active, &preferences)
            .expect("偏好应可读")
            .expect("偏好应存在")
            .markdown,
        "偏好中文",
        "删除约定不得影响偏好正文"
    );
    assert!(
        store
            .read(&active, &conventions)
            .expect("缺失查询应成功")
            .is_none(),
        "已删除主题不得再从有效快照读取"
    );
    assert!(
        !active.catalog().contains("conventions.md"),
        "生成目录必须去除被删除主题"
    );
}

/// IO 边界拒绝指向外部正文的符号链接，已有内存快照保持原始版本。
///
/// 数据构造：
///   - V1 保存 preferences；将该磁盘主题替换为指向临时根外部 sentinel.md 的 symlink。
///   - sentinel 正文与主题相同，使单纯校验正文哈希无法代替 symlink 拒绝。
///
/// 执行过程：
///   1. 替换文件后请求新快照，预期失败。
///   2. 从已有快照读取原主题，并检查 sentinel 原文未变。
///
/// 预期结果：
///   - 正断言：原快照仍是原正文，外部文件保持原样。
///   - 负断言：新快照不得跟随 symlink 或返回外部正文。
#[test]
fn symlink_topic_cannot_escape_the_revision() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let store =
        MemoryStore::open(&DatabasePaths::isolated_test(temp.path())).expect("应打开记忆库");
    let scope = MemoryScope::Personal;
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    let first = store
        .commit(&scope, MemoryRevision::INITIAL, &[put(&id, "原始偏好")])
        .expect("主题应发布");
    let old = store.snapshot(&scope).expect("应固定原始快照");
    let external = temp.path().join("sentinel.md");
    std::fs::write(&external, "原始偏好").expect("应构造与主题相同的外部文件");
    let topic = store
        .root()
        .join("personal/revisions")
        .join(format!("{:020}", first.revision.number()))
        .join("topics/preferences.md");
    std::fs::remove_file(&topic).expect("应移除测试主题");
    symlink(&external, &topic).expect("应注入逃逸 symlink");
    assert!(
        store.snapshot(&scope).is_err(),
        "新快照必须拒绝跟随主题 symlink"
    );
    assert_eq!(
        store
            .read(&old, &id)
            .expect("固定快照应可读")
            .expect("主题应存在")
            .markdown,
        "原始偏好",
        "固定快照不得读取替换后的外部文件"
    );
    assert_eq!(
        std::fs::read_to_string(&external).expect("应读取测试 sentinel"),
        "原始偏好",
        "外部 sentinel 不得被存储操作修改"
    );
}
