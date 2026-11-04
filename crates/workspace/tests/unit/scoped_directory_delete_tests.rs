use super::*;
use std::fs;

/// 场景：身份校验后的根/嵌套目录不能在递归删除前被换走。
/// 构造：带空格和中文的两层真实NTFS目录，各自取得身份后释放初始只读句柄。
/// 执行：在生产删除路径已pin每层的确定性边界尝试rename，再继续实际删除。
/// 预期：两次替换均被OS sharing violation(32)拒绝，子项与根仍能经相同句柄完整删除。
#[test]
fn scoped_delete_pins_root_and_nested_directories_after_identity_check() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("checkout 中文");
    let nested = target.join("nested space");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("owned.txt"), b"owned").unwrap();
    let expected = Directory::open(&target, false).unwrap().identity().unwrap();
    let nested_id = Directory::open(&nested, false).unwrap().identity().unwrap();
    let parent = Directory::open(temp.path(), true).unwrap();
    let mut pinned_root = false;
    let mut pinned_nested = false;
    let removed = parent
        .remove_child_tree_with("checkout 中文", expected, |file| {
            let info = information(file).unwrap();
            let id = (
                info.dwVolumeSerialNumber,
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            );
            let source = if id == expected {
                pinned_root = true;
                Some(&target)
            } else if id == nested_id {
                pinned_nested = true;
                Some(&nested)
            } else {
                None
            };
            if let Some(source) = source {
                let error = fs::rename(source, temp.path().join("parked"))
                    .expect_err("已pin目录必须拒绝并发换名");
                assert_eq!(
                    error.raw_os_error(),
                    Some(32),
                    "替换必须真实被NTFS共享模式拒绝，不能靠事后路径检查"
                );
            }
        })
        .unwrap();

    assert!(
        pinned_root && pinned_nested,
        "必须实际经过根和嵌套目录的pin边界"
    );
    assert!(removed, "身份匹配时必须完成真实递归删除");
    assert!(!target.exists(), "拒绝换名的句柄不能阻止本次合法删除");
    assert!(
        !temp.path().join("parked").exists(),
        "任何替换尝试都不能成功"
    );
}
