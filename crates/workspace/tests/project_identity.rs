use std::process::Command;
use workspace::project::{identify_project, project_worktrees};

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("测试应可调用 git");
    assert!(
        output.status.success(),
        "git {args:?} 失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// DWS-02：主工作树和链接工作树属于同一项目，但保留各自可选择的执行目录。
///
/// 数据构造（含关键数值的推导过程）：
///   Git 仓库数 = 1；工作树目录数 = 2；预期项目身份数 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始化仓库并创建一次空提交，使 Git 可创建链接工作树。
///   2. 创建 linked 工作树，分别识别两个目录。
///   3. 比较项目 key 与所选 root。
///
/// 预期结果：
///   - 正断言：两个目录有相同项目 key，两个 root 都是合法目录。
///   - 负断言：两个工作树的 root 不相同，不会误把 worktree 当项目身份。
#[test]
fn linked_worktrees_share_project_identity_but_keep_distinct_roots() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let main = temp.path().join("main");
    let linked = temp.path().join("linked");
    std::fs::create_dir(&main).expect("测试应创建主仓库目录");
    git(&main, &["init", "--initial-branch=main"]);
    git(
        &main,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ],
    );
    git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "feature",
            linked.to_str().expect("临时路径应为 UTF-8"),
        ],
    );

    let main_identity = identify_project(&main).expect("应识别主工作树");
    let linked_identity = identify_project(&linked).expect("应识别链接工作树");

    assert_eq!(
        main_identity.key, linked_identity.key,
        "同一 Git common dir 应映射到同一个项目"
    );
    assert_eq!(
        main_identity.root,
        main.canonicalize().expect("主工作树应可规范化"),
        "主工作树应保留执行目录"
    );
    assert_eq!(
        linked_identity.root,
        linked.canonicalize().expect("链接工作树应可规范化"),
        "链接工作树应保留执行目录"
    );
    assert_ne!(
        main_identity.root, linked_identity.root,
        "两个工作树目录不得被误认为同一个执行目录"
    );
}

/// DWS-02：Agent 创建选择器应列出同项目的全部 Git worktree；普通目录只有自身。
///
/// 数据构造（含关键数值的推导过程）：
///   Git 项目 = 1，工作树 = 主树 1 + 链接树 1 = 2；普通目录 = 1，候选执行目录 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始化 Git 仓库及第二工作树。
///   2. 从链接工作树识别项目，并列出可选 worktree。
///   3. 为普通目录列出可选执行目录。
///
/// 预期结果：
///   - 正断言：Git 项目同时列出主树和链接树，普通目录只列出自身。
///   - 负断言：Git 候选不重复，普通目录不混入 Git 工作树。
#[test]
fn agent_worktree_choices_cover_repository_and_plain_directory() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let main = temp.path().join("main");
    let linked = temp.path().join("linked");
    let plain = temp.path().join("plain");
    std::fs::create_dir(&main).expect("应创建仓库目录");
    std::fs::create_dir(&plain).expect("应创建普通目录");
    git(&main, &["init", "--initial-branch=main"]);
    git(
        &main,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ],
    );
    git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "feature",
            linked.to_str().expect("临时路径应为 UTF-8"),
        ],
    );

    let git_choices = project_worktrees(&identify_project(&linked).expect("应识别链接工作树"))
        .expect("应读取 Git worktree 列表");
    let plain_choices = project_worktrees(&identify_project(&plain).expect("应识别普通目录"))
        .expect("普通目录应可用");
    assert_eq!(git_choices.len(), 2, "Git 项目应提供两个工作树");
    assert!(
        git_choices.contains(&main.canonicalize().expect("主树应可规范化")),
        "候选中必须有主工作树"
    );
    assert!(
        git_choices.contains(&linked.canonicalize().expect("链接树应可规范化")),
        "候选中必须有链接工作树"
    );
    assert_eq!(
        plain_choices,
        vec![plain.canonicalize().expect("普通目录应可规范化")],
        "普通目录仅以自身为执行目录"
    );
    assert_ne!(git_choices[0], git_choices[1], "Git 候选不得重复");
    assert!(
        !plain_choices.contains(&linked),
        "普通目录不得混入 Git 工作树"
    );
}
