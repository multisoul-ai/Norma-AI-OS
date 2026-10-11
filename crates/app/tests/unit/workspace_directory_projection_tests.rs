use crate::workspace::directory::project_runtime_directory;
use norma::workspace_dispatch::*;
use std::sync::{Arc, Mutex};

fn catalog(card: WorkspaceCardRef) -> WorkspaceDirectory {
    let project = WorkspaceProjectRef {
        workspace_id: "space".into(),
        project_id: "project".into(),
    };
    WorkspaceDirectory {
        workspace_id: "space".into(),
        revision: 1,
        catalog_fingerprint: "catalog".into(),
        authorization_revision: "desktop".into(),
        authorized_area_ids: vec!["area".into()],
        covered_area_ids: vec!["area".into()],
        missing_area_ids: vec![],
        registration_complete: true,
        projects: vec![WorkspaceDirectoryProject {
            reference: project.clone(),
            name: "Web".into(),
            area_ids: vec!["area".into()],
        }],
        areas: vec![WorkspaceDirectoryArea {
            area_id: "area".into(),
            project: project.clone(),
            name: "Web".into(),
        }],
        call_keys: vec![WorkspaceDirectoryCallKey {
            key: "alan".into(),
            card: card.clone(),
            state: WorkspaceCallKeyStatus::Active,
        }],
        roles: vec![WorkspaceDirectoryRole {
            card,
            project,
            name: "Alan".into(),
            speech_alias: None,
            enabled: true,
            submission: WorkspaceSubmissionCapability::Supported,
            execution: None,
        }],
    }
}

// 场景：已有普通无定义OpenCode实例，需要从完整目录投影真实执行指纹；
// 构造记录型PTY并冻结持久卡，执行纯内存投影；预期零新增输入、PTY数量/焦点不变。
#[test]
fn workspace_directory_live_projection_is_readonly_and_keeps_focus() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let observed = writes.clone();
    let mut runtime = runtime::Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            observed.clone(),
        )))
    }));
    let node = runtime
        .spawn_agent(
            canvas::AgentProfile::OpenCode,
            canvas::CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("应创建已有记录型PTY");
    let target = runtime
        .workspace_card_target("space", "area", 7, node)
        .expect("应捕获已有实例");
    let focus_before = format!("{:?}", runtime.focus);
    let mut directory = catalog(target.card.clone());
    project_runtime_directory(&mut directory, "area", &runtime, &[], 7);
    assert_eq!(
        directory.roles[0].execution,
        Some(target.execution),
        "投影只可使用精确原实例的执行身份"
    );
    assert_eq!(runtime.live_len(), 1, "目录投影不得创建额外PTY");
    assert!(
        writes.lock().expect("输入记录锁应可读").is_empty(),
        "目录投影不得向卡片写任何字节"
    );
    assert_eq!(
        format!("{:?}", runtime.focus),
        focus_before,
        "目录投影不得导航或抢焦点"
    );
    assert!(directory.is_complete(), "原实例匹配时授权目录仍完整");
}

// 场景：读完持久目录后Runtime把同node_id换成新卡，或出现未登记新增卡；
// 构造旧目录与替换文档后执行投影；预期coverage gap，不刷新旧绑定为新卡指纹。
#[test]
fn workspace_directory_projection_marks_replaced_or_unregistered_cards_missing() {
    let mut runtime = runtime::Runtime::new_for_workspace("/tmp");
    let add = |runtime: &mut runtime::Runtime| {
        runtime.document.add_node(
            canvas::NodeKind::Agent(canvas::AgentSpec {
                profile: canvas::AgentProfile::OpenCode,
                alias: Some("Alan".into()),
                definition_id: None,
                mobile_control: false,
            }),
            canvas::CanvasRect::new(0., 0., 640., 400.),
        )
    };
    let node = add(&mut runtime);
    let original = runtime
        .workspace_card_target("space", "area", 7, node)
        .expect("应捕获原持久卡");
    let mut directory = catalog(original.card.clone());
    runtime.document = canvas::Document::new();
    let replacement = add(&mut runtime);
    assert_eq!(replacement, node, "必须构造相同局部node_id的替换反例");
    project_runtime_directory(&mut directory, "area", &runtime, &[], 7);
    assert!(
        !directory.is_complete(),
        "原目录与当前内存身份不一致不能自动派工"
    );
    assert_eq!(
        directory.missing_area_ids,
        vec!["area"],
        "应指明替换区域覆盖缺口"
    );
    assert_eq!(
        directory.roles[0].card, original.card,
        "不能悄悄把原持久目录卡改成新身份"
    );
    assert_eq!(
        directory.roles[0].execution, None,
        "替换卡指纹不能投影到旧卡"
    );
    assert_eq!(runtime.live_len(), 0, "发现缺口不能启动PTY修补目录");
}

// 场景：持久卡已加载Document但未启动PTY，目录查询必须保持冷状态；构造无定义卡，
// 纯内存投影后预期指纹仍None，节点/名称/焦点不变且不修补启动任何PTY。
#[test]
fn document_only_card_does_not_gain_execution_fingerprint() {
    let mut runtime = runtime::Runtime::new_for_workspace("/tmp");
    let node = runtime.document.add_node(
        canvas::NodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::OpenCode,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(0., 0., 640., 400.),
    );
    let identity = runtime
        .workspace_card_target("space", "area", 0, node)
        .expect("Document卡有持久身份");
    let mut directory = catalog(identity.card.clone());
    project_runtime_directory(&mut directory, "area", &runtime, &[], 0);
    assert_eq!(
        directory.roles[0].execution, None,
        "未启动PTY的卡不能虚构live fingerprint"
    );
    assert_eq!(runtime.live_len(), 0, "读取目录不可启动PTY");
    assert_eq!(directory.roles[0].card, identity.card, "原持久身份保持不变");
    assert!(
        directory.is_complete(),
        "Document-only卡仍应完整参与候选目录"
    );
}

// 场景：发起A空间目录读取后用户立即切到B，两个空间有相同node_id的live卡；
// 构造真实隔离数据库/retained sessions/记录PTY，调用生产后台reader；预期只overlay A原身份，
// 所有卡零新增输入、PTY与焦点不变，且导航保留B，不按active_id改取B事实。
#[test]
fn workspace_directory_reader_keeps_frozen_source_across_view_switch() {
    use crate::app::{background_project::PreparedProject, MultisoulApp};
    use gpui::{AppContext as _, TestAppContext};
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离AppDB");
    let catalog = store.desktop_workspace_store();
    let a = catalog.create("A").expect("应创建源空间");
    let b = catalog.create("B").expect("应创建导航目标空间");
    let writes = Arc::new(Mutex::new(Vec::new()));
    let mut fixtures = vec![];
    for (workspace, name) in [(&a, "Alan"), (&b, "Bella")] {
        let area = catalog
            .add_project(&workspace.id, &root)
            .expect("应添加独立画布");
        let mut runtime = runtime::Runtime::new_for_workspace(&area.root);
        let observed = writes.clone();
        runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::recording(
                tx,
                observed.clone(),
            )))
        }));
        let node = runtime
            .spawn_agent_with_alias(
                canvas::AgentProfile::OpenCode,
                Some(name.into()),
                canvas::CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .expect("应构造已存在PTY");
        let card = runtime
            .workspace_card_target(&workspace.id, &area.id, 0, node)
            .expect("应捕获原实例")
            .card;
        storage::ProjectStore::initialize_for_desktop_area(
            &paths,
            &area.root,
            &workspace.id,
            &area.id,
        )
        .expect("应初始化区域CanvasDB")
        .canvas_store()
        .save_snapshot(&runtime.document.to_snapshot())
        .expect("应持久保存原卡");
        storage::WorkspaceAgentRoleStore::new(paths.app_db_path().to_owned())
            .create(&card, name, true)
            .expect("应登记角色");
        let prepared = PreparedProject::load_for_desktop(paths.clone(), area.clone())
            .expect("应准备已有空间实例");
        fixtures.push((area, runtime, card, prepared));
    }
    let mut test = TestAppContext::single();
    let (host, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    cx.run_until_parked();
    let mut sessions = vec![];
    for (area, runtime, card, prepared) in fixtures {
        let session = cx.new(|cx| {
            let mut app = MultisoulApp::from_background_project(prepared, cx);
            app.runtime = runtime;
            app.workspace_route_id = Some(format!("route-{}", area.desktop_workspace_id));
            app.desktop_host = Some(host.downgrade());
            app
        });
        sessions.push((area, card, session));
    }
    host.update_in(cx, |app, window, cx| {
        crate::workspace::desktop::sessions::register(
            paths.app_db_path().to_owned(),
            window.window_handle(),
            host.clone(),
            cx,
        );
        app.desktop_workspace.active_id = Some(a.id.clone());
        for (area, _, session) in &sessions {
            app.desktop_workspace.sessions.insert(
                (area.desktop_workspace_id.clone(), area.id.clone()),
                session.clone(),
            );
        }
    });
    let result = Arc::new(Mutex::new(None));
    let capture = result.clone();
    host.update(cx, |app, cx| {
        let task = app.read_workspace_dispatch_directory(a.id.clone(), "desktop".into(), None, cx);
        cx.spawn(async move |_, _| {
            *capture.lock().expect("结果记录锁应可写") = Some(task.await);
        })
        .detach();
        app.desktop_workspace.active_id = Some(b.id.clone());
    });
    cx.run_until_parked();
    let directory = result
        .lock()
        .expect("结果锁应可读")
        .take()
        .expect("后台读取应完成")
        .expect("完整目录应可读");
    assert_eq!(directory.workspace_id, a.id, "目录必须绑定发送时的源空间");
    assert_eq!(directory.roles.len(), 1, "不能混入另一空间的角色");
    assert_eq!(
        directory.roles[0].card, sessions[0].1,
        "相同node_id不得使目标跳到B卡"
    );
    assert_ne!(directory.roles[0].card, sessions[1].1, "B卡不进入A源目录");
    sessions[0].2.read_with(cx, |app, _| {
        let original = app
            .runtime
            .workspace_card_target(
                &a.id,
                &sessions[0].0.id,
                app.workspace_dispatch_route_generation(),
                sessions[0].1.node_id,
            )
            .expect("应捕获现存A指纹");
        assert_eq!(
            directory.roles[0].execution,
            Some(original.execution),
            "overlay必须来自A保留实例且使用共用route generation"
        );
        assert_eq!(app.runtime.live_len(), 1, "A不能新增PTY");
    });
    sessions[1].2.read_with(cx, |app, _| {
        assert_eq!(app.runtime.live_len(), 1, "B不能新增PTY");
    });
    assert!(
        writes.lock().expect("输入记录锁应可读").is_empty(),
        "查询所有卡零新增输入"
    );
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.active_id.as_deref(),
            Some(b.id.as_str()),
            "迟到查询不能导航回A"
        );
    });
    assert!(directory.is_complete(), "源空间完整覆盖应保留");
}
