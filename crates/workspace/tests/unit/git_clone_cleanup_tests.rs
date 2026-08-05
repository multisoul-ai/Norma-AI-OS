#[cfg(unix)]
use super::{
    validate_clone_destination, CloneCancellation, CloneEvent, CloneFailureKind, GitCloneRequest,
    GitCloneRunner,
};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// GC-C1：Git 运行期间目标被替换为符号链接时，失败清理必须停止，不能递归删除链接目标。
///
/// 数据构造（含关键数值的推导过程）：
///   Git 参数位置 = command(1) + option(1) + separator(1) + remote(1) + target(1)，故 target=$5。
///   外部保护文件 = outside/keep.txt(1)；允许删除数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Runner 创建并记录 target 的 device/inode。
///   2. fake Git 删除空 target，将其替换为指向 outside 的符号链接，然后返回失败。
///   3. Runner 在清理前读取 symlink metadata，发现目标不再是原目录并停止递归删除。
///
/// 预期结果：
///   - 正断言：返回 GitFailed，outside/keep.txt 与替换后的符号链接均保留。
///   - 负断言：不得把链接目标当成本任务目录删除，也不得发送 Completed。
#[cfg(unix)]
#[test]
fn git_clone_cleanup_preserves_target_replaced_by_symlink() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let projects = temp.path().join("projects");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&projects).expect("测试应创建 clone 父目录");
    std::fs::create_dir(&outside).expect("测试应创建不能被清理的 outside 目录");
    std::fs::write(outside.join("keep.txt"), b"keep").expect("测试应写入 outside 保护文件");
    let fake_git = temp.path().join("replace-target-git.sh");
    let script = format!(
        "#!/bin/sh\nrmdir \"$5\"\nln -s \"{}\" \"$5\"\nexit 1\n",
        outside.display()
    );
    std::fs::write(&fake_git, script).expect("测试应写入替换 target 的 fake Git");
    let mut permissions = std::fs::metadata(&fake_git)
        .expect("fake Git metadata 应可读取")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_git, permissions).expect("fake Git 应设置为当前用户可执行");
    let destination = validate_clone_destination(&projects, "replaced")
        .expect("尚不存在的 replaced 目标应通过校验");
    let request = GitCloneRequest::for_url_for_test("ignored-source".to_string(), destination);
    let mut events = Vec::new();

    let error = GitCloneRunner::with_binary_for_test(fake_git)
        .run(request, &CloneCancellation::new(), |event| {
            events.push(event)
        })
        .expect_err("替换 target 后 fake Git 的非零退出必须返回失败");
    let replaced = projects.join("replaced");

    assert_eq!(
        error.kind(),
        CloneFailureKind::GitFailed,
        "未知 fake Git 失败应稳定收口为 GitFailed"
    );
    assert!(
        replaced
            .symlink_metadata()
            .expect("替换后的 target 符号链接应保留")
            .file_type()
            .is_symlink(),
        "目录 identity 改变后清理必须保留替换后的符号链接"
    );
    assert!(
        outside.join("keep.txt").is_file(),
        "失败清理不得沿符号链接删除 outside/keep.txt"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CloneEvent::Started { .. })),
        "fake Git 已启动时必须发送 Started 事件"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, CloneEvent::Completed { .. })),
        "Git 失败且目标被替换时不得发送 Completed 事件"
    );
}
