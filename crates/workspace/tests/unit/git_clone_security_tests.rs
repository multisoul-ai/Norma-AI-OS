use super::{secure_cloned_workspace, ClonedWorkspaceSecurityError};
use std::path::Path;
use std::process::Command;

fn init_git_repository(path: &Path) {
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(path)
        .status()
        .expect("测试环境应能启动系统 Git");
    assert!(status.success(), "git init 应成功创建测试仓库");
}

/// GC-S1：仓库跟踪 `.multisoul` 时必须保留文件并阻止自动打开。
///
/// 数据构造（含关键数值的推导过程）：
///   Git 仓库 = 1；被跟踪的不可信状态文件 = `.multisoul/project.sqlite`(1)。
///   安全处理后的删除文件数 = 0；自动打开许可数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始化仓库并创建 `.multisoul/project.sqlite`。
///   2. `git add` 将该文件放入 index，模拟远端仓库携带 Norma 状态。
///   3. 执行克隆后安全收口，检查 tracked path 并拒绝信任。
///
/// 预期结果：
///   - 正断言：返回 TrackedNormaState，供 App 阻止自动打开并提示用户。
///   - 正断言：不可信文件仍保留，用户可自行审查。
///   - 负断言：不得写入 exclude 后假装仓库安全，也不得删除整个克隆结果。
#[test]
fn cloned_workspace_blocks_tracked_norma_state_without_deleting_repository() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    init_git_repository(temp.path());
    let norma_dir = temp.path().join(".multisoul");
    std::fs::create_dir(&norma_dir).expect("测试应创建不可信 .multisoul 目录");
    let project_state = norma_dir.join("project.sqlite");
    std::fs::write(&project_state, b"untrusted-state").expect("测试应写入不可信状态文件");
    let add_status = Command::new("git")
        .args(["add", ".multisoul/project.sqlite"])
        .current_dir(temp.path())
        .status()
        .expect("测试环境应能启动 git add");
    assert!(
        add_status.success(),
        "git add 应将 .multisoul 文件加入 index"
    );

    let error =
        secure_cloned_workspace(temp.path()).expect_err("跟踪 .multisoul 的仓库必须阻止自动打开");
    let exclude = std::fs::read_to_string(temp.path().join(".git/info/exclude"))
        .expect("git init 应创建可读取的 info/exclude");

    assert_eq!(
        error,
        ClonedWorkspaceSecurityError::TrackedNormaState,
        "跟踪 Norma 状态应返回 TrackedNormaState"
    );
    assert!(
        project_state.exists(),
        "安全阻断必须保留不可信文件供用户审查，不能静默删除"
    );
    assert!(
        temp.path().join(".git").is_dir(),
        "安全阻断后 Git 仓库必须仍完整存在"
    );
    assert!(
        !exclude.lines().any(|line| line == ".multisoul/"),
        "跟踪攻击者状态的仓库不得通过写 exclude 被误标为安全"
    );
}

/// GC-S2：安全仓库应幂等地本地排除 Norma 状态，且不修改项目 `.gitignore`。
///
/// 数据构造（含关键数值的推导过程）：
///   安全收口调用 = first(1) + repeat(1) = 2 次。
///   `.multisoul/` 期望出现次数 = 1，而不是 2；项目 `.gitignore` 写入次数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始化不跟踪 `.multisoul` 的空仓库。
///   2. 连续执行两次安全收口。
///   3. 读取 `.git/info/exclude` 与项目根 `.gitignore`。
///
/// 预期结果：
///   - 正断言：两次调用都成功，exclude 精确包含一条 `.multisoul/`。
///   - 负断言：不得创建或修改项目 `.gitignore`，避免污染用户仓库。
#[test]
fn cloned_workspace_adds_norma_to_local_exclude_idempotently() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    init_git_repository(temp.path());

    secure_cloned_workspace(temp.path()).expect("安全仓库首次收口应成功");
    secure_cloned_workspace(temp.path()).expect("安全仓库重复收口应保持幂等");

    let exclude = std::fs::read_to_string(temp.path().join(".git/info/exclude"))
        .expect("安全收口后 info/exclude 应可读取");
    let norma_exclude_count = exclude
        .lines()
        .filter(|line| *line == ".multisoul/")
        .count();

    assert_eq!(
        norma_exclude_count, 1,
        "连续 2 次安全收口后 `.multisoul/` 应只出现 1 次"
    );
    assert!(
        exclude.lines().any(|line| line == ".multisoul/"),
        "本地 exclude 应明确包含 `.multisoul/`"
    );
    assert!(
        !temp.path().join(".gitignore").exists(),
        "安全收口不得创建项目级 .gitignore"
    );
    assert!(
        !temp.path().join(".multisoul").exists(),
        "安全收口只配置排除规则，不应提前创建 Norma 状态目录"
    );
}

/// GC-S3：非 Git 仓库或异常 Git 元数据不能被误判为可安全打开。
///
/// 数据构造（含关键数值的推导过程）：
///   普通目录 = 1；`.git` 元数据目录 = 0；Git index = 0。
///   安全成功数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建普通临时目录，不运行 git init。
///   2. 尝试执行克隆后安全收口。
///   3. 检查错误分类与文件系统副作用。
///
/// 预期结果：
///   - 正断言：返回 GitInspectionFailed。
///   - 负断言：不得为了继续流程而创建伪 `.git` 或 `.multisoul` 目录。
#[test]
fn cloned_workspace_rejects_non_git_directory_without_creating_metadata() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");

    let error =
        secure_cloned_workspace(temp.path()).expect_err("非 Git 目录不能通过克隆后安全检查");

    assert!(
        matches!(error, ClonedWorkspaceSecurityError::GitInspectionFailed(_)),
        "非 Git 目录应返回 GitInspectionFailed，实际为 {error:?}"
    );
    assert!(
        !temp.path().join(".git").exists(),
        "失败的安全检查不得创建伪 .git 元数据"
    );
    assert!(
        !temp.path().join(".multisoul").exists(),
        "失败的安全检查不得创建 .multisoul 状态目录"
    );
    assert_ne!(
        error,
        ClonedWorkspaceSecurityError::TrackedNormaState,
        "缺失 Git 元数据不得误报为仓库跟踪 Norma 状态"
    );
}
