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
