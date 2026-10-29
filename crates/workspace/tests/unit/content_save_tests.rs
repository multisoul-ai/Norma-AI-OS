use super::*;
use std::io::Write;

/// 场景：最终替换之前，外部编辑器已经保存另一版本。
/// 数据构造：用户看到 before，准备 draft 后把磁盘改为 external。
/// 执行过程：原子交换并校验被置换的真实文件。
/// 预期结果：返回冲突，磁盘恢复 external，草稿副本保留供恢复。
#[test]
fn exchange_preserves_external_edit_arriving_before_replace() {
    let temp = tempfile::tempdir().expect("目录");
    let target = temp.path().join("a.txt");
    std::fs::write(&target, "external").expect("外部版本");
    let mut draft = tempfile::NamedTempFile::new_in(temp.path()).expect("草稿");
    draft.write_all(b"draft").expect("写草稿");
    let error = replace_checked(draft, &target, &fingerprint(b"before")).expect_err("必须报告冲突");
    assert!(
        matches!(error, ContentError::SaveConflictRestored { .. }),
        "冲突错误必须保留恢复副本语义"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("磁盘"),
        "external",
        "恢复实际外部版本"
    );
    let recovery = std::fs::read_dir(temp.path())
        .expect("目录")
        .filter_map(|e| e.ok())
        .find(|e| e.path() != target)
        .expect("保留恢复副本");
    assert_eq!(
        std::fs::read(recovery.path()).expect("副本"),
        b"draft",
        "草稿不能丢失"
    );
}
