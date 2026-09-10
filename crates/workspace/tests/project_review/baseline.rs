use super::{capture, fixture, git};
use std::time::{Duration, Instant};
use workspace::review::{capture_since, ReviewError};

/// 子目录任务包括两次commit间的变更及首次commit，不能把父项目内容带入成果。
///
/// 数据构造：授权inside含a.txt；父目录含private.txt；任务起点为unborn，a正文为draft。
/// 执行过程：保存起点 → 首次commit并改a → 第二次commit同时改父文件 → 分别与两个起点对比。
/// 预期结果：两次成果均只有相对路径a.txt，原文分别为draft/first；父目录和内部状态缺席。
#[test]
fn nested_task_includes_commits_without_escaping_scope() {
    let dir = tempfile::tempdir().expect("隔离仓库");
    git(dir.path(), &["init", "--quiet"]);
    let inside = dir.path().join("inside");
    std::fs::create_dir(&inside).expect("授权根");
    std::fs::write(inside.join("a.txt"), "draft\n").expect("起点");
    std::fs::write(dir.path().join("private.txt"), "PRIVATE_BEFORE\n").expect("外层");
    // 两次完整捕获各10s，与正式预算一致；每个捕获只有一个授权文件。
    let start = capture(&inside, Instant::now() + Duration::from_secs(10)).expect("unborn起点");
    std::fs::write(inside.join("a.txt"), "first\n").expect("首次成果");
    commit_all(dir.path());
    let first = capture_since(&inside, &start, Instant::now() + Duration::from_secs(10))
        .expect("首commit成果");
    assert_eq!(first.files.len(), 1, "首次commit只能包含授权子目录");
    assert_eq!(first.files[0].path, "a.txt", "路径相对授权根而非仓库根");
    assert_eq!(
        first.files[0].before.as_ref().expect("原文").bytes,
        b"draft\n",
        "首次commit不能抹去unborn起点"
    );
    assert!(
        !first.files.iter().any(|f| f.path.contains("private.txt")),
        "不得泄漏外层文件路径"
    );
    let second_start =
        capture(&inside, Instant::now() + Duration::from_secs(10)).expect("干净起点");
    std::fs::write(inside.join("a.txt"), "second\n").expect("第二次成果");
    std::fs::write(dir.path().join("private.txt"), "PRIVATE_AFTER\n").expect("外层变化");
    std::fs::create_dir(inside.join(".norma")).expect("内部状态");
    std::fs::write(inside.join(".norma/state"), "INTERNAL\n").expect("内部文件");
    commit_all(dir.path());
    let second = capture_since(
        &inside,
        &second_start,
        Instant::now() + Duration::from_secs(10),
    )
    .expect("两个commit间成果");
    assert_eq!(second.files.len(), 1, "不得返回父目录或内部状态");
    assert_eq!(
        second.files[0].before.as_ref().expect("原文").bytes,
        b"first\n",
        "干净起点应取当时commit"
    );
    assert_eq!(
        second.files[0].after.as_ref().expect("正文").bytes,
        b"second\n",
        "已commit成果不能丢失"
    );
    assert!(
        !second
            .files
            .iter()
            .any(|f| f.path.contains(".norma") || f.path.contains("private")),
        "内部状态和父文件均不可见"
    );
}

/// 损坏的持久化起点不得变成Git选项、父目录读取或内部状态读取。
///
/// 数据构造：一份真实起点，分别注入--help commit、../secret和.norma/state路径。
/// 执行过程：三次capture_since先验证存储数据；合法原始起点仍能成功读取。
/// 预期结果：错误commit为Unavailable；两条越权路径均UnsafePath；原始起点没有虚构变更。
#[test]
fn saved_baseline_rejects_git_arguments_and_unsafe_paths() {
    let dir = fixture();
    std::fs::write(dir.path().join("main.txt"), "draft\n").expect("起点草稿");
    // 每次10s为生产捕获预算，非法输入必须在执行Git前拒绝。
    let start = capture(dir.path(), Instant::now() + Duration::from_secs(10)).expect("起点");
    let mut corrupted = start.clone();
    corrupted.base_commit = Some("--help".into());
    assert_eq!(
        capture_since(
            dir.path(),
            &corrupted,
            Instant::now() + Duration::from_secs(10)
        ),
        Err(ReviewError::Unavailable),
        "commit必须是完整hash，不能执行参数"
    );
    corrupted = start.clone();
    corrupted.files[0].path = "../secret".into();
    assert_eq!(
        capture_since(
            dir.path(),
            &corrupted,
            Instant::now() + Duration::from_secs(10)
        ),
        Err(ReviewError::UnsafePath),
        "父目录读取必须拒绝"
    );
    corrupted.files[0].path = ".norma/state".into();
    assert_eq!(
        capture_since(
            dir.path(),
            &corrupted,
            Instant::now() + Duration::from_secs(10)
        ),
        Err(ReviewError::UnsafePath),
        "内部状态不能通过保存的路径读取"
    );
    let unchanged = capture_since(dir.path(), &start, Instant::now() + Duration::from_secs(10))
        .expect("合法起点");
    assert!(unchanged.files.is_empty(), "未执行任务不能伪造变更");
    assert!(!unchanged.conflicted, "原始干净index没有冲突");
}

fn commit_all(root: &std::path::Path) {
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.name=Norma Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "scoped task",
        ],
    );
}

/// Git被标记为忽略工作副本变动的文件不能被当作干净起点或空成果。
///
/// 数据构造：main.txt有commit原文；先保存干净起点，再改为hidden draft并设置assume-unchanged/skip-worktree。
/// 执行过程：分别捕获两种隐藏标记；清除标记后按原起点读取真实变更。
/// 预期结果：隐藏标记必须Unsupported，不能拿HEAD冒充当前正文；正常读取保留hidden draft且非空。
#[test]
fn ignored_index_flags_cannot_hide_task_changes() {
    let dir = fixture();
    //10s为正式捕获预算；标志拒绝应早于状态扫描和内容读取。
    let start = capture(dir.path(), Instant::now() + Duration::from_secs(10)).expect("干净起点");
    std::fs::write(dir.path().join("main.txt"), "hidden draft\n").expect("隐藏修改");
    git(
        dir.path(),
        &["update-index", "--assume-unchanged", "main.txt"],
    );
    assert_eq!(
        capture(dir.path(), Instant::now() + Duration::from_secs(10)),
        Err(ReviewError::Unsupported),
        "assume-unchanged会让status遗漏用户已有修改，必须明确拒绝"
    );
    git(
        dir.path(),
        &["update-index", "--no-assume-unchanged", "main.txt"],
    );
    git(dir.path(), &["update-index", "--skip-worktree", "main.txt"]);
    assert_eq!(
        capture_since(dir.path(), &start, Instant::now() + Duration::from_secs(10)),
        Err(ReviewError::Unsupported),
        "skip-worktree不能静默生成空成果"
    );
    git(
        dir.path(),
        &["update-index", "--no-skip-worktree", "main.txt"],
    );
    let visible = capture_since(dir.path(), &start, Instant::now() + Duration::from_secs(10))
        .expect("恢复正常读取");
    assert_eq!(
        visible.files[0].after.as_ref().expect("正文").bytes,
        b"hidden draft\n",
        "清除标记后必须使用真实工作副本"
    );
    assert!(!visible.files.is_empty(), "实际改动不能丢失");
}

/// 任务开始已有未提交文件时，交付以保存的工作副本为原文；Agent提交代码后也不能丢失变更。
///
/// 数据构造：main原HEAD为before，任务起点为user draft；pre.txt是已有未跟踪文件；Agent改main、删除deleted并提交new。
/// 执行过程：捕获起点 → 修改/暂存/提交 → 再改new工作副本 → 相对原起点捕获交付。
/// 预期结果：main原文是user draft而非HEAD；已commit改动、删除和新文件均在；pre不冒充新建内容。
#[test]
fn delivery_compares_saved_dirty_state_and_includes_committed_changes() {
    let dir = fixture();
    std::fs::write(dir.path().join("main.txt"), "user draft\n").expect("任务前修改");
    std::fs::write(dir.path().join("pre.txt"), "preexisting\n").expect("任务前未跟踪文件");
    // 起点与交付各10s，与正式项目预览相同，覆盖完整Git读取但不重试或截断。
    let start = capture(dir.path(), Instant::now() + Duration::from_secs(10)).expect("保存起点");
    std::fs::write(dir.path().join("main.txt"), "agent final\n").expect("任务修改");
    std::fs::write(dir.path().join("new.txt"), "committed\n").expect("任务新文件");
    std::fs::remove_file(dir.path().join("deleted.txt")).expect("任务删除");
    git(dir.path(), &["add", "-A"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.name=Norma Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "task result",
        ],
    );
    std::fs::write(dir.path().join("new.txt"), "working\n").expect("commit后的继续修改");
    let done = capture_since(dir.path(), &start, Instant::now() + Duration::from_secs(10))
        .expect("包含已commit成果");
    let main = done
        .files
        .iter()
        .find(|f| f.path == "main.txt")
        .expect("已commit的main");
    assert_eq!(
        main.before.as_ref().expect("原文").bytes,
        b"user draft\n",
        "任务前已有修改必须保留，不能拿HEAD冒充起点"
    );
    assert_eq!(
        main.after.as_ref().expect("结果").bytes,
        b"agent final\n",
        "已commit文件不能因为status干净而消失"
    );
    assert!(
        !main.diff.as_ref().expect("diff").contains("-before\n"),
        "用户先前修改不得算作本轮删除"
    );
    let new = done
        .files
        .iter()
        .find(|f| f.path == "new.txt")
        .expect("新文件");
    assert!(new.before.is_none(), "任务开始时并无该文件");
    assert_eq!(
        new.index.as_ref().expect("暂存").bytes,
        b"committed\n",
        "index保留真实commit版本"
    );
    assert_eq!(
        new.after.as_ref().expect("当前").bytes,
        b"working\n",
        "提交后继续修改仍可见"
    );
    let deleted = done
        .files
        .iter()
        .find(|f| f.path == "deleted.txt")
        .expect("删除文件");
    assert!(deleted.after.is_none(), "已commit的删除不能消失");
    assert_eq!(
        deleted.before.as_ref().expect("删除前").bytes,
        b"deleted baseline\n",
        "未改过的文件可从起点commit恢复"
    );
    let pre = done
        .files
        .iter()
        .find(|f| f.path == "pre.txt")
        .expect("已被提交的原有文件");
    assert_eq!(
        pre.before.as_ref().expect("原有正文").bytes,
        b"preexisting\n",
        "原有未跟踪文件不能冒充本轮新建"
    );
    assert!(
        pre.diff.as_ref().expect("正文diff").is_empty(),
        "内容没变，不能伪造新增行"
    );
}
