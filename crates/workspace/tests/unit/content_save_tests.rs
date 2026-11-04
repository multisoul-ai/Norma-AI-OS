use super::*;
use std::io::Write;

#[cfg(windows)]
#[path = "../support/windows_privileges.rs"]
mod windows_privileges;

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

/// 场景：ACL同时阻止冲突回滚和keep的属性更新，旧版本不能被自动清理。
/// 构造：目标external拒绝WRITE_ATTRIBUTES，owner仍有DELETE和WRITE_DAC。
/// 执行：真实原子交换、回滚和keep；预期Recovery明确报告旧版本路径。
/// 预期：目标仍为完整draft、恢复副本为完整external；不能伪报Restored或删掉任一正文。
#[cfg(windows)]
#[test]
fn failed_rollback_and_keep_preserve_both_versions() {
    let temp = tempfile::tempdir().expect("隔离ACL夹具");
    let _access = windows_privileges::WithoutTokenPrivileges::new();
    let target = temp.path().join("a.txt");
    std::fs::write(&target, "external").expect("外部版本");
    deny_write_attributes(&target);
    let mut draft = tempfile::NamedTempFile::new_in(temp.path()).expect("草稿");
    let recovery_path = draft.path().to_path_buf();
    draft.write_all(b"draft").expect("写草稿");
    let error = replace_checked(draft, &target, &fingerprint(b"before"))
        .expect_err("回滚和keep失败不能变成成功保存");
    assert!(
        matches!(&error, ContentError::SaveConflictRecovery { recovery, detail }
            if recovery == &recovery_path && detail.contains("os error 5")),
        "必须保留旧版本路径和属性拒绝诊断：{error:?}"
    );
    assert!(
        !matches!(&error, ContentError::SaveConflictRestored { .. }),
        "ACL阻止恢复时不能承诺目标已恢复"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("目标仍在"),
        "draft",
        "回滚失败时目标保留已写入的草稿，不能伪造恢复状态"
    );
    assert_eq!(
        std::fs::read(&recovery_path).expect("keep失败不能删除旧版本"),
        b"external",
        "错误路径必须确实包含完整外部版本"
    );
}

/// 场景：回滚已成功，只在归档草稿时拒绝属性更新；不能混淆副本角色。
/// 构造：目标为已恢复external，TempPath为draft，仅草稿的真实DACL拒绝WRITE_ATTRIBUTES。
/// 执行：调用实际归档I/O边界conflict_result，传入已成功的rollback状态。
/// 预期：报告Restored并保留属性警告，target为external、recovery为draft，两份均不被删除。
#[cfg(windows)]
#[test]
fn restored_conflict_keeps_draft_when_attributes_cannot_be_persisted() {
    let temp = tempfile::tempdir().expect("隔离keep失败");
    let _access = windows_privileges::WithoutTokenPrivileges::new();
    let target = temp.path().join("a.txt");
    std::fs::write(&target, "external").expect("已恢复的外部版本");
    let mut draft = tempfile::NamedTempFile::new_in(temp.path()).expect("归档草稿");
    draft.write_all(b"draft").expect("草稿正文");
    let path = draft.into_temp_path();
    let recovery_path = path.to_path_buf();
    deny_write_attributes(&recovery_path);
    let error = conflict_result(path, Ok(())).expect_err("保存冲突仍需显式报告");
    assert!(
        matches!(&error, ContentError::SaveConflictRestored { recovery, .. } if recovery == &recovery_path),
        "磁盘已恢复时必须准确标明草稿副本角色：{error:?}"
    );
    assert!(
        error.to_string().contains("os error 5"),
        "属性失败不能被吞掉：{error:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("恢复目标"),
        "external",
        "磁盘版本保持已恢复"
    );
    assert_eq!(
        std::fs::read(&recovery_path).expect("保留草稿副本"),
        b"draft",
        "属性失败不能删除唯一草稿"
    );
}

#[cfg(windows)]
fn deny_write_attributes(target: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree, INVALID_HANDLE_VALUE},
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW, SetFileSecurityW,
            DACL_SECURITY_INFORMATION,
        },
        Storage::FileSystem::{
            CreateFileW, GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_NORMAL,
            FILE_ATTRIBUTE_TEMPORARY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            FILE_WRITE_ATTRIBUTES, INVALID_FILE_ATTRIBUTES, OPEN_EXISTING,
        },
    };
    // 0x100 is FILE_WRITE_ATTRIBUTES; FA retains DELETE and WRITE_DAC for the owner.
    let path: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let attributes = unsafe { GetFileAttributesW(path.as_ptr()) };
    assert_ne!(
        attributes, INVALID_FILE_ATTRIBUTES,
        "夹具文件的真实属性必须可读"
    );
    assert_ne!(
        unsafe { SetFileAttributesW(path.as_ptr(), attributes | FILE_ATTRIBUTE_TEMPORARY) },
        0,
        "keep必须真正清除TEMPORARY，不能让NORMAL无变化操作绕过故障注入"
    );
    assert_ne!(
        unsafe { GetFileAttributesW(path.as_ptr()) } & FILE_ATTRIBUTE_TEMPORARY,
        0,
        "夹具必须回读确认TEMPORARY位确已设置"
    );
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
    let applied = unsafe { SetFileSecurityW(path.as_ptr(), DACL_SECURITY_INFORMATION, descriptor) };
    let error = std::io::Error::last_os_error();
    unsafe { LocalFree(descriptor) };
    assert_ne!(applied, 0, "必须实际应用拒绝属性的DACL：{error}");
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_WRITE_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    let open_error = std::io::Error::last_os_error();
    if handle != INVALID_HANDLE_VALUE {
        unsafe { CloseHandle(handle) };
    }
    assert_eq!(handle, INVALID_HANDLE_VALUE, "真实DACL必须拒绝写属性句柄");
    assert_eq!(
        open_error.raw_os_error(),
        Some(5),
        "拒绝句柄必须是AccessDenied"
    );
    let denied = unsafe { SetFileAttributesW(path.as_ptr(), FILE_ATTRIBUTE_NORMAL) };
    let error = std::io::Error::last_os_error();
    assert_eq!(
        denied, 0,
        "生产keep使用的实际属性更新必须被DACL拒绝，不能伪造故障"
    );
    assert_eq!(
        error.raw_os_error(),
        Some(5),
        "必须取得真实AccessDenied：{error}"
    );
}
