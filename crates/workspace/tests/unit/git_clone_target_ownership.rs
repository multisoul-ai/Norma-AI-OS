use super::*;

/// 场景：clone失败或取消后应清理本次创建的目录，不能保留半成品。
/// 构造：隔离父目录、一个含partial文件的目标，以及父目录中的独立哨兵。
/// 执行：捕获目标所有权后调用生产cleanup。
/// 预期：目标及其半成品消失，父目录与树外哨兵保持完整。
#[test]
fn clone_target_cleanup_removes_owned_directory() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("checkout");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("partial"), b"owned checkout").unwrap();
    let sentinel = root.path().join("keep.txt");
    fs::write(&sentinel, b"outside checkout").unwrap();
    let owned = OwnedTarget::capture(target.clone()).unwrap();

    owned.cleanup();

    assert!(!target.exists(), "本次拥有的clone半成品必须完整移除");
    assert!(root.path().is_dir(), "清理不能删除父目录");
    assert_eq!(
        fs::read(sentinel).unwrap(),
        b"outside checkout",
        "树外文件必须保持原样"
    );
}

/// 场景：clone运行期间目标被移走，同路径的新目录已属于另一位用户操作。
/// 构造：原目标含owned文件，捕获身份后移到parked，再在旧路径创建含foreign文件的新目录。
/// 执行：在路径相同但文件系统身份不同的条件下调用生产cleanup。
/// 预期：保留新目录及foreign内容，也不追踪或删除被移走的原目录。
#[test]
fn clone_target_cleanup_preserves_replacement_directory() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("checkout");
    let parked = root.path().join("parked");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("owned.txt"), b"original clone").unwrap();
    let owned = OwnedTarget::capture(target.clone()).unwrap();
    fs::rename(&target, &parked).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(target.join("foreign.txt"), b"new owner").unwrap();

    owned.cleanup();

    assert!(
        target.is_dir(),
        "同路径的替换目录不是本次clone所有，必须保留"
    );
    assert_eq!(
        fs::read(target.join("foreign.txt")).unwrap(),
        b"new owner",
        "新所有者的内容不能被取消清理删除"
    );
    assert_eq!(
        fs::read(parked.join("owned.txt")).unwrap(),
        b"original clone",
        "不按路径跟踪用户移走的目录"
    );
    assert!(
        !target.join("owned.txt").exists(),
        "原目录与替换目录必须确实分离"
    );
}

/// 场景：create_dir后目标被替换为指向外部项目的链接，捕获所有权必须拒绝。
/// 构造：外部目录有sentinel，checkout链接指向它；Unix/Windows均使用真实目录链接。
/// 执行：调用生产capture，不启动Git或删除外部数据。
/// 预期：不能把reparse/symlink当成本次创建的普通目录，外部内容完整保留。
#[test]
fn clone_target_capture_rejects_directory_links() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel"), b"foreign project").unwrap();
    let target = root.path().join("checkout");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &target).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&outside, &target).unwrap();

    assert!(
        OwnedTarget::capture(target).is_err(),
        "链接不能取得普通clone目录的所有权"
    );
    assert_eq!(
        fs::read(outside.join("sentinel")).unwrap(),
        b"foreign project",
        "捕获失败不能修改外部项目"
    );
}

/// 场景：clone半成品中有指向外部目录的NTFS junction，取消只能删链接本身。
/// 构造：真实junction连到含sentinel的外部目录，原目标中另有本次写入的partial。
/// 执行：经capture和生产cleanup移除半成品。
/// 预期：目标完整移除；junction不能使清理递归进入外部目录。
#[cfg(windows)]
#[test]
fn clone_target_cleanup_does_not_follow_nested_junction() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("checkout");
    let outside = root.path().join("outside");
    fs::create_dir(&target).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(target.join("partial"), b"owned checkout").unwrap();
    fs::write(outside.join("sentinel"), b"outside checkout").unwrap();
    let link = target.join("junction");
    let result = Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "真实junction夹具必须创建成功：{}",
        String::from_utf8_lossy(&result.stderr)
    );

    OwnedTarget::capture(target.clone()).unwrap().cleanup();

    assert!(!target.exists(), "本次拥有的目录及junction入口应移除");
    assert_eq!(
        fs::read(outside.join("sentinel")).unwrap(),
        b"outside checkout",
        "不能跟随junction清理树外数据"
    );
}

/// 场景：只读半成品文件与树外文件是hardlink，清理不能改变树外对象属性。
/// 构造：两个目录入口指向同一真实NTFS文件，并将共享文件对象设为只读。
/// 执行：清理checkout，读取树外字节和只读属性，再仅为测试回收恢复可写。
/// 预期：本次目录删除成功；树外hardlink的数据和原只读属性都保持。
#[cfg(windows)]
#[test]
fn clone_target_cleanup_preserves_external_readonly_hardlink() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("checkout");
    fs::create_dir(&target).unwrap();
    let outside = root.path().join("outside.txt");
    fs::write(&outside, b"linked data").unwrap();
    fs::hard_link(&outside, target.join("readonly.txt")).unwrap();
    let mut permissions = fs::metadata(&outside).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&outside, permissions).unwrap();

    OwnedTarget::capture(target.clone()).unwrap().cleanup();

    let retained = target.exists();
    let contents = fs::read(&outside).unwrap();
    let mut permissions = fs::metadata(&outside).unwrap().permissions();
    let readonly = permissions.readonly();
    permissions.set_readonly(false);
    fs::set_permissions(&outside, permissions).unwrap();
    assert!(!retained, "只读半成品也必须通过句柄清理成功");
    assert_eq!(contents, b"linked data", "树外hardlink的字节必须保持完整");
    assert!(readonly, "清理不能通过修改共享文件属性来绕过只读保护");
}
