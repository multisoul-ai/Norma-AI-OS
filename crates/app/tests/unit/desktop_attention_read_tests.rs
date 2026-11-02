use super::*;
use crate::workspace::desktop::DesktopArtifact;
use gpui::TestAppContext;
use storage::DesktopWorkspaceStore;

/// 打开回合成果清除该版本的全部文件提醒，不串到新回合或其他项目。
///
/// 数据构造：A/Agent 7 的 old 回合有 alpha、beta 两文件；new 回合再次修改 alpha；
///   B/Agent 7 同名回合和版本有 gamma。初始为 3 条成果 + 4 条文件 = 7 条提醒。
/// 执行过程：先对非 Git 目录打开成果 → 失败后保留提醒；
///   再初始化空 Git → 空变更列表仍保留提醒；写入 alpha/beta 后重开才清除。
///   随后请求单文件 A/new，再显式改看 beta，验证不会把未展示的 alpha 记成已读。
///   最后提交 alpha/beta，加载已无 diff 的成果期间收起右栏，回退预览不得重新展开它。
/// 预期结果：
///   - A/old 的 alpha、beta 已读且不再提醒，冻结成果引用仍存在。
///   - A/new 的 alpha 和 B/old 的 gamma 仍未读；剩 2 条成果 + 2 条文件 = 4 条提醒。
#[test]
fn opening_turn_review_clears_only_its_artifact_version() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("Review").expect("创建工作空间");
    let root_a = temp.path().join("a");
    let root_b = temp.path().join("b");
    std::fs::create_dir(&root_a).expect("创建项目 A");
    std::fs::create_dir(&root_b).expect("创建项目 B");
    let area_a = catalog.add_project(&workspace.id, &root_a).expect("添加 A");
    let area_b = catalog.add_project(&workspace.id, &root_b).expect("添加 B");
    let artifact = |area: &storage::DesktopProjectArea, turn: &str, version: &str, path: &str| {
        DesktopArtifact {
            area_id: area.id.clone(),
            node_id: 7,
            agent_profile: "Codex".into(),
            turn_id: turn.into(),
            path: area.root.join(path),
            source_path: path.into(),
            version: version.into(),
            saved_at_unix: 1,
            viewed: false,
        }
    };
    let mut test = TestAppContext::single();
    let (app, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.desktop_workspace.areas = vec![area_a.clone(), area_b.clone()];
        app.desktop_workspace.content_artifacts = vec![
            artifact(&area_a, "old", "v1", "alpha.txt"),
            artifact(&area_a, "old", "v1", "beta.txt"),
            artifact(&area_a, "new", "v2", "alpha.txt"),
            artifact(&area_b, "old", "v1", "gamma.txt"),
        ];
        app.mark_workspace_activity_dirty(cx);
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let screen = app.presented_overview(cx);
        assert_eq!(screen.summary()[0], 7, "三回合四文件都应先出现在待处理中");
        let action = screen.row(0).expect("应有 A/old 成果行").action.clone();
        app.activate_overview_action(action, window, cx);
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.ensure_file_workbench(window, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, cx| {
        assert!(
            !app.desktop_workspace.content_artifacts[0].viewed,
            "Git 加载失败不能把 alpha 标成已读"
        );
        assert!(
            !app.desktop_workspace.content_artifacts[1].viewed,
            "Git 加载失败不能把 beta 标成已读"
        );
        assert_eq!(
            app.presented_overview(cx).summary()[0],
            7,
            "未展示成果不能减少提醒"
        );
        assert!(
            app.desktop_workspace.workbench.git_error.is_some(),
            "夹具应实际触发 Git 加载失败"
        );
    });
    let init = std::process::Command::new("/usr/bin/git")
        .arg("init")
        .arg(&root_a)
        .output()
        .expect("初始化 Git");
    assert!(init.status.success(), "项目 A 应成功初始化 Git");
    app.update_in(cx, |app, window, cx| {
        let action = app
            .presented_overview(cx)
            .row(0)
            .expect("成果仍待处理")
            .action
            .clone();
        app.activate_overview_action(action, window, cx);
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.ensure_file_workbench(window, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.workbench.git.is_some(),
            "空仓库也应完成 Git 加载"
        );
        assert!(
            !app.desktop_workspace.content_artifacts[0].viewed,
            "空变更列表没有展示 alpha，不能已读"
        );
        assert!(
            !app.desktop_workspace.content_artifacts[1].viewed,
            "空变更列表没有展示 beta，不能已读"
        );
    });
    std::fs::write(root_a.join("alpha.txt"), "alpha").expect("写入 alpha");
    std::fs::write(root_a.join("beta.txt"), "beta").expect("写入 beta");
    app.update_in(cx, |app, window, cx| {
        let action = app
            .presented_overview(cx)
            .row(0)
            .expect("失败后成果仍在")
            .action
            .clone();
        app.activate_overview_action(action, window, cx);
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.ensure_file_workbench(window, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, cx| {
        let artifacts = &app.desktop_workspace.content_artifacts;
        assert!(artifacts[0].viewed, "打开 A/old 应将 alpha 标为已读");
        assert!(artifacts[1].viewed, "打开 A/old 应将 beta 标为已读");
        assert!(!artifacts[2].viewed, "不能清除 A/new 的同路径新版本");
        assert!(!artifacts[3].viewed, "不能清除 B 的同智能体同回合提醒");
        assert_eq!(artifacts.len(), 4, "已读不能删除原始成果");
        assert_eq!(
            app.presented_overview(cx).summary()[0],
            4,
            "应仅保留 A/new 与 B 的四条提醒"
        );
        assert!(
            app.desktop_workspace.drawers.right_open,
            "成果变更应在右侧打开"
        );
        assert_eq!(
            app.desktop_workspace.workbench.saved.area_id.as_deref(),
            Some(area_a.id.as_str()),
            "工作台应展示 A"
        );
    });
    app.update_in(cx, |app, window, cx| {
        let action = app
            .presented_overview(cx)
            .row(0)
            .expect("A/new 成果仍待处理")
            .action
            .clone();
        app.activate_overview_action(action, window, cx);
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.ensure_file_workbench(window, cx);
        app.select_workbench_path("beta.txt".into(), false, false, cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            !app.desktop_workspace.content_artifacts[2].viewed,
            "改看 beta 不能清除尚未展示的 alpha 新回合"
        );
        assert_eq!(
            app.desktop_workspace.workbench.saved.selected.as_deref(),
            Some("beta.txt"),
            "后台成果加载不能覆盖用户改选的文件"
        );
    });
    for args in [
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "Reviewed files",
        ],
    ] {
        let result = std::process::Command::new("/usr/bin/git")
            .arg("-C")
            .arg(&root_a)
            .args(args)
            .output()
            .expect("提交测试文件");
        assert!(
            result.status.success(),
            "测试文件应提交成功以形成无当前 diff 场景"
        );
    }
    app.update_in(cx, |app, window, cx| {
        let action = app
            .presented_overview(cx)
            .row(0)
            .expect("A/new 成果仍应待处理")
            .action
            .clone();
        app.activate_overview_action(action, window, cx);
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.ensure_file_workbench(window, cx);
        app.desktop_workspace.drawers.right_open = false;
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            !app.desktop_workspace.drawers.right_open,
            "无 diff 的冻结文件回退不得重新展开用户收起的右栏"
        );
        assert!(
            !app.desktop_workspace.content_artifacts[2].viewed,
            "隐藏的成果不得因为后台回退而变成已读"
        );
    });
}

/// 文件打开的路径校验和标签存储阶段都服从用户关闭右栏的取消操作。
///
/// 数据构造：一个工作空间、A/B 两个项目和 A 的 alpha.txt 成果；未提供冻结正文，
///   因为取消请求不应进入正文加载。最后无取消重开作为正向对照。
/// 执行过程：
///   1. 发起成果路径校验后立即通过生产抽屉入口关闭右栏，等待后台完成。
///   2. 直接发起带版本的标签存储后关闭右栏，等待后台完成。
///   3. 阻塞标签队列后请求成果，通过项目选择器切到 B，再释放队列。
///   4. 再次正常打开成果，等待标签安装，确认正常请求仍能展开右栏。
/// 预期结果：关闭和切项目都不能被迟到请求覆盖、不记已读；正常打开仍可显示文件标签。
#[test]
fn artifact_open_cancellation_covers_path_validation_and_tab_storage() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("Cancel").expect("创建工作空间");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("创建项目");
    std::fs::write(root.join("alpha.txt"), "alpha").expect("写入文件");
    let area = catalog.add_project(&workspace.id, &root).expect("添加项目");
    let root_b = temp.path().join("project-b");
    std::fs::create_dir(&root_b).expect("创建项目 B");
    let area_b = catalog
        .add_project(&workspace.id, &root_b)
        .expect("添加项目 B");
    let artifact = DesktopArtifact {
        area_id: area.id.clone(),
        node_id: 7,
        agent_profile: "Codex".into(),
        turn_id: "turn".into(),
        path: root.join("alpha.txt"),
        source_path: "alpha.txt".into(),
        version: "v1".into(),
        saved_at_unix: 1,
        viewed: false,
    };
    let mut test = TestAppContext::single();
    let (app, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area.clone(), area_b.clone()];
        app.desktop_workspace.content_artifacts = vec![artifact.clone()];
        app.desktop_workspace.drawers.right_open = true;
        app.open_desktop_artifact(artifact.clone(), cx);
        let mut closed = app.desktop_workspace.drawers;
        closed.right_open = false;
        app.apply_desktop_drawers(closed, cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            !app.desktop_workspace.drawers.right_open,
            "路径校验迟到不得重开右栏"
        );
        assert!(
            app.desktop_workspace.content_tabs.is_empty(),
            "取消路径校验不得安装文件标签"
        );
        assert!(
            !app.desktop_workspace.content_artifacts[0].viewed,
            "取消打开不得记已读"
        );
    });
    app.update(cx, |app, cx| {
        app.desktop_workspace.drawers.right_open = true;
        app.open_desktop_content_tab_with_version(
            storage::DesktopContentKind::File,
            artifact.path.to_string_lossy().into_owned(),
            "alpha.txt".into(),
            Some(area.id.clone()),
            Some(
                serde_json::to_string(&crate::workspace::desktop::DesktopArtifactVersion {
                    node_id: 7,
                    fingerprint: "v1".into(),
                    source_path: "alpha.txt".into(),
                })
                .expect("序列化版本"),
            ),
            cx,
        );
        let mut closed = app.desktop_workspace.drawers;
        closed.right_open = false;
        app.apply_desktop_drawers(closed, cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            !app.desktop_workspace.drawers.right_open,
            "标签存储迟到不得重开右栏"
        );
        assert!(
            app.desktop_workspace.content_tabs.is_empty(),
            "取消存储回调不得安装文件标签"
        );
        assert!(
            !app.desktop_workspace.content_artifacts[0].viewed,
            "未展示内容不得记已读"
        );
    });
    app.update(cx, |app, cx| app.open_file_workbench(cx));
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.ensure_file_workbench(window, cx));
    cx.run_until_parked();
    let (release, waiting) = futures::channel::oneshot::channel::<()>();
    app.update(cx, |app, cx| {
        app.desktop_workspace.content_tab_task = Some(cx.spawn(async move |_, _| {
            let _ = waiting.await;
        }));
        app.open_desktop_artifact(artifact.clone(), cx);
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        use crate::workspace::desktop::drawers::content::workbench::Choice;
        use ui::component::select::{SearchableVec, SelectEvent};
        let picker = app
            .desktop_workspace
            .workbench
            .project_picker
            .clone()
            .expect("应创建项目选择器");
        picker.update(cx, |_, cx| {
            cx.emit(SelectEvent::<SearchableVec<Choice>>::Confirm(Some(
                area_b.id.clone(),
            )))
        });
    });
    cx.run_until_parked();
    release.send(()).expect("释放项目切换前的标签队列");
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.workbench.saved.area_id.as_deref(),
            Some(area_b.id.as_str()),
            "选择器应确实切换到 B"
        );
        assert!(
            app.desktop_workspace
                .content_tabs
                .iter()
                .any(|tab| tab.selected && tab.kind == storage::DesktopContentKind::ProjectFiles),
            "迟到成果不得盖过用户切换后的工作台"
        );
        assert!(
            !app.desktop_workspace.content_artifacts[0].viewed,
            "切项目取消的成果不能已读"
        );
    });
    app.update(cx, |app, cx| app.open_desktop_artifact(artifact, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.drawers.right_open,
            "未取消的打开仍应展开右栏"
        );
        assert_eq!(
            app.desktop_workspace
                .content_tabs
                .iter()
                .filter(|tab| tab.kind == storage::DesktopContentKind::File)
                .count(),
            1,
            "未取消的打开应安装唯一文件标签"
        );
        assert!(
            !app.desktop_workspace.content_artifacts[0].viewed,
            "缺失冻结正文时仍不能记已读"
        );
    });
}

/// 仅暂存的单文件成果打开实际暂存差异，成功展示后才清除提醒。
///
/// 数据构造：一个空 Git 仓库，alpha.txt 内容为 staged alpha，写入后执行 git add；
///   index 与工作区相同，因此未暂存 diff 为空；HEAD 到 index 的暂存 diff 包含新增正文。
/// 执行过程：
///   1. 通过概览打开单文件成果，等待工作台目录与 Git 快照。
///   2. 等待目标差异加载及已读持久化。
/// 预期结果：差异包含 +staged alpha，不能展示空 diff；成果已读、原引用仍存在。
#[test]
fn staged_only_turn_review_opens_the_staged_diff_before_acknowledging() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("Staged").expect("创建工作空间");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("创建项目");
    std::fs::write(root.join("alpha.txt"), "staged alpha\n").expect("写入文件");
    for args in [vec!["init"], vec!["add", "alpha.txt"]] {
        let result = std::process::Command::new("/usr/bin/git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .output()
            .expect("准备仅暂存的文件");
        assert!(result.status.success(), "Git 夹具准备必须成功");
    }
    let area = catalog.add_project(&workspace.id, &root).expect("添加项目");
    let mut test = TestAppContext::single();
    let (app, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    app.update_in(cx, |app, window, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace.content_artifacts = vec![DesktopArtifact {
            area_id: area.id.clone(),
            node_id: 7,
            agent_profile: "Codex".into(),
            turn_id: "turn".into(),
            path: root.join("alpha.txt"),
            source_path: "alpha.txt".into(),
            version: "v1".into(),
            saved_at_unix: 1,
            viewed: false,
        }];
        app.activate_overview_action(
            OverviewAction::OpenReview {
                area_id: area.id.clone(),
                node_id: 7,
                fingerprint: "v1".into(),
                paths: vec!["alpha.txt".into()],
                direct_diff: true,
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.ensure_file_workbench(window, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        let diff = app
            .desktop_workspace
            .workbench
            .diff
            .as_deref()
            .expect("应加载差异");
        assert!(
            diff.contains("+staged alpha"),
            "仅暂存的文件必须显示暂存侧的新增正文"
        );
        assert!(!diff.trim().is_empty(), "空的未暂存差异不能冒充成功展示");
        assert!(
            app.desktop_workspace.content_artifacts[0].viewed,
            "实际差异展示后应记已读"
        );
        assert_eq!(
            app.desktop_workspace.content_artifacts.len(),
            1,
            "已读不能删除成果引用"
        );
    });
}
