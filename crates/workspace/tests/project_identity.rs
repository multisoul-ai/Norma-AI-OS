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

/// 已保存的仓库项目在外部 Git 命令不可执行时仍保持原身份。
///
/// 数据构造：真实仓库、普通目录及仓库内子目录，再启动 PATH 中没有 git 的测试子进程。
/// 执行过程：子进程分别调用正式 identify_project，并设置 Git 搜索的上行边界。
/// 预期结果：仓库键仍为 .git；普通目录及边界内子目录都以自身为键。
#[test]
fn repository_identity_survives_missing_git_executable() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let root = temp.path().join("project");
    let plain = temp.path().join("plain");
    let nested = root.join("nested");
    std::fs::create_dir(&root).expect("应创建项目目录");
    std::fs::create_dir(&plain).expect("应创建普通目录");
    git2::Repository::init(&root).expect("无外部 Git 命令也应能建立测试仓库");
    std::fs::create_dir(&nested).expect("应创建仓库内子目录");
    let empty_path = temp.path().join("empty-path");
    std::fs::create_dir(&empty_path).expect("应创建空 PATH 目录");
    let output = Command::new(std::env::current_exe().expect("应定位测试进程"))
        .args([
            "--exact",
            "repository_identity_without_git_child",
            "--nocapture",
        ])
        .env("NORMA_TEST_REPO_ROOT", &root)
        .env("NORMA_TEST_PLAIN_ROOT", &plain)
        .env("NORMA_TEST_NESTED_ROOT", &nested)
        .env(
            "GIT_CEILING_DIRECTORIES",
            root.canonicalize().expect("搜索边界应可规范化"),
        )
        .env("PATH", &empty_path)
        .output()
        .expect("应执行缺少 Git 的子进程");
    assert!(
        output.status.success(),
        "无外部 Git 时项目身份必须稳定：{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "子进程必须实际运行身份检查：{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// 仅作为缺少 Git 可执行文件的隔离子进程入口，由父测试提供真实仓库路径。
/// 数据构造：父测试创建的仓库、普通目录、仓库内子目录及空 PATH。
/// 预期结果：仓库键为 .git；普通目录和搜索边界内子目录均保持自身身份。
#[test]
fn repository_identity_without_git_child() {
    let Ok(root) = std::env::var("NORMA_TEST_REPO_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root)
        .canonicalize()
        .expect("仓库目录应可规范化");
    let identity = identify_project(&root).expect("无外部 Git 时仍应识别仓库");
    assert_eq!(
        identity.key,
        root.join(".git").canonicalize().expect("Git 目录应存在"),
        "无外部 Git 时项目键必须保持为原仓库"
    );
    assert_eq!(identity.root, root, "无外部 Git 时工作目录不得改变");
    let plain = std::path::PathBuf::from(
        std::env::var("NORMA_TEST_PLAIN_ROOT").expect("应提供普通目录路径"),
    )
    .canonicalize()
    .expect("普通目录应可规范化");
    let plain_identity = identify_project(&plain).expect("无外部 Git 时普通目录仍应可用");
    assert_eq!(
        plain_identity.key, plain,
        "无外部 Git 时普通目录应以自身为项目键"
    );
    assert_eq!(
        plain_identity.root, plain,
        "无外部 Git 时普通目录仍是 Agent 工作目录"
    );
    let nested = std::path::PathBuf::from(
        std::env::var("NORMA_TEST_NESTED_ROOT").expect("应提供搜索边界内的子目录路径"),
    )
    .canonicalize()
    .expect("子目录应可规范化");
    let nested_identity = identify_project(&nested).expect("应识别搜索边界内的子目录");
    assert_eq!(
        nested_identity.key, nested,
        "Git 上行搜索边界内的目录不应归属边界外的仓库"
    );
    assert_eq!(nested_identity.root, nested, "子目录仍应保留所选执行目录");
}

/// 原 Git 项目在同一路径被普通目录替换时，必须识别出项目键已变化。
///
/// 数据构造：一个真实仓库与其保存的 .git 键；删除整个目录后在同一路径重建普通目录。
/// 执行过程：分别在替换前后调用正式 identify_project。
/// 预期结果：路径相同但项目键不同，创建方仍可拒绝错误目标。
#[test]
fn replacing_repository_at_same_path_changes_project_identity() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    git2::Repository::init(&root).expect("无需外部 Git 命令即可建立测试仓库");
    let saved = identify_project(&root).expect("应保存原仓库身份");
    std::fs::remove_dir_all(&root).expect("应移除原仓库目录");
    std::fs::create_dir(&root).expect("应在同一路径重建普通目录");
    let current = identify_project(&root).expect("应识别替换后的目录");
    assert_eq!(current.root, saved.root, "替换前后的规范化路径应相同");
    assert_ne!(
        current.key, saved.key,
        "同一路径重建后，项目键必须揭示原仓库已被替换"
    );
    assert_eq!(
        current.key,
        root.canonicalize().expect("替换目录应可规范化"),
        "替换后的普通目录应使用规范化目录路径作为项目键"
    );
}

/// 带无效 .git 文件的普通目录仍可作为 Agent 项目。
///
/// 数据构造：一个普通目录，其中 .git 只是无效文本文件，并非仓库。
/// 执行过程：调用正式项目身份识别。
/// 预期结果：项目键与执行目录均为所选目录，不因无效 Git 元数据阻止创建。
#[test]
fn malformed_git_marker_remains_plain_directory() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let root = temp.path().join("plain-project");
    std::fs::create_dir(&root).expect("应创建普通目录");
    std::fs::write(root.join(".git"), "not a gitdir pointer").expect("应创建无效 Git 标记文件");

    let identity = identify_project(&root).expect("普通目录仍应可识别");
    let root = root.canonicalize().expect("普通目录应可规范化");
    assert_eq!(identity.key, root, "普通目录应使用自身作为项目键");
    assert_eq!(identity.root, root, "普通目录应保留所选执行目录");
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
