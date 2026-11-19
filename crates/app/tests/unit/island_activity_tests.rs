use crate::app::{AppScreen, MultisoulApp};
use gpui::{AppContext, TestAppContext};

/// 场景：A 是当前空间，B 的项目仍在后台运行；关掉宿主窗口后，岛仍应看到 B。
/// 数据：隔离数据库中 A/B 各一个项目，B 有一张待提问的 Claude 卡片（echo PTY，不调用模型）；宿主没有活动窗口。
/// 执行：通过活动变更入口触发后台投影，读取全局模型；再 detach 后刷新并释放宿主。
/// 预期：两个空间同时存在、B 的实时卡片可见；detach 不丢 B，stop 才清空来源。
#[test]
fn island_activity_includes_inactive_workspace_and_retained_host() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("创建项目");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = store.desktop_workspace_store();
    let a = catalog.create("Alpha").expect("创建 A");
    let b = catalog.create("Beta").expect("创建 B");
    catalog.add_project(&a.id, &root).expect("添加 A 项目");
    let b_area = catalog.add_project(&b.id, &root).expect("添加 B 项目");
    let mut cx = TestAppContext::single();
    let host =
        cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
    let mut expected_question = None;
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        );
        app.desktop_host = Some(host.downgrade());
        let raw = serde_json::from_str(include_str!(
            "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
        ))
        .expect("读取 Claude 提问夹具");
        let (target, _, _) =
            crate::test_brain_question::make_question_from_raw(&mut app, cx, &root, raw);
        if let canvas::CanvasNodeKind::Agent(spec) = &mut app
            .runtime
            .document
            .node_mut(target.node_id)
            .expect("提问卡片存在")
            .kind
        {
            spec.alias = Some("Background Ada".into());
        }
        expected_question = Some(target);
        app
    });
    let model = cx.update(crate::island::model);
    host.update(&mut cx, |app, cx| {
        app.screen = AppScreen::DesktopWorkspace;
        app.window_active = false;
        app.desktop_workspace.active_id = Some(a.id.clone());
        app.desktop_workspace.workspaces = catalog.list().expect("加载目录");
        app.desktop_workspace.areas = catalog.list_projects(&a.id).expect("加载 A");
        app.desktop_workspace
            .sessions
            .insert((b.id.clone(), b_area.id.clone()), session);
        app.note_workspace_activity_changed(cx);
    });
    cx.run_until_parked();
    model.read_with(&cx, |model, _| {
        assert_eq!(model.workspaces.len(), 2, "当前空间之外的 B 不能遗漏");
        let background = model
            .workspaces
            .iter()
            .find(|ws| ws.id == b.id)
            .expect("B 存在");
        assert!(background.projects[0].loaded, "无前台窗口仍有实时会话");
        assert_eq!(
            background.projects[0].questions.len(),
            1,
            "后台空间的待提问不能遗漏"
        );
        assert_eq!(
            Some(&background.projects[0].questions[0].snapshot.question.target),
            expected_question.as_ref(),
            "提问必须保留原始 provider/run/request 身份"
        );
        assert_eq!(
            background.projects[0].overview.idle[0].name, "Background Ada",
            "B 卡片不能取 A 的缓存"
        );
    });
    let notifications = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = notifications.clone();
    let _observer =
        cx.update(|cx| cx.observe(&model, move |_, _| observed.set(observed.get() + 1)));
    host.update(&mut cx, |app, cx| app.note_workspace_activity_changed(cx));
    cx.run_until_parked();
    assert_eq!(notifications.get(), 0, "相同可见内容不应通知岛窗口重绘");
    host.update(&mut cx, |app, cx| {
        app.detach_desktop_host_window(cx);
        app.note_workspace_activity_changed(cx);
    });
    cx.run_until_parked();
    model.read_with(&cx, |model, _| {
        assert_eq!(model.workspaces.len(), 2, "保留宿主关窗不能移除贡献");
    });
    host.update(&mut cx, |app, cx| {
        app.note_workspace_activity_changed(cx);
        app.stop_desktop_sessions(cx);
    });
    cx.run_until_parked();
    model.read_with(&cx, |model, _| {
        assert!(model.workspaces.is_empty(), "真正释放后不能继续显示旧来源");
    });
    assert!(
        notifications.get() > 0,
        "移除来源必须通知，证明重复数据为零的观察器有效"
    );
}

/// 场景：Launchpad 上存在一个从未打开的空间。
/// 数据：隔离数据库仅有一个空空间；宿主停在 Launchpad，没有活动空间也没有实时会话。
/// 执行：触发一次目录刷新，等待后台投影完成，再读取全局岛模型。
/// 预期：岛只加载已打开的工作空间，未打开的空间不出现在岛缓存中。
#[test]
fn island_activity_skips_unopened_workspace() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    store
        .desktop_workspace_store()
        .create("Unopened")
        .expect("创建空间");
    let mut cx = TestAppContext::single();
    let host = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let model = cx.update(crate::island::model);
    host.update(&mut cx, |app, cx| app.refresh_desktop_previews(cx));
    cx.run_until_parked();
    model.read_with(&cx, |model, _| {
        assert!(model.workspaces.is_empty(), "未打开的空间不能进入岛缓存")
    });
}

/// 场景：其他工作空间的持久化读取失败，当前窗口已有完整内存快照。
/// 数据：用目录路径模拟无法打开的 SQLite 文件，并提供一个已捕获的空工作空间。
/// 执行：在后台投影边界同时请求全局目录与当前窗口投影。
/// 预期：全局读取明确失败；有效的当前窗口投影仍返回，不能被无关存储错误吞掉。
#[test]
fn island_activity_keeps_current_projection_when_global_storage_fails() {
    let temp = tempfile::tempdir().expect("创建无效数据库路径");
    let database = temp.path().to_owned();
    let facts = norma::mobile::WorkspaceActivityFacts {
        workspace: norma::mobile::DesktopChatWorkspace {
            id: "current".into(),
            name: "Current".into(),
            revision: 0,
            projects: Vec::new(),
        },
        observed: true,
        approvals: Vec::new(),
        reviews: Vec::new(),
    };
    let projection = super::project::project_workspaces(super::capture::ActivityCapture {
        generation: 1,
        database: database.clone(),
        live: Default::default(),
        current: Some(super::ActivityInput {
            database,
            facts,
            cached_facts: None,
            questions: Vec::new(),
            proposals: Vec::new(),
            area_ids: Vec::new(),
            presentation: Vec::new(),
            current_area_id: String::new(),
            files: Vec::new(),
            presence: Vec::new(),
        }),
    });
    assert!(projection.islands.is_err(), "存储错误必须明确报告");
    assert!(
        projection.current.is_some(),
        "全局读取错误不能丢掉当前窗口的内存投影"
    );
}

/// 场景：同一个工作空间有两个项目，提议只属于其中一个。
/// 数据：项目 a 带着提议 7，项目 b 没有；另有一条 area 不匹配的提议 9。
/// 执行：用生产的岛投影按 area 过滤。
/// 预期：a 只有 7；b 为空；9 不会出现在任何一个项目上。
#[test]
fn island_workspace_keeps_each_proposal_on_its_own_area() {
    use norma::mobile::{
        DesktopChatProject, DesktopChatWorkspace, RunOverviewProjectPresence,
        WorkspaceActivityFacts, WorkspaceActivityView,
    };
    let project = |area: &str| DesktopChatProject {
        area_id: area.into(),
        project_id: area.into(),
        name: area.into(),
        root: format!("/{area}"),
        agents: Vec::new(),
        definitions: Vec::new(),
    };
    let facts = WorkspaceActivityFacts {
        workspace: DesktopChatWorkspace {
            id: "ws".into(),
            name: "Norma".into(),
            revision: 0,
            projects: vec![project("a"), project("b")],
        },
        observed: true,
        approvals: Vec::new(),
        reviews: Vec::new(),
    };
    let proposal = |id: u64, area: &str| crate::island::IslandProposal {
        id,
        area_id: area.into(),
        agent: "Ada".into(),
        title: "下一步".into(),
        task: "去做".into(),
        why: "因为空着".into(),
        block: None,
        creates_card: false,
    };
    let input = super::ActivityInput {
        database: "db.sqlite".into(),
        questions: Vec::new(),
        proposals: vec![proposal(7, "a"), proposal(9, "gone")],
        facts: facts.clone(),
        cached_facts: None,
        area_ids: vec!["a".into(), "b".into()],
        presentation: Vec::new(),
        current_area_id: String::new(),
        files: Vec::new(),
        presence: vec![
            RunOverviewProjectPresence {
                area_id: "a".into(),
                loaded: true,
                updated_at_unix_ms: None,
            },
            RunOverviewProjectPresence {
                area_id: "b".into(),
                loaded: true,
                updated_at_unix_ms: None,
            },
        ],
    };
    let view = WorkspaceActivityView::from_facts(&facts);
    let island = super::project::island_workspace(&input, &view);
    let alpha = island
        .projects
        .iter()
        .find(|project| project.area_id == "a")
        .expect("项目 a 必须在");
    let beta = island
        .projects
        .iter()
        .find(|project| project.area_id == "b")
        .expect("项目 b 必须在");
    assert_eq!(alpha.proposals.len(), 1, "a 只有自己的一条提议");
    assert_eq!(alpha.proposals[0].id, 7, "a 上的提议是 7");
    assert!(beta.proposals.is_empty(), "b 不能看到 a 的提议");
    assert!(
        island
            .projects
            .iter()
            .all(|project| project.proposals.iter().all(|item| item.id != 9)),
        "对不上项目的提议 9 不能出现"
    );
}
