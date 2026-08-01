use super::{
    validate_clone_destination, CloneCancellation, CloneEvent, CloneFailureKind, GitCloneRequest,
    GitCloneRunner,
};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn run_git(path: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(path)
        .status()
        .expect("测试环境应能启动系统 Git");
    assert!(status.success(), "测试 Git 命令 {args:?} 应成功");
}

fn create_committed_repository(path: &Path, tracked_norma_state: bool) {
    std::fs::create_dir_all(path).expect("测试应创建源仓库目录");
    run_git(path, &["init", "--quiet"]);
    std::fs::write(path.join("README.md"), b"# Fixture\n").expect("测试应写入 README fixture");
    run_git(path, &["add", "README.md"]);
    if tracked_norma_state {
        std::fs::create_dir(path.join(".multisoul")).expect("测试应创建 tracked .multisoul 目录");
        std::fs::write(path.join(".multisoul/project.sqlite"), b"untrusted")
            .expect("测试应写入 tracked Norma 状态 fixture");
        run_git(path, &["add", ".multisoul/project.sqlite"]);
    }
    run_git(
        path,
        &[
            "-c",
            "user.name=Norma Test",
            "-c",
            "user.email=norma-test@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
}

/// GC-G1：系统 Git 成功克隆后应完成安全收口并保留结构化事件。
///
/// 数据构造（含关键数值的推导过程）：
///   源仓库 = 1；tracked README = 1；目标目录初始数量 = 0。
///   必需生命周期事件 = Started(1) + Completed(1) = 2 个。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建带一个 commit 的本地源仓库。
///   2. Runner 原子创建目标目录，并以参数数组执行 `git clone --progress --`。
///   3. 解析进度、安全检查 `.multisoul`，写本地 exclude，然后完成。
///
/// 预期结果：
///   - 正断言：返回目标路径，README 与 `.git` 均存在。
///   - 正断言：事件首尾分别为 Started、Completed。
///   - 负断言：不得创建项目 `.gitignore`，不得留下失败状态。
#[test]
fn git_clone_runner_completes_local_clone_and_security_finalize() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let source = temp.path().join("source");
    let projects = temp.path().join("projects");
    std::fs::create_dir(&projects).expect("测试应创建目标父目录");
    create_committed_repository(&source, false);
    let destination =
        validate_clone_destination(&projects, "cloned").expect("新目标路径应通过校验");
    let request =
        GitCloneRequest::for_url_for_test(source.to_string_lossy().into_owned(), destination);
    let cancellation = CloneCancellation::new();
    let mut events = Vec::new();

    let target = GitCloneRunner::system()
        .run(request, &cancellation, |event| events.push(event))
        .expect("本地 Git fixture 应成功克隆");

    assert_eq!(
        target,
        projects
            .canonicalize()
            .expect("目标父目录应可解析")
            .join("cloned"),
        "Runner 应返回 canonical parent 下的精确目标路径"
    );
    assert!(
        target.join("README.md").is_file(),
        "成功克隆后 tracked README 必须存在"
    );
    assert!(
        target.join(".git").is_dir(),
        "成功克隆后 Git 元数据必须存在"
    );
    assert!(
        matches!(events.first(), Some(CloneEvent::Started { .. })),
        "生命周期首事件必须是 Started，实际为 {events:?}"
    );
    assert!(
        matches!(events.last(), Some(CloneEvent::Completed { .. })),
        "生命周期末事件必须是 Completed，实际为 {events:?}"
    );
    assert!(
        !target.join(".gitignore").exists(),
        "克隆安全收口不得污染项目 .gitignore"
    );
}

/// GC-G2：Git 失败时只清理本任务创建的未完成目标，并返回可分类错误。
///
/// 数据构造（含关键数值的推导过程）：
///   不存在的本地源 = 1；Runner 创建的目标 = projects/failed；预存 sibling = 1。
///   失败后应清理目标数 = 1；应保留 sibling 数 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造指向不存在源路径的 clone request。
///   2. Runner 创建目标后执行 Git，Git 返回非零状态。
///   3. Runner 校验目标仍为本任务创建的目录，再清理并分类失败。
///
/// 预期结果：
///   - 正断言：返回 RepositoryNotFound 或 GitFailed。
///   - 正断言：未完成目标被删除，预存 sibling 保留。
///   - 负断言：不得发送 Completed 事件。
#[test]
fn git_clone_runner_cleans_owned_target_after_git_failure() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let projects = temp.path().join("projects");
    std::fs::create_dir(&projects).expect("测试应创建目标父目录");
    let sibling = projects.join("keep-me");
    std::fs::create_dir(&sibling).expect("测试应创建必须保留的 sibling");
    let destination = validate_clone_destination(&projects, "failed")
        .expect("失败用例的新目标路径应通过前置校验");
    let missing_source = temp.path().join("does-not-exist");
    let request = GitCloneRequest::for_url_for_test(
        missing_source.to_string_lossy().into_owned(),
        destination,
    );
    let mut events = Vec::new();

    let error = GitCloneRunner::system()
        .run(request, &CloneCancellation::new(), |event| {
            events.push(event)
        })
        .expect_err("不存在的源仓库必须导致 clone 失败");

    assert!(
        matches!(
            error.kind(),
            CloneFailureKind::RepositoryNotFound | CloneFailureKind::GitFailed
        ),
        "不存在源应分类为 RepositoryNotFound 或 GitFailed，实际为 {:?}",
        error.kind()
    );
    assert!(
        !projects.join("failed").exists(),
        "Git 失败后应清理本任务创建的 failed 目标"
    );
    assert!(
        sibling.is_dir(),
        "失败清理必须保留预存 sibling，不能扩大删除范围"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, CloneEvent::Completed { .. })),
        "Git 失败路径不得发送 Completed 事件"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CloneEvent::Started { .. })),
        "Git 已实际启动时必须留下 Started 事件"
    );
}

/// GC-G3：启动前取消应不创建目标、不启动生命周期，并稳定返回 Cancelled。
///
/// 数据构造（含关键数值的推导过程）：
///   cancellation 初始状态 = false；调用 cancel 后 = true。
///   Git 启动数 = 0；目标创建数 = 0；事件数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建有效 request 与 cancellation token。
///   2. 在调用 Runner 前先 cancel。
///   3. Runner 在文件系统和子进程副作用前检查 token。
///
/// 预期结果：
///   - 正断言：返回 Cancelled。
///   - 负断言：目标目录与任何生命周期事件都不应出现。
#[test]
fn git_clone_runner_honors_preflight_cancellation_without_side_effects() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let source = temp.path().join("source");
    let projects = temp.path().join("projects");
    std::fs::create_dir(&projects).expect("测试应创建目标父目录");
    create_committed_repository(&source, false);
    let destination =
        validate_clone_destination(&projects, "cancelled").expect("取消用例的新目标路径应通过校验");
    let request =
        GitCloneRequest::for_url_for_test(source.to_string_lossy().into_owned(), destination);
    let cancellation = CloneCancellation::new();
    cancellation.cancel();
    let mut events = Vec::new();

    let error = GitCloneRunner::system()
        .run(request, &cancellation, |event| events.push(event))
        .expect_err("启动前取消必须返回失败结果而不是继续 clone");

    assert_eq!(
        error.kind(),
        CloneFailureKind::Cancelled,
        "预取消 request 应稳定分类为 Cancelled"
    );
    assert!(
        !projects.join("cancelled").exists(),
        "预取消不得创建 cancelled 目标目录"
    );
    assert!(
        events.is_empty(),
        "预取消发生在生命周期开始前，不得发送 Started 或 Completed"
    );
    assert_ne!(
        error.kind(),
        CloneFailureKind::GitFailed,
        "预取消不得误报为 Git 执行失败"
    );
}

/// GC-G4：克隆成功但仓库跟踪 `.multisoul` 时应保留 clone 并返回安全阻断。
///
/// 数据构造（含关键数值的推导过程）：
///   源仓库 tracked 文件 = README(1) + `.multisoul/project.sqlite`(1) = 2。
///   Git clone 成功数 = 1；Workspace 自动打开许可数 = 0；保留目标数 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并提交携带 `.multisoul/project.sqlite` 的源仓库。
///   2. Runner 完成 Git clone 后执行安全收口。
///   3. 安全层发现 tracked Norma 状态，保留目标并返回 UnsafeWorkspaceState。
///
/// 预期结果：
///   - 正断言：错误分类为 UnsafeWorkspaceState，且 target_preserved=true。
///   - 正断言：克隆目录与不可信状态文件都保留。
///   - 负断言：不得发送 Completed，避免 App 自动打开。
#[test]
fn git_clone_runner_preserves_clone_but_blocks_tracked_norma_workspace_state() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let source = temp.path().join("source");
    let projects = temp.path().join("projects");
    std::fs::create_dir(&projects).expect("测试应创建目标父目录");
    create_committed_repository(&source, true);
    let destination = validate_clone_destination(&projects, "untrusted")
        .expect("安全阻断用例的新目标路径应通过校验");
    let request =
        GitCloneRequest::for_url_for_test(source.to_string_lossy().into_owned(), destination);
    let mut events = Vec::new();

    let error = GitCloneRunner::system()
        .run(request, &CloneCancellation::new(), |event| {
            events.push(event)
        })
        .expect_err("tracked .multisoul 必须阻止 clone 被标记完成");
    let target = projects.join("untrusted");

    assert_eq!(
        error.kind(),
        CloneFailureKind::UnsafeWorkspaceState,
        "tracked Norma 状态应分类为 UnsafeWorkspaceState"
    );
    assert!(
        error.target_preserved(),
        "安全阻断应明确告知 App 克隆目标已保留"
    );
    assert!(
        target.join(".multisoul/project.sqlite").is_file(),
        "被阻断的不可信状态文件应保留供用户审查"
    );
    assert!(
        target.join(".git").is_dir(),
        "安全阻断不能删除已成功克隆的 Git 仓库"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, CloneEvent::Completed { .. })),
        "安全阻断路径不得发送 Completed，避免 App 自动打开"
    );
}

/// GC-G5：运行中的取消必须终止并 wait Git 子进程，再清理本任务目标。
///
/// 数据构造（含关键数值的推导过程）：
///   fake Git sleep = 10s；取消响应预算 = 2s；轮询间隔 = 25ms。
///   理论轮询上界 ≈ ceil(2000 / 25) = 80 次，远小于 fake Git 的 10s 自然结束时间。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建会 sleep 10s 的可执行 fake Git，并在独立线程启动 Runner。
///   2. 收到 Started 事件后立即设置 cancellation token。
///   3. Runner 在下一次轮询中 kill + wait 子进程，校验目录 identity 后清理目标。
///
/// 预期结果：
///   - 正断言：返回 Cancelled，且总耗时小于 2s。
///   - 正断言：本任务创建的目标被清理。
///   - 负断言：不得等待完整 10s，也不得发送 Completed。
#[cfg(unix)]
#[test]
fn git_clone_runner_cancels_running_child_and_cleans_owned_target() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let projects = temp.path().join("projects");
    std::fs::create_dir(&projects).expect("测试应创建目标父目录");
    let fake_git = temp.path().join("fake-git.sh");
    std::fs::write(&fake_git, b"#!/bin/sh\nsleep 10\n").expect("测试应写入可阻塞的 fake Git");
    let mut permissions = std::fs::metadata(&fake_git)
        .expect("fake Git metadata 应可读取")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_git, permissions).expect("fake Git 应设置为当前用户可执行");
    let destination = validate_clone_destination(&projects, "cancel-running")
        .expect("运行中取消用例的新目标路径应通过校验");
    let request = GitCloneRequest::for_url_for_test("ignored-source".to_string(), destination);
    let cancellation = CloneCancellation::new();
    let runner_cancellation = cancellation.clone();
    let (event_tx, event_rx) = mpsc::channel();
    let start = Instant::now();

    let handle = std::thread::spawn(move || {
        GitCloneRunner::with_binary_for_test(fake_git).run(request, &runner_cancellation, |event| {
            event_tx
                .send(event)
                .expect("测试事件接收端在 Runner 结束前应保持存在");
        })
    });
    let started = event_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("Runner 应在 2s 预算内发送 Started");
    cancellation.cancel();
    let result = handle.join().expect("Runner 测试线程不应 panic");
    let elapsed = start.elapsed();

    assert!(
        matches!(started, CloneEvent::Started { .. }),
        "取消前收到的首个生命周期事件应是 Started，实际为 {started:?}"
    );
    assert_eq!(
        result.expect_err("运行中取消必须返回错误").kind(),
        CloneFailureKind::Cancelled,
        "运行中取消应稳定分类为 Cancelled"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "取消应在 2s 内 kill + wait，而不是等待 fake Git 的完整 10s，实际 {elapsed:?}"
    );
    assert!(
        !projects.join("cancel-running").exists(),
        "取消后应清理本任务创建且 identity 未变化的目标目录"
    );
    assert!(
        event_rx
            .try_iter()
            .all(|event| !matches!(event, CloneEvent::Completed { .. })),
        "运行中取消不得发送 Completed 事件"
    );
    assert_ne!(
        elapsed >= Duration::from_secs(10),
        true,
        "取消路径不得退化为等待 fake Git 自然退出"
    );
}
