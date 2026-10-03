use agent::{MemoryEntryId, MemoryMutation, MemoryRevision, MemoryScope};
use storage::{DatabasePaths, MemoryStore};

/// 进程重启复用旧 PID 时，残留的临时指针不会阻止第一次新提交。
///
/// 数据构造：
///   - 独立测试二进制仅有此测试，尚未调用 commit。
///   - scope 中预先保留 .CURRENT-<本进程PID>-1，模拟旧进程在 rename 前中断。
///   - 新正文为“重启后的偏好”，初始有效版本仍为空库。
///
/// 执行过程：
///   1. 准备旧 PID/首次序号的残留文件，重新打开 Store。
///   2. 执行第一次 commit，随后重开并读取主题及活动版本。
///
/// 预期结果：
///   - 正断言：第一次新提交成功，主题正确且可持久读取。
///   - 负断言：旧临时指针不得成为有效快照或阻止发布。
#[test]
fn stale_pointer_from_a_reused_process_id_does_not_block_publication() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let store = MemoryStore::open(&paths).expect("应打开空库");
    let scope = MemoryScope::Personal;
    let empty = store.snapshot(&scope).expect("应创建个人范围");
    let stale = store
        .root()
        .join("personal")
        .join(format!(".CURRENT-{}-1", std::process::id()));
    std::fs::write(&stale, "中断的旧临时指针").expect("应构造旧进程残留");
    let reopened = MemoryStore::open(&paths).expect("应模拟重新打开");
    let id = MemoryEntryId::parse("preferences").expect("主题 ID 应有效");
    let change = MemoryMutation::Upsert {
        id: id.clone(),
        markdown: "重启后的偏好".into(),
        sources: vec![],
    };
    let receipt = reopened
        .commit(&scope, empty.revision(), &[change])
        .expect("残留临时指针不得阻止第一次提交");
    let final_store = MemoryStore::open(&paths).expect("发布后应能重开");
    let snapshot = final_store.snapshot(&scope).expect("应读取完整有效版本");
    assert_eq!(
        snapshot.revision(),
        receipt.revision,
        "重开应定位本次成功提交"
    );
    assert_ne!(
        snapshot.revision(),
        MemoryRevision::INITIAL,
        "旧临时指针不得让已发布集合仍被当成空库"
    );
    assert_eq!(
        final_store
            .read(&snapshot, &id)
            .expect("主题应可读")
            .expect("主题应存在")
            .markdown,
        "重启后的偏好",
        "残留指针不得替换本次正文"
    );
    assert!(
        !snapshot.catalog().contains("中断的旧临时指针"),
        "残留指针不得参与目录生成"
    );
}
