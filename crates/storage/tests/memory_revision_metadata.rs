use agent::{MemoryEntryId, MemoryMutation, MemoryRevision, MemoryScope};
use sha2::{Digest, Sha256};
use storage::{DatabasePaths, MemoryStore};

/// 场景：每轮元数据检查必须识别版本变化且验证发布身份，不读取主题正文。
/// 构造：隔离个人库发布两版，随后移走正文并逐一损坏 CURRENT/manifest。
/// 执行：读取轻量版本；模拟 digest、scope、schema、revision、parent 错误。
/// 预期：合法版本可读、完整快照不可读；任何损坏元数据均报错，不冒充空库。
#[test]
fn memory_revision_metadata_validates_publication_without_reading_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DatabasePaths::isolated_test(dir.path());
    let store = MemoryStore::open(&paths).unwrap();
    let scope = MemoryScope::Personal;
    assert_eq!(
        store.current_revision(&scope).unwrap(),
        MemoryRevision::INITIAL,
        "仅不存在 CURRENT 表示初始版本"
    );
    let mut expected = MemoryRevision::INITIAL;
    for text in ["旧偏好", "新偏好"] {
        expected = store
            .commit(
                &scope,
                expected,
                &[MemoryMutation::Upsert {
                    id: MemoryEntryId::parse("preference").unwrap(),
                    markdown: text.into(),
                    sources: vec![],
                }],
            )
            .unwrap()
            .revision;
        assert_eq!(
            store.current_revision(&scope).unwrap(),
            expected,
            "必须识别外部发布的新版本"
        );
    }
    let root = store.root().join("personal");
    let revision = root.join(format!("revisions/{:020}", expected.number()));
    std::fs::rename(revision.join("topics"), revision.join("hidden-topics")).unwrap();
    assert_eq!(
        store.current_revision(&scope).unwrap(),
        expected,
        "轻量版本不能依赖正文可读"
    );
    assert!(store.snapshot(&scope).is_err(), "完整快照仍必须检查正文");
    let pointer_path = root.join("CURRENT");
    let pointer_bytes = std::fs::read(&pointer_path).unwrap();
    let pointer: serde_json::Value = serde_json::from_slice(&pointer_bytes).unwrap();
    let manifest_path = revision.join("manifest.json");
    let manifest_bytes = std::fs::read(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    std::fs::write(&manifest_path, b"{}").unwrap();
    assert!(
        store.current_revision(&scope).is_err(),
        "digest 不匹配必须拒绝"
    );
    for field in ["scope", "schema", "revision", "parent"] {
        let mut changed = manifest.clone();
        match field {
            "scope" => {
                changed[field] = serde_json::to_value(MemoryScope::ProjectShared {
                    project_id: "foreign".into(),
                })
                .unwrap()
            }
            "schema" => changed[field] = 99.into(),
            "revision" => changed[field] = 0.into(),
            "parent" => changed[field] = pointer.clone(),
            _ => unreachable!(),
        }
        let bytes = serde_json::to_vec(&changed).unwrap();
        let mut updated_pointer = pointer.clone();
        updated_pointer["manifest_sha256"] = format!("{:x}", Sha256::digest(&bytes)).into();
        std::fs::write(&manifest_path, bytes).unwrap();
        std::fs::write(&pointer_path, serde_json::to_vec(&updated_pointer).unwrap()).unwrap();
        assert!(
            store.current_revision(&scope).is_err(),
            "非法 {field} 即使 digest 匹配也必须拒绝"
        );
    }
    std::fs::write(&manifest_path, manifest_bytes).unwrap();
    for bad in [
        b"bad".to_vec(),
        b"{\"revision\":0,\"manifest_sha256\":\"x\"}".to_vec(),
    ] {
        std::fs::write(&pointer_path, bad).unwrap();
        assert!(
            store.current_revision(&scope).is_err(),
            "损坏指针或零版本不可视为空库"
        );
    }
    std::fs::write(&pointer_path, pointer_bytes).unwrap();
    assert_eq!(
        store.current_revision(&scope).unwrap(),
        expected,
        "恢复合法元数据后版本可读"
    );
}
