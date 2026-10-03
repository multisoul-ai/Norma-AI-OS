use agent::{MemoryEntryId, MemoryMutation, MemoryRevision, MemoryScope};
use std::os::unix::fs::symlink;
use storage::{DatabasePaths, MemoryError, MemoryStore};

const CHILD_ROOT: &str = "NORMA_MEMORY_LOCK_PROBE_ROOT";

fn put(id: &MemoryEntryId, markdown: &str) -> MemoryMutation {
    MemoryMutation::Upsert {
        id: id.clone(),
        markdown: markdown.into(),
        sources: vec![],
    }
}

/// 另一个进程持有范围 OS 锁时，公开 commit 必须拒绝写入且不改变正文。
///
/// 数据构造：
///   - 个人库 V1 保存“原偏好”；父进程持有该范围的 LOCK 文件排他锁。
///   - 子进程打开同一隔离数据根并尝试提交“跨进程覆盖”。
///
/// 执行过程：
///   1. 父进程锁住 LOCK，启动当前测试二进制的 child_commit_respects_os_lock。
///   2. 子进程调用公共 snapshot/commit 并验证 Busy。
///   3. 父进程释放锁，检查原主题并再次提交。
///
/// 预期结果：
///   - 正断言：子进程成功完成 Busy 检查，释放锁后提交可用。
///   - 负断言：另一进程不得覆盖正文或推进活动版本。
#[test]
fn another_process_cannot_publish_while_scope_is_locked() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let store = MemoryStore::open(&paths).expect("应打开记忆库");
    let scope = MemoryScope::Personal;
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    let first = store
        .commit(&scope, MemoryRevision::INITIAL, &[put(&id, "原偏好")])
        .expect("V1 应发布");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(store.root().join("personal/LOCK"))
        .expect("应打开范围锁文件");
    lock.try_lock().expect("父进程应持有排他锁");
    let output = std::process::Command::new(std::env::current_exe().expect("应定位测试二进制"))
        .args(["--exact", "child_commit_respects_os_lock", "--nocapture"])
        .env(CHILD_ROOT, temp.path())
        .output()
        .expect("应启动独立子进程");
    drop(lock);
    assert!(
        output.status.success(),
        "独立进程必须验证 Busy，stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let current = store.snapshot(&scope).expect("原版本应可读");
    assert_eq!(
        current.revision(),
        first.revision,
        "被锁拒绝的子进程不得推进版本"
    );
    assert_eq!(
        store
            .read(&current, &id)
            .expect("主题应可读")
            .expect("主题应存在")
            .markdown,
        "原偏好",
        "子进程不得覆盖原偏好"
    );
    let next = store
        .commit(&scope, current.revision(), &[put(&id, "锁已释放")])
        .expect("释放锁后应可提交");
    assert_ne!(
        next.revision, first.revision,
        "释放锁后必须能发布新的有效版本"
    );
}

/// 独立进程锁探针：仅在父测试指定隔离根时验证 commit 的 OS 锁边界。
///
/// 数据构造：
///   - 根目录由 NORMA_MEMORY_LOCK_PROBE_ROOT 指定，父进程已持有个人范围 LOCK。
///   - 新正文“跨进程覆盖”用于检测是否错误绕过跨进程锁。
///
/// 执行过程：
///   1. 没有专用环境变量时直接返回，避免接触真实 App 数据。
///   2. 有变量时打开 Store，取得当前版本并尝试提交。
///
/// 预期结果：
///   - 正断言：commit 返回 Busy。
///   - 负断言：正文和活动版本都没有变化。
#[test]
fn child_commit_respects_os_lock() {
    let Some(root) = std::env::var_os(CHILD_ROOT) else {
        return;
    };
    let store = MemoryStore::open(&DatabasePaths::isolated_test(std::path::Path::new(&root)))
        .expect("子进程应打开隔离库");
    let scope = MemoryScope::Personal;
    let current = store.snapshot(&scope).expect("子进程应能只读活动版本");
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    assert!(
        matches!(
            store.commit(&scope, current.revision(), &[put(&id, "跨进程覆盖")]),
            Err(MemoryError::Busy)
        ),
        "独立进程必须受 OS 排他锁约束"
    );
    let unchanged = store.snapshot(&scope).expect("拒绝后应可读");
    assert_eq!(
        unchanged.revision(),
        current.revision(),
        "子进程被拒绝后不得推进活动版本"
    );
    assert_eq!(
        store
            .read(&unchanged, &id)
            .expect("主题应可读")
            .expect("主题应存在")
            .markdown,
        "原偏好",
        "子进程拒绝后原主题不得被覆盖"
    );
}

/// 范围目录和 CURRENT 指针的符号链接均不得将读写引向受管根外。
///
/// 数据构造：
///   - 个人范围原本为空，将 personal 指向外部 sentinel 目录。
///   - 随后建立真实 personal 并发布，将有效 CURRENT 复制到外部 pointer.json 再链接。
///
/// 执行过程：
///   1. 尝试向符号链接范围提交，预期失败。
///   2. 移除目录链接，创建真实空范围，再注入指针链接并请求快照。
///
/// 预期结果：
///   - 正断言：外部 sentinel 文件保留原样。
///   - 负断言：范围 symlink 不得产生外部 revisions，指针 symlink 不得读取成功。
#[test]
fn symlink_scope_and_pointer_are_rejected() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let store =
        MemoryStore::open(&DatabasePaths::isolated_test(temp.path())).expect("应打开记忆库");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&outside).expect("应创建外部目录");
    let sentinel = outside.join("pointer.json");
    std::fs::write(&sentinel, "外部指针占位").expect("应创建外部 sentinel");
    let personal = store.root().join("personal");
    symlink(&outside, &personal).expect("应创建范围逃逸链接");
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    assert!(
        store
            .commit(
                &MemoryScope::Personal,
                MemoryRevision::INITIAL,
                &[put(&id, "越界写入")]
            )
            .is_err(),
        "范围符号链接必须在 IO 边界被拒绝"
    );
    assert!(
        !outside.join("revisions").exists(),
        "不得在链接指向的外部范围准备任何版本"
    );
    std::fs::remove_file(&personal).expect("应移除测试范围链接");
    store
        .commit(
            &MemoryScope::Personal,
            MemoryRevision::INITIAL,
            &[put(&id, "原偏好")],
        )
        .expect("应在真实范围发布有效主题");
    let pointer = std::fs::read(personal.join("CURRENT")).expect("应读取有效指针作为外部 fixture");
    std::fs::write(&sentinel, &pointer).expect("应构造有效外部指针；不能靠 JSON 错误伪装路径防护");
    std::fs::remove_file(personal.join("CURRENT")).expect("应移除测试指针");
    symlink(&sentinel, personal.join("CURRENT")).expect("应创建指针链接");
    assert!(
        store.snapshot(&MemoryScope::Personal).is_err(),
        "CURRENT 符号链接必须被拒绝"
    );
    assert_eq!(
        std::fs::read(&sentinel).expect("应读测试 sentinel"),
        pointer,
        "外部指针不得被写入修改"
    );
}

/// 校验值拒绝被修改的目录/正文，同时 Test 数据根不会落入 Production 目录。
///
/// 数据构造：
///   - fake-home 使用 local_test_for_home，只创建 Norma OS Test/norma/memory。
///   - V1 主题 preferences；外部故意把 MEMORY.md 改为“不完整目录”。
///
/// 执行过程：
///   1. 按 Test DatabasePaths 写入 V1，检查生产目录没有创建。
///   2. 破坏目录内容，要求新的快照拒绝完整性错误。
///
/// 预期结果：
///   - 正断言：记忆位于显式 Test 数据根。
///   - 负断言：真实/模拟生产记忆不被创建，损坏目录不得成功读取。
#[test]
fn test_database_paths_and_revision_integrity_are_enforced() {
    let temp = tempfile::tempdir().expect("应创建隔离 HOME");
    let paths = DatabasePaths::local_test_for_home(temp.path());
    let store = MemoryStore::open(&paths).expect("应打开 Test 记忆库");
    let expected = temp
        .path()
        .canonicalize()
        .expect("应规范化测试 HOME")
        .join("Library/Application Support/Norma OS Test/norma/memory");
    assert_eq!(
        store.root(),
        expected,
        "必须沿用显式 Test DatabasePaths 的数据根"
    );
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    let first = store
        .commit(
            &MemoryScope::Personal,
            MemoryRevision::INITIAL,
            &[put(&id, "测试偏好")],
        )
        .expect("Test 主题应发布");
    let production = temp
        .path()
        .join("Library/Application Support/Norma OS/norma/memory");
    assert!(
        !production.exists(),
        "Test 模式不得创建 Production 记忆目录"
    );
    let catalog = store
        .root()
        .join("personal/revisions")
        .join(format!("{:020}", first.revision.number()))
        .join("MEMORY.md");
    std::fs::write(catalog, "不完整目录").expect("应注入目录损坏");
    assert!(
        matches!(
            store.snapshot(&MemoryScope::Personal),
            Err(MemoryError::CorruptRevision)
        ),
        "损坏目录不得和完整正文混合成有效快照"
    );
}
