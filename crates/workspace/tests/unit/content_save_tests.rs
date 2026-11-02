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
        "冲突错误必须保留恢复副本语义，实际为 {error:?}"
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

/// 场景：冲突已回滚，但Windows拒绝修改恢复文件属性，目录仍允许删除。
/// 构造：外部版本的DACL拒绝WRITE_ATTRIBUTES（0x100），owner保留其他完整权限。
/// 执行：真实ReplaceFileW交换/回滚保留目标DACL，TempPath::keep触发属性拒绝。
/// 预期：错误保留恢复路径，external和draft都在，不能自动清理掉唯一草稿。
#[cfg(windows)]
#[test]
fn conflict_recovery_survives_denied_keep_attributes() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW, SetFileSecurityW,
            DACL_SECURITY_INFORMATION,
        },
    };
    let temp = tempfile::tempdir().expect("隔离ACL夹具");
    let target = temp.path().join("a.txt");
    std::fs::write(&target, "external").expect("外部版本");
    // 0x100 is FILE_WRITE_ATTRIBUTES; FA retains DELETE and WRITE_DAC for the owner.
    let sddl: Vec<u16> = "D:P(D;;0x100;;;WD)(A;;FA;;;OW)\0".encode_utf16().collect();
    let mut descriptor = std::ptr::null_mut();
    assert_ne!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        },
        0,
        "DACL必须可解析：{}",
        std::io::Error::last_os_error()
    );
    let path: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let applied = unsafe { SetFileSecurityW(path.as_ptr(), DACL_SECURITY_INFORMATION, descriptor) };
    let acl_error = std::io::Error::last_os_error();
    unsafe { LocalFree(descriptor) };
    assert_ne!(applied, 0, "必须实际应用拒绝属性的DACL：{acl_error}");
    let mut draft = tempfile::NamedTempFile::new_in(temp.path()).expect("草稿");
    let recovery_path = draft.path().to_path_buf();
    draft.write_all(b"draft").expect("写草稿");
    let error = replace_checked(draft, &target, &fingerprint(b"before"))
        .expect_err("冲突与保留属性失败必须显式报告");
    assert!(
        matches!(&error, ContentError::SaveConflictRecovery { recovery, .. } if recovery == &recovery_path),
        "keep失败必须报告可恢复的草稿路径，实际为 {error:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("外部版本仍在"),
        "external",
        "回滚保留外部编辑"
    );
    assert_eq!(
        std::fs::read(&recovery_path).expect("keep失败不能删除恢复副本"),
        b"draft",
        "草稿正文必须完整保留"
    );
}
