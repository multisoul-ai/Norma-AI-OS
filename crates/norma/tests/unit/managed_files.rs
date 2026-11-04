use super::{atomic_write, set_mode};
use crate::private_fs::{create_private_dir, permissions_are_private};
use std::fs;

/// 场景：受管凭据被外部放宽访问权限；构造临时文件和固定合成内容；
/// 执行创建、放宽、检测、修复；预期初始/修复均为私有，放宽后不得被判为安全。
#[test]
fn managed_credential_detects_and_repairs_broader_permissions() {
    let root = tempfile::tempdir().expect("创建独立权限夹具");
    let path = root.path().join("credential.json");
    atomic_write(&path, b"synthetic-credential", 0o600).expect("创建私有凭据");
    assert!(
        permissions_are_private(&path).expect("查询初始权限"),
        "创建时必须已限制当前用户访问"
    );
    broaden_permissions(&path);
    assert!(
        !permissions_are_private(&path).expect("查询放宽后的权限"),
        "额外读取者不得被判为私有"
    );
    set_mode(&path, 0o600).expect("修复原有文件权限");
    assert!(
        permissions_are_private(&path).expect("复查修复结果"),
        "修复必须撤销额外读取权限"
    );
    assert_eq!(
        fs::read(&path).expect("读回凭据"),
        b"synthetic-credential",
        "权限修复不得轮换或损坏内容"
    );
}

/// 场景：中文/空格路径内已有受管文件；构造 old/new 两份内容；
/// 执行两次原子写；预期只保留新内容、私有权限，目录中没有遗留临时文件。
#[test]
fn managed_file_replacement_preserves_content_and_private_permissions() {
    let root = tempfile::tempdir().expect("创建原子写夹具");
    let directory = root.path().join("中文 folder");
    fs::create_dir(&directory).expect("创建特殊路径");
    let path = directory.join("credential.json");
    atomic_write(&path, b"old", 0o600).expect("首次写入");
    atomic_write(&path, b"new", 0o600).expect("替换已存在文件");
    assert_eq!(
        fs::read(&path).expect("读取结果"),
        b"new",
        "替换后应为完整新内容"
    );
    assert_ne!(
        fs::read(&path).expect("再次读取"),
        b"old",
        "不能静默保留旧内容"
    );
    assert!(
        permissions_are_private(&path).expect("检查替换权限"),
        "替换文件仍需私有访问控制"
    );
    assert_eq!(
        fs::read_dir(directory).expect("枚举夹具").count(),
        1,
        "只应留下目标文件，不得泄漏临时文件"
    );
}

/// 场景：会话标记目录首次创建与再次使用；数据为隔离临时父目录；
/// 执行两次创建；预期目录可复用且新写入内容可读，不误删既有标记。
#[test]
fn private_session_directory_is_reusable_without_losing_markers() {
    let root = tempfile::tempdir().expect("创建标记目录夹具");
    let directory = root.path().join("markers");
    create_private_dir(&directory).expect("首次创建私有目录");
    fs::write(directory.join("marker"), b"owned").expect("写入会话标记");
    create_private_dir(&directory).expect("复用既有私有目录");
    assert!(directory.is_dir(), "复用不能删除标记目录");
    assert_eq!(
        fs::read(directory.join("marker")).expect("读回标记"),
        b"owned",
        "复用不能清空会话标记"
    );
}

#[cfg(unix)]
fn broaden_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).expect("模拟组/其他用户读取");
}

/// 场景：标记目录被 junction 重定向；构造外部目标、固定标记及目录链接；
/// 执行权限查询、修复及目录复用；预期均拒绝链接，外部标记保持原样。
#[cfg(windows)]
#[test]
fn private_directory_rejects_junction_instead_of_validating_its_own_acl() {
    let root = tempfile::tempdir().expect("创建 junction 夹具");
    let target = root.path().join("outside");
    let link = root.path().join("redirect");
    fs::create_dir(&target).expect("创建外部目标");
    fs::write(target.join("marker"), b"unchanged").expect("写入目标标记");
    let output = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .output()
        .expect("创建无须符号链接特权的 junction");
    assert!(
        output.status.success(),
        "junction 反例必须真实建立：{:?}",
        output.status
    );
    assert!(
        permissions_are_private(&link).is_err(),
        "不能用链接自身的私有 ACL 证明目标私有"
    );
    assert!(
        set_mode(&link, 0o600).is_err(),
        "不得把权限修复应用于重定向链接"
    );
    assert!(
        create_private_dir(&link).is_err(),
        "会话标记目录不得接受 junction"
    );
    assert_eq!(
        fs::read(target.join("marker")).expect("读取外部标记"),
        b"unchanged",
        "拒绝路径不得修改目标内容"
    );
    fs::remove_dir(link).expect("只删除 junction 本身");
}

/// 场景：凭据文件符号链接指向其他文件；构造合成目标和文件链接；
/// 执行受管权限检查；预期拒绝链接，不把链接 ACL 当作目标凭据的权限证明。
#[cfg(windows)]
#[test]
fn credential_permissions_reject_file_symlinks() {
    let root = tempfile::tempdir().expect("创建文件链接夹具");
    let target = root.path().join("outside.json");
    let link = root.path().join("credential.json");
    fs::write(&target, b"synthetic").expect("创建合成目标");
    std::os::windows::fs::symlink_file(&target, &link)
        .expect("runner 必须真实建立符号链接反例，不得跳过");
    assert!(
        permissions_are_private(&link).is_err(),
        "符号链接不得显示凭据权限安全"
    );
    assert!(
        set_mode(&link, 0o600).is_err(),
        "不得只修改链接自身而误报已修复目标"
    );
    assert_eq!(
        fs::read(target).expect("读取目标"),
        b"synthetic",
        "拒绝链接时目标内容保持不变"
    );
}

#[cfg(windows)]
fn broaden_permissions(path: &std::path::Path) {
    let output = std::process::Command::new("icacls.exe")
        .arg(path)
        .args(["/grant", "*S-1-1-0:(R)"])
        .output()
        .expect("为合成文件授予 Everyone 读取权限");
    assert!(
        output.status.success(),
        "ACL 夹具必须成功放宽权限：{:?}",
        output.status
    );
}
