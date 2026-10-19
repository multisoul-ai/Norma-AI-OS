use std::path::Path;
use workspace::content::{FileOptions, Project};

/// 场景：项目目录包含普通文件、隐藏文件、忽略文件及越界链接。
/// 数据构造：临时 Git 仓库，.gitignore 排除 build；链接指向仓库外。
/// 执行过程：列目录、开启显示选项，再读取越界链接。
/// 预期结果：默认隐藏内部项；显式显示时可见；读取不能逃逸根目录。
#[test]
fn project_tree_respects_visibility_and_root() {
    let temp = tempfile::tempdir().expect("创建临时目录");
    git2::Repository::init(temp.path()).expect("创建仓库");
    std::fs::write(temp.path().join(".gitignore"), "build\n").expect("写忽略规则");
    std::fs::write(temp.path().join("hello.md"), "# Hello").expect("写文件");
    std::fs::create_dir(temp.path().join("build")).expect("创建忽略目录");
    let outside = tempfile::NamedTempFile::new().expect("创建外部文件");
    std::os::unix::fs::symlink(outside.path(), temp.path().join("escape")).expect("创建链接");
    let project = Project::open(temp.path()).expect("打开项目");
    let entries = project
        .directory(Path::new(""), FileOptions::default())
        .expect("列目录");
    assert!(
        entries.iter().any(|e| e.name == "hello.md"),
        "普通文件应可见"
    );
    assert!(
        !entries
            .iter()
            .any(|e| e.name == "build" || e.name == ".git"),
        "默认不显示忽略目录和 Git 内部目录"
    );
    let all = project
        .directory(
            Path::new(""),
            FileOptions {
                hidden: true,
                ignored: true,
            },
        )
        .expect("列全部");
    assert!(all.iter().any(|e| e.name == "build"), "显式显示忽略目录");
    assert!(
        !all.iter().any(|e| e.name == ".git"),
        "Git 内部目录始终隐藏"
    );
    assert!(
        project.read_text(Path::new("escape")).is_err(),
        "禁止读取根目录外的链接"
    );
    assert!(
        project
            .directory(Path::new("../"), FileOptions::default())
            .is_err(),
        "禁止目录穿越"
    );
}

/// 场景：编辑保存时 Coding Agent 已改写同一个文件。
/// 数据构造：读取 first 版本后从外部改写为 external。
/// 执行过程：用旧版本保存草稿，再用最新版本保存。
/// 预期结果：旧保存被拒绝且外部内容保留；新版本保存成功。
#[test]
fn save_checks_the_version_the_user_read() {
    let temp = tempfile::tempdir().expect("临时目录");
    std::fs::write(temp.path().join("a.txt"), "first").expect("原文");
    let p = Project::open(temp.path()).expect("项目");
    let before = p.read_text(Path::new("a.txt")).expect("读取");
    std::fs::write(temp.path().join("a.txt"), "external").expect("外部修改");
    assert!(
        p.save_text(Path::new("a.txt"), &before.version, "draft")
            .is_err(),
        "不能覆盖外部修改"
    );
    let current = p.read_text(Path::new("a.txt")).expect("读取外部修改");
    assert_eq!(current.text, "external", "拒绝保存不能改变磁盘");
    let saved = p
        .save_text(Path::new("a.txt"), &current.version, "merged")
        .expect("保存合并");
    assert_eq!(saved.text, "merged", "最新版本可保存");
}

fn fixture() -> (tempfile::TempDir, Project) {
    let temp = tempfile::tempdir().expect("临时目录");
    let repo = git2::Repository::init(temp.path()).expect("仓库");
    let mut config = repo.config().expect("配置");
    config.set_str("user.name", "Test").expect("姓名");
    config
        .set_str("user.email", "test@example.invalid")
        .expect("邮箱");
    std::fs::write(temp.path().join("a.txt"), "before\n").expect("初始文件");
    let mut index = repo.index().expect("索引");
    index.add_path(Path::new("a.txt")).expect("暂存");
    index.write().expect("写索引");
    let tree = repo
        .find_tree(index.write_tree().expect("写树"))
        .expect("树");
    let sig = repo.signature().expect("身份");
    repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
        .expect("初始提交");
    let project = Project::open(temp.path()).expect("项目");
    (temp, project)
}

/// 场景：暂存后继续编辑文件，再提交。
/// 数据构造：初始 a=before，暂存 staged 后再写 unstaged，另外添加未跟踪文件。
/// 执行过程：查看状态、暂存、提交；再尝试用旧状态操作。
/// 预期结果：只提交索引内容；未暂存和未跟踪内容保留，旧状态写入被拒绝。
#[test]
fn git_actions_preserve_unstaged_work_and_reject_stale_state() {
    use workspace::content::{Comparison, GitAction};
    let (temp, p) = fixture();
    std::fs::write(temp.path().join("a.txt"), "staged\n").expect("修改");
    let status = p.git_snapshot(&Comparison::Local).expect("状态");
    assert_eq!(status.changes.len(), 1, "有一个变更");
    p.git_action(&status.version, GitAction::Stage(vec!["a.txt".into()]))
        .expect("暂存");
    std::fs::write(temp.path().join("a.txt"), "unstaged\n").expect("继续编辑");
    std::fs::write(temp.path().join("new.txt"), "untracked").expect("未跟踪文件");
    assert!(
        p.git_action(&status.version, GitAction::Unstage(vec!["a.txt".into()]))
            .is_err(),
        "旧状态不能执行写入"
    );
    let status = p.git_snapshot(&Comparison::Local).expect("新状态");
    p.git_action(&status.version, GitAction::Commit("stage only".into()))
        .expect("提交");
    let repo = git2::Repository::open(temp.path()).expect("仓库");
    let tree = repo.head().expect("HEAD").peel_to_tree().expect("树");
    let object = tree
        .get_path(Path::new("a.txt"))
        .expect("文件")
        .to_object(&repo)
        .expect("对象");
    assert_eq!(
        object.as_blob().expect("blob").content(),
        b"staged\n",
        "只能提交索引版本"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("a.txt")).expect("磁盘"),
        "unstaged\n",
        "未暂存内容保留"
    );
    assert!(temp.path().join("new.txt").exists(), "未跟踪文件不能删除");
}

/// 场景：已暂存的重命名应显示新路径并能完整取消暂存。
/// 数据构造：初始 a.txt 移动为 b.txt，将删除与新增共同暂存。
/// 执行过程：读取状态，按 UI 返回的新路径取消暂存。
/// 预期结果：变更条目同时保存新旧路径，取消暂存后索引恢复 a.txt。
#[test]
fn staged_rename_uses_new_path_and_unstages_both_sides() {
    use workspace::content::{Comparison, GitAction};
    let (temp, p) = fixture();
    std::fs::rename(temp.path().join("a.txt"), temp.path().join("b.txt")).expect("重命名");
    let repo = git2::Repository::open(temp.path()).expect("仓库");
    let mut index = repo.index().expect("索引");
    index.remove_path(Path::new("a.txt")).expect("移除旧名");
    index.add_path(Path::new("b.txt")).expect("暂存新名");
    index.write().expect("写索引");
    let status = p.git_snapshot(&Comparison::Local).expect("状态");
    assert_eq!(status.changes[0].path, "b.txt", "重命名应指向新文件");
    assert_eq!(
        status.changes[0].old_path.as_deref(),
        Some("a.txt"),
        "保留旧路径用于完整操作"
    );
    p.git_action(&status.version, GitAction::Unstage(vec!["b.txt".into()]))
        .expect("取消暂存重命名");
    let mut index = repo.index().expect("索引");
    index.read(true).expect("重读外部 Git 写入的索引");
    assert!(
        index.get_path(Path::new("a.txt"), 0).is_some(),
        "恢复旧名索引项"
    );
    assert!(
        index.get_path(Path::new("b.txt"), 0).is_none(),
        "移除新名索引项"
    );
    assert!(temp.path().join("b.txt").exists(), "不能删除工作树新文件");
}

/// 场景：项目内符号链接指向 .git 目录。
/// 数据构造：metadata -> .git，Git 配置含私有值。
/// 执行过程：通过别名尝试读取和写入配置。
/// 预期结果：规范化后的内部路径也被拒绝，不能借别名修改 Git 元数据。
#[test]
fn git_metadata_alias_cannot_be_read_or_written() {
    let (temp, p) = fixture();
    std::os::unix::fs::symlink(".git", temp.path().join("metadata")).expect("别名");
    assert!(
        p.read_text(Path::new("metadata/config")).is_err(),
        "内部目录别名不可读取"
    );
    assert!(
        p.save_text(Path::new("metadata/config"), "", "broken")
            .is_err(),
        "内部目录别名不可写入"
    );
}

/// 场景：用户删除整个目录后撤销其中已跟踪文件。
/// 数据构造：提交 src/a 后删除 src。
/// 执行过程：按变更列表的路径撤销。
/// 预期结果：父目录缺失不妨碍恢复，文件内容恢复为索引版本。
#[test]
fn discard_restores_deleted_parent_directory() {
    use workspace::content::{Comparison, GitAction};
    let (temp, p) = fixture();
    std::fs::create_dir(temp.path().join("src")).expect("目录");
    std::fs::write(temp.path().join("src/a"), "source").expect("文件");
    let status = p.git_snapshot(&Comparison::Local).expect("状态");
    p.git_action(&status.version, GitAction::Stage(vec!["src/a".into()]))
        .expect("暂存");
    let status = p.git_snapshot(&Comparison::Local).expect("状态");
    p.git_action(&status.version, GitAction::Commit("src".into()))
        .expect("提交");
    std::fs::remove_dir_all(temp.path().join("src")).expect("删除目录");
    let status = p.git_snapshot(&Comparison::Local).expect("状态");
    p.git_action(&status.version, GitAction::Discard(vec!["src/a".into()]))
        .expect("恢复删除目录中的文件");
    assert_eq!(
        std::fs::read_to_string(temp.path().join("src/a")).expect("恢复文件"),
        "source",
        "按索引恢复"
    );
}

/// 场景：首个提交前暂存 v1 后继续编辑为 v2，再取消暂存。
/// 数据构造：没有 HEAD 的新仓库，索引与工作树内容不同。
/// 执行过程：取消暂存 a.txt。
/// 预期结果：索引移除 a.txt，磁盘 v2 不丢失。
#[test]
fn unborn_unstage_keeps_worktree_edits() {
    use workspace::content::{Comparison, GitAction};
    let temp = tempfile::tempdir().expect("目录");
    let repo = git2::Repository::init(temp.path()).expect("仓库");
    let p = Project::open(temp.path()).expect("项目");
    std::fs::write(temp.path().join("a.txt"), "v1").expect("v1");
    let mut index = repo.index().expect("索引");
    index.add_path(Path::new("a.txt")).expect("暂存");
    index.write().expect("写索引");
    std::fs::write(temp.path().join("a.txt"), "v2").expect("v2");
    let snapshot = p.git_snapshot(&Comparison::Local).expect("状态");
    p.git_action(&snapshot.version, GitAction::Unstage(vec!["a.txt".into()]))
        .expect("取消暂存");
    let mut index = repo.index().expect("索引");
    index.read(true).expect("重读外部 Git 写入的索引");
    assert!(index.is_empty(), "索引应为空");
    assert_eq!(
        std::fs::read_to_string(temp.path().join("a.txt")).expect("文件"),
        "v2",
        "磁盘修改保留"
    );
}

/// 场景：分支提交只重命名文件，工作区另有未提交内容。
/// 数据构造：base 指向初始提交，新提交将 a.txt 移为 b.txt；随后修改 b。
/// 执行过程：以 base 比较分支，读取单文件差异。
/// 预期结果：显示 rename 且不包含未提交的工作区内容。
#[test]
fn branch_diff_preserves_rename_and_excludes_worktree() {
    use workspace::content::{Comparison, GitAction};
    let (temp, p) = fixture();
    let repo = git2::Repository::open(temp.path()).expect("仓库");
    let head = repo.head().expect("HEAD").peel_to_commit().expect("提交");
    repo.branch("base", &head, false).expect("基准分支");
    std::fs::rename(temp.path().join("a.txt"), temp.path().join("b.txt")).expect("重命名");
    let status = p.git_snapshot(&Comparison::Local).expect("状态");
    p.git_action(
        &status.version,
        GitAction::Stage(vec!["a.txt".into(), "b.txt".into()]),
    )
    .expect("暂存");
    let status = p.git_snapshot(&Comparison::Local).expect("状态");
    p.git_action(&status.version, GitAction::Commit("rename".into()))
        .expect("提交");
    std::fs::write(temp.path().join("b.txt"), "not committed").expect("工作区修改");
    let comparison = Comparison::Branch("base".into());
    let snapshot = p.git_snapshot(&comparison).expect("分支状态");
    assert_eq!(snapshot.changes[0].path, "b.txt", "使用新路径");
    let diff = p.git_diff(&comparison, "b.txt", false).expect("差异");
    assert!(
        diff.contains("rename from a.txt"),
        "分支 diff 必须识别重命名：{diff}"
    );
    assert!(
        !diff.contains("not committed"),
        "分支比较不能包含工作区修改"
    );
}

/// 场景：仓库 pre-commit 拒绝提交。
/// 数据构造：隔离仓库内 hook 输出 REJECTED 并退出 1。
/// 执行过程：暂存并提交。
/// 预期结果：正常执行 hook，向调用方返回失败，HEAD 与已暂存内容保留。
#[test]
fn commit_runs_hooks_and_reports_rejection() {
    use std::os::unix::fs::PermissionsExt;
    use workspace::content::{Comparison, GitAction};
    let (temp, p) = fixture();
    let repo = git2::Repository::open(temp.path()).expect("仓库");
    let original = repo.head().expect("HEAD").target();
    let hook = temp.path().join(".git/hooks/pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho REJECTED >&2\nexit 1\n").expect("hook");
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).expect("可执行");
    std::fs::write(temp.path().join("a.txt"), "change").expect("修改");
    let snapshot = p.git_snapshot(&Comparison::Local).expect("状态");
    p.git_action(&snapshot.version, GitAction::Stage(vec!["a.txt".into()]))
        .expect("暂存");
    let snapshot = p.git_snapshot(&Comparison::Local).expect("状态");
    let error = p
        .git_action(&snapshot.version, GitAction::Commit("reject".into()))
        .expect_err("hook 应拒绝");
    assert!(error.contains("REJECTED"), "展示 hook 的失败信息");
    assert_eq!(
        repo.head().expect("HEAD").target(),
        original,
        "不能提交失败却改变 HEAD"
    );
    assert!(
        p.git_snapshot(&Comparison::Local).expect("状态").changes[0].staged,
        "失败后暂存保留"
    );
}
