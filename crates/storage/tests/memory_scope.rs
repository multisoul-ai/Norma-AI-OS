use agent::MemoryScopeAlias;
use storage::{DatabasePaths, MemoryAccess};

/// 同仓库工作树共享项目知识，分支事实与其他 clone 隔离；路径替换不能继承旧范围。
///
/// 数据构造：一个 Git 仓库加 linked worktree，另一个独立 clone；三套受控会话。
/// 执行过程：1. 主工作树写 project/shared 与 worktree/branch。2. linked worktree 读取。
///   3. clone 读取项目目录。4. 原根移走并新建同路径目录，旧会话再次访问。
/// 预期结果：linked 共享约定存在，branch 不存在；clone 不含 shared；旧绑定拒绝替换目录。
#[test]
fn repository_worktrees_share_only_project_memory() {
    use std::process::Command;
    let temp = tempfile::tempdir().expect("独立根");
    let root = temp.path().join("repo");
    let git = |args: &[&str]| {
        let output = Command::new("git").args(args).output().expect("运行 git");
        assert!(
            output.status.success(),
            "Git fixture 必须成功：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", root.to_str().expect("utf8")]);
    git(&[
        "-C",
        root.to_str().expect("utf8"),
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit",
        "--allow-empty",
        "-m",
        "fixture",
    ]);
    let linked = temp.path().join("linked");
    git(&[
        "-C",
        root.to_str().expect("utf8"),
        "worktree",
        "add",
        "-b",
        "test-branch",
        linked.to_str().expect("utf8"),
    ]);
    let cloned = temp.path().join("clone");
    git(&[
        "clone",
        root.to_str().expect("utf8"),
        cloned.to_str().expect("utf8"),
    ]);
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let mut main = MemoryAccess::new(paths.clone(), Some(root.clone()));
    main.settings_save(
        MemoryScopeAlias::Project,
        agent::MemoryRevision::INITIAL,
        vec![("shared".into(), "项目使用 Rust".into())],
        "项目约定",
        "p1",
    )
    .expect("共享保存");
    main.settings_save(
        MemoryScopeAlias::Worktree,
        agent::MemoryRevision::INITIAL,
        vec![("branch".into(), "当前分支测试已通过".into())],
        "分支事实",
        "w1",
    )
    .expect("工作树保存");
    let mut other = MemoryAccess::new(paths.clone(), Some(linked));
    assert!(
        other
            .settings_snapshot(MemoryScopeAlias::Project)
            .expect("共享范围")
            .entries()
            .any(|entry| entry.id.as_str() == "shared"),
        "linked worktree 必须共享项目约定"
    );
    assert!(
        !other
            .settings_snapshot(MemoryScopeAlias::Worktree)
            .expect("独立工作树")
            .entries()
            .any(|entry| entry.id.as_str() == "branch"),
        "工作树事实不能串到另一分支"
    );
    let mut clone = MemoryAccess::new(paths, Some(cloned));
    assert!(
        !clone
            .settings_snapshot(MemoryScopeAlias::Project)
            .expect("clone 范围")
            .entries()
            .any(|entry| entry.id.as_str() == "shared"),
        "不同 clone 默认隔离"
    );
    std::fs::rename(&root, temp.path().join("moved")).expect("移走旧根");
    std::fs::create_dir(&root).expect("替换同路径目录");
    assert!(
        main.settings_snapshot(MemoryScopeAlias::Project).is_err(),
        "旧会话不能继承替换目录的身份"
    );
}
