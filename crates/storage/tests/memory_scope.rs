use agent::MemoryScopeAlias;
use storage::{DatabasePaths, MemoryAccess};

/// 同一路径普通目录被替换时，目录文件身份必须阻止旧会话访问或污染新记忆。
/// 构造：无Git目录，project key与worktree root始终为完全相同的路径字符串。
/// 执行：保存两种scope→移走根→同路径重建目录→旧会话读写→新会话读取。
/// 预期：旧会话两种scope均拒绝读写，新会话为空；仅比较路径的伪实现不能通过。
#[test]
fn replacement_at_the_same_plain_path_cannot_inherit_memory_identity() {
    let temp = tempfile::tempdir().expect("独立目录身份根");
    let root = temp.path().join("plain-project");
    std::fs::create_dir(&root).expect("普通项目目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let mut old = MemoryAccess::new(paths.clone(), Some(root.clone()));
    for alias in [MemoryScopeAlias::Project, MemoryScopeAlias::Worktree] {
        old.settings_save(
            alias,
            agent::MemoryRevision::INITIAL,
            vec![("original".into(), "旧目录的独立记忆".into())],
            "原记录",
            &format!("original-{alias:?}"),
        )
        .expect("旧目录合法身份可保存");
    }
    std::fs::rename(&root, temp.path().join("moved")).expect("移走普通目录");
    std::fs::create_dir(&root).expect("完全同一路径的新目录");
    let mut fresh = MemoryAccess::new(paths, Some(root));
    for alias in [MemoryScopeAlias::Project, MemoryScopeAlias::Worktree] {
        assert!(
            old.settings_snapshot(alias).is_err(),
            "旧会话不能读取替换目录范围"
        );
        assert!(
            old.settings_save(
                alias,
                agent::MemoryRevision::INITIAL,
                vec![("late".into(), "迟到写入".into())],
                "拒绝写入",
                "late-op"
            )
            .is_err(),
            "旧会话不能向替换目录写入"
        );
        assert_eq!(
            fresh
                .settings_snapshot(alias)
                .expect("新目录可绑定")
                .entries()
                .count(),
            0,
            "新身份不能继承旧记忆或迟到写入"
        );
    }
}

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
