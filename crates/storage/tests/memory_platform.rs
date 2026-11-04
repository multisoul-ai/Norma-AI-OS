use agent::{MemoryEntryId, MemoryMutation, MemoryRevision, MemoryScope};
use storage::{DatabasePaths, MemoryStore};

/// 场景：默认平台真实发布和恢复记忆；数据为临时个人库及中文正文。
/// 执行：保存两次、重开并读取，再把 CURRENT 换成外部文件的硬链接。
/// 预期：正文和版本保持一致；硬链接不得作为有效受管指针读取，外部内容不变。
#[test]
fn native_publication_reopens_and_rejects_hardlinked_pointer() {
    let temp = tempfile::tempdir().expect("隔离数据根");
    let paths = DatabasePaths::isolated_test(temp.path());
    let store = MemoryStore::open(&paths).expect("默认平台应打开记忆库");
    let scope = MemoryScope::Personal;
    let id = MemoryEntryId::parse("preferences").expect("主题身份");
    let put = |body: &str| MemoryMutation::Upsert {
        id: id.clone(),
        markdown: body.into(),
        sources: vec![],
    };
    let first = store
        .commit(&scope, MemoryRevision::INITIAL, &[put("旧偏好")])
        .expect("初次发布");
    let second = store
        .commit(&scope, first.revision, &[put("中文新偏好")])
        .expect("原子替换指针");
    let reopened = MemoryStore::open(&paths).expect("重开");
    let snapshot = reopened.snapshot(&scope).expect("读取发布版本");
    assert_eq!(
        snapshot.revision(),
        second.revision,
        "重开必须保留实际活动版本"
    );
    assert_ne!(snapshot.revision(), first.revision, "旧指针不得残留");
    assert_eq!(
        reopened
            .read(&snapshot, &id)
            .expect("读取")
            .expect("正文")
            .markdown,
        "中文新偏好",
        "正文必须完整持久保存"
    );
    let current = store.root().join("personal/CURRENT");
    let original = std::fs::read(&current).expect("真实有效指针");
    let external = temp.path().join("external.json");
    std::fs::write(&external, &original).expect("构造有效外部内容");
    std::fs::remove_file(&current).expect("移除原指针");
    std::fs::hard_link(&external, &current).expect("构造真实硬链接");
    assert!(reopened.snapshot(&scope).is_err(), "多链接受管文件必须拒绝");
    assert_eq!(
        std::fs::read(&external).expect("外部内容"),
        original,
        "拒绝不能改写外部文件"
    );
}
