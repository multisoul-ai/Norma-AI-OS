use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use workspace::review::{capture, ContentKind, ReviewError};
#[path = "project_review/baseline.rs"]
mod baseline;

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("/usr/bin/git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("运行隔离Git fixture");
    assert!(
        output.status.success(),
        "隔离Git fixture命令必须成功: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("隔离项目");
    git(dir.path(), &["init", "--quiet"]);
    std::fs::write(dir.path().join("main.txt"), "before\n").expect("原文件");
    std::fs::write(dir.path().join("deleted.txt"), "deleted baseline\n").expect("将删除文件");
    git(dir.path(), &["add", "main.txt", "deleted.txt"]);
    // 本测试仓库没有项目hooks；只在该临时目录建立基准，不提交当前工作区或修改全局Git配置。
    git(
        dir.path(),
        &[
            "-c",
            "user.name=Norma Test",
            "-c",
            "user.email=norma-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "baseline",
        ],
    );
    dir
}

/// 工作区交付快照包含HEAD到当前内容的完整变更，并同时保留未跟踪文件和删除前正文。
///
/// 数据构造：main.txt先staged再unstaged；删除deleted.txt；新增new.txt；另有未变化.git内部数据。
/// 执行过程：Git元数据定位变更 → 从Git基准取旧正文 → 安全打开项目内文件 → 生成前后与diff。
/// 预期结果：main使用最终正文而非index；新文件before为空，删除文件after为空；无.git与临时路径泄露。
#[test]
fn capture_keeps_full_before_after_and_current_changes() {
    let dir = fixture();
    std::fs::write(dir.path().join("main.txt"), "staged\n").expect("已暂存版本");
    git(dir.path(), &["add", "main.txt"]);
    std::fs::write(dir.path().join("main.txt"), "after\n").expect("最终工作副本");
    std::fs::remove_file(dir.path().join("deleted.txt")).expect("删除");
    std::fs::write(dir.path().join("new.txt"), "new file\n").expect("新文件");
    std::fs::create_dir(dir.path().join(".norma")).expect("本地运行数据目录");
    std::fs::write(dir.path().join(".norma/state.sqlite"), "LOCAL_RUNTIME_ONLY").expect("本地状态");
    // 5s覆盖本地Git子进程和三个小文件；capture与每个子进程共用该绝对deadline。
    let review =
        capture(dir.path(), Instant::now() + Duration::from_secs(5)).expect("捕获完整变更");
    assert_eq!(
        review.files.len(),
        3,
        "必须恰好包含三条变更，不隐去新文件或混入.git"
    );
    let changed = review
        .files
        .iter()
        .find(|f| f.path == "main.txt")
        .expect("主文件");
    assert_eq!(
        changed.before.as_ref().expect("旧正文").bytes,
        b"before\n",
        "before必须取HEAD，不能取index"
    );
    assert_eq!(
        changed.after.as_ref().expect("新正文").bytes,
        b"after\n",
        "after必须是用户实际验收的工作副本"
    );
    assert!(
        changed.diff.as_ref().expect("文本diff").contains("+after"),
        "diff必须对应最终正文"
    );
    assert!(
        !changed.diff.as_ref().expect("文本diff").contains("staged"),
        "已被替换的index正文不能混进交付"
    );
    assert_eq!(
        changed.index.as_ref().expect("暂存版本").bytes,
        b"staged\n",
        "暂存区内容必须单独保留，防止预览与随后提交不一致"
    );
    assert!(
        changed
            .staged_diff
            .as_ref()
            .expect("暂存diff")
            .contains("+staged"),
        "暂存diff必须对应index而非最终工作副本"
    );
    let added = review
        .files
        .iter()
        .find(|f| f.path == "new.txt")
        .expect("新增文件");
    assert!(added.before.is_none(), "未跟踪文件没有伪造的旧版本");
    assert_eq!(
        added.after.as_ref().expect("新增正文").bytes,
        b"new file\n",
        "新文件正文完整保留"
    );
    let deleted = review
        .files
        .iter()
        .find(|f| f.path == "deleted.txt")
        .expect("删除文件");
    assert_eq!(
        deleted.before.as_ref().expect("删除前").bytes,
        b"deleted baseline\n",
        "删除前内容仍可核对"
    );
    assert!(deleted.after.is_none(), "删除不能冒充空文件");
    assert!(
        !review.files.iter().any(|f| f.path.starts_with(".git")),
        "Git内部数据不得作为成果返回"
    );
}

/// 符号链接只返回链接本身，超限和已到期读取必须明确失败，不能截断或读出项目外内容。
///
/// 数据构造：项目内link指向相邻临时目录的secret；over.bin为1MiB+1，超出单文件1MiB上限。
/// 执行过程：捕获链接 → 验证仅有链接目标字符串 → 加入超限文件 → 用已到期预算捕获。
/// 预期结果：不读secret正文；超限不返回部分成功；deadline到期不继续运行Git。
#[cfg(unix)]
#[test]
fn capture_does_not_follow_links_or_silently_truncate() {
    let dir = fixture();
    let outside = tempfile::tempdir().expect("项目外");
    let secret = outside.path().join("secret");
    std::fs::write(&secret, "DO_NOT_EXPORT_SECRET").expect("边界标记");
    std::os::unix::fs::symlink(&secret, dir.path().join("link")).expect("项目内链接");
    let review =
        capture(dir.path(), Instant::now() + Duration::from_secs(5)).expect("只读链接文本");
    let link = review
        .files
        .iter()
        .find(|f| f.path == "link")
        .expect("链接变更");
    assert_eq!(
        link.after.as_ref().expect("链接版本").kind,
        ContentKind::Symlink,
        "链接不能被当作普通正文读取"
    );
    assert!(
        !String::from_utf8_lossy(&link.after.as_ref().expect("链接").bytes)
            .contains("DO_NOT_EXPORT_SECRET"),
        "不得导出项目外目标内容"
    );
    std::fs::write(dir.path().join("over.bin"), vec![b'x'; 1024 * 1024 + 1])
        .expect("1MiB+1越过单文件上限");
    assert!(
        matches!(
            capture(dir.path(), Instant::now() + Duration::from_secs(5)),
            Err(ReviewError::TooLarge)
        ),
        "超限必须拒绝整个快照而非截断验收内容"
    );
    assert!(
        matches!(
            capture(dir.path(), Instant::now()),
            Err(ReviewError::Expired)
        ),
        "过期读取不得继续启动进程"
    );
}

/// 子目录授权只能读取该目录变更；无首次commit的项目保留新增文件但不伪造基准。
///
/// 数据构造：同一Git仓库有inside/a.txt与outside.txt，分别修改；另建只有new.txt的unborn仓库。
/// 执行过程：以inside作为授权根捕获 → 检查HEAD正文 → 捕获unborn仓库。
/// 预期结果：inside快照不含外层变更；unborn的base与before为空，after完整。
#[test]
fn capture_scopes_nested_projects_and_handles_unborn_head() {
    let dir = fixture();
    std::fs::create_dir(dir.path().join("inside")).expect("子目录");
    std::fs::write(dir.path().join("inside/a.txt"), "inside before\n").expect("子目录基准");
    git(dir.path(), &["add", "inside/a.txt"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.name=Norma Test",
            "-c",
            "user.email=norma-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "nested baseline",
        ],
    );
    std::fs::write(dir.path().join("inside/a.txt"), "inside after\n").expect("子目录修改");
    std::fs::write(dir.path().join("main.txt"), "OUTSIDE_PROJECT_SCOPE\n").expect("外层修改");
    // 5s为与其他capture用例相同的本地Git读取预算，捕获范围仅inside。
    let nested = capture(
        &dir.path().join("inside"),
        Instant::now() + Duration::from_secs(5),
    )
    .expect("子项目快照");
    assert_eq!(
        nested.files.len(),
        1,
        "父目录其他文件不得进入授权子目录快照"
    );
    assert_eq!(nested.files[0].path, "a.txt", "路径必须相对授权根");
    assert_eq!(
        nested.files[0].before.as_ref().expect("子目录旧正文").bytes,
        b"inside before\n",
        "HEAD必须读取同一子目录路径"
    );
    assert!(
        !nested.files.iter().any(|f| f.path == "main.txt"),
        "父项目变更不可泄露"
    );
    let empty = tempfile::tempdir().expect("尚无commit的项目");
    git(empty.path(), &["init", "--quiet"]);
    std::fs::write(empty.path().join("new.txt"), "new project\n").expect("新项目文件");
    let unborn =
        capture(empty.path(), Instant::now() + Duration::from_secs(5)).expect("新项目快照");
    assert!(unborn.base_commit.is_none(), "无commit不得伪造HEAD");
    assert!(unborn.files[0].before.is_none(), "新增文件不得伪造旧正文");
    assert_eq!(
        unborn.files[0].after.as_ref().expect("新正文").bytes,
        b"new project\n",
        "新项目正文完整"
    );
}

/// 只读预览不得运行仓库配置的clean/filter脚本，也不能递归进入另有配置的submodule。
///
/// 数据构造：main.txt配置会写FILTER_RAN的clean脚本；之后移除filter，并登记mode160000的子模块。
/// 执行过程：修改已跟踪正文 → 捕获 → 移除filter → 构造子模块index → 再捕获。
/// 预期结果：两次均明确Unsupported，标记文件不存在；禁止返回遗漏子模块的部分快照。
#[test]
fn capture_rejects_external_filters_and_submodules_without_executing_them() {
    let dir = fixture();
    git(
        dir.path(),
        &["config", "filter.probe.clean", "touch FILTER_RAN; cat"],
    );
    std::fs::write(dir.path().join(".gitattributes"), "main.txt filter=probe\n").expect("本地属性");
    std::fs::write(dir.path().join("main.txt"), "changed\n").expect("触发状态核验");
    // 5s与其他捕获用例一致；安全拒绝必须早于Git status可触发的filter。
    assert_eq!(
        capture(dir.path(), Instant::now() + Duration::from_secs(5)),
        Err(ReviewError::Unsupported),
        "配置可执行filter的项目必须明确拒绝"
    );
    assert!(
        !dir.path().join("FILTER_RAN").exists(),
        "只读预览不能执行clean脚本"
    );
    git(dir.path(), &["config", "--remove-section", "filter.probe"]);
    let hash = Command::new("/usr/bin/git")
        .current_dir(dir.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("HEAD");
    let hash = String::from_utf8(hash.stdout).expect("ASCII hash");
    git(
        dir.path(),
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},module", hash.trim()),
        ],
    );
    assert_eq!(
        capture(dir.path(), Instant::now() + Duration::from_secs(5)),
        Err(ReviewError::Unsupported),
        "子模块需要独立范围与配置，不可递归执行或静默忽略"
    );
    assert!(
        !dir.path().join("FILTER_RAN").exists(),
        "任何捕获阶段都不应运行filter"
    );
}
