use super::*;
use gpui::{AppContext, TestAppContext};
use serde_json::{json, Value};
use std::path::Path;

fn write(path: &Path, value: Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

fn make_app(
    test: &mut TestAppContext,
    paths: storage::DatabasePaths,
) -> gpui::Entity<MultisoulApp> {
    test.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    })
}

/// 即使配置先到达，数据迁移完成前也不能启动服务。
/// 数据：启用设置和已注册项目，准备阶段先失败再重试成功。
/// 执行：在未准备、准备中、准备失败、准备成功四个时刻申请启动。
/// 预期：前三阶段均无ticket，成功后只允许一个启动；准备不能并发执行。
#[test]
fn brain_data_preparation_gates_service_start_and_retries_failure() {
    let owner = owner::SharedBrain::default();
    let settings = config::BrainSettings {
        enabled: true,
        ..Default::default()
    };
    owner.configure(settings.clone(), false);
    let _client = owner.register(
        norma::protocol::BrainScope {
            workspace_id: "alpha".into(),
            route_id: "route".into(),
        },
        "/alpha".into(),
    );
    let now = Instant::now();
    assert!(owner.begin_start(now).is_none(), "未准备数据时不能启动");
    assert!(owner.begin_prepare(now), "第一次准备可开始");
    assert!(!owner.begin_prepare(now), "多个项目不能同时准备");
    assert!(owner.begin_start(now).is_none(), "迁移中仍不启动");
    owner.finish_prepare(Err("fixture migration failure".into()));
    assert!(owner.begin_start(now).is_none(), "迁移失败不能用空状态启动");
    assert!(!owner.begin_prepare(now), "失败后避免逐帧重试IO");
    assert!(
        owner.begin_prepare(now + START_RETRY + Duration::from_secs(1)),
        "截止后允许重试"
    );
    owner.finish_prepare(Ok(data::Prepared {
        directory: "/global/current".into(),
        settings,
    }));
    assert!(owner.begin_start(now).is_some(), "准备成功才允许启动");
    assert!(
        owner.begin_start(now).is_none(),
        "准备成功后也只能一个启动任务"
    );
}

/// 真实App后台迁移失败时，不能启用服务或用默认设置覆盖旧配置。
/// 数据：有效启用配置与损坏的旧目标文件，隔离数据库和brain目录。
/// 执行：调用生产刷新入口等待失败，再尝试修改设置。
/// 预期：无全局入口、无启动资格；原文件字节保持，错误支持中英文。
#[test]
fn brain_data_failed_preparation_preserves_originals_and_rejects_writes() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("brain");
    let old = base
        .join("projects")
        .join(brain::migration::workspace_hash("alpha"));
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("goals.json"), "broken goals").unwrap();
    write(
        &base.join("config.json"),
        json!({"enabled":true,"speak":false}),
    );
    let original = std::fs::read(base.join("config.json")).unwrap();
    let mut test = TestAppContext::single();
    let app = make_app(
        &mut test,
        storage::DatabasePaths::isolated_test(&dir.path().join("data")),
    );
    app.update(&mut test, |app, cx| {
        app.refresh_brain_settings_from(base.clone(), cx)
    });
    test.run_until_parked();
    app.update(&mut test, |app, cx| {
        let owner = owner::shared(cx);
        assert!(owner.directory().is_none(), "失败没有就绪目录");
        assert!(owner.settings().is_none(), "不安装默认空配置");
        assert!(
            owner.begin_start(Instant::now()).is_none(),
            "数据失败后不能启动"
        );
        let error = app
            .desktop_workspace
            .error
            .as_ref()
            .expect("UI展示迁移失败");
        assert!(
            error
                .resolve_for(ui::i18n::Locale::ZhCn)
                .contains("原文件已保留"),
            "中文提示保留原数据"
        );
        assert!(
            error
                .resolve_for(ui::i18n::Locale::En)
                .contains("Original files were preserved"),
            "英文提示同义"
        );
        app.set_brain_speaking(true, cx);
    });
    brain::persistence::shared_writer()
        .flush()
        .recv()
        .unwrap()
        .unwrap();
    assert!(
        !brain::migration::active_dir(&base).exists(),
        "失败不发布全局入口"
    );
    assert_eq!(
        std::fs::read(base.join("config.json")).unwrap(),
        original,
        "禁止设置操作覆盖旧配置"
    );
    assert_eq!(
        std::fs::read_to_string(old.join("goals.json")).unwrap(),
        "broken goals",
        "保留损坏原文供修复"
    );
}

/// 真实App实体的后台准备读取普通项目与桌面画布元数据，迁移后所有读写使用全局目录。
/// 数据：同一项目根的普通身份和桌面画布身份各有旧目标，画布有一条完整提议，旧配置静音。
/// 执行：生产刷新入口后台准备，加载并忽略提议、修改语音开关，再创建第二个App上下文模拟重开。
/// 预期：两目标来源准确，提议完整且只消费一次，新配置写全局；重开不复活旧提议，备份原样保留。
#[test]
fn brain_data_app_preparation_migrates_and_consumes_only_global_state() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let identity = storage::WorkspaceIdentityStore::new(database.clone())
        .resolve_or_create(&root, "Project")
        .unwrap();
    let catalog = storage::DesktopWorkspaceStore::new(database);
    let workspace = catalog.create("Workspace").unwrap();
    let area = catalog.add_project(&workspace.id, &root).unwrap();
    let scope = format!("desktop:{}:{}", workspace.id, area.id);
    let base = dir.path().join("brain");
    for id in [&identity.workspace_id, &scope] {
        let old = base
            .join("projects")
            .join(brain::migration::workspace_hash(id));
        write(
            &old.join("goals.json"),
            json!([{"agent":"Alan","text":format!("{id} complete original goal")} ]),
        );
    }
    let old_proposals = base
        .join("projects")
        .join(brain::migration::workspace_hash(&scope))
        .join("proposals.json");
    write(
        &old_proposals,
        json!({"next_id":1,"items":[{"id":1,"agent":"Alan","title":"验证迁移","task":"keep the entire original task","why":"保留旧数据","at":100}]}),
    );
    let original_proposals = std::fs::read(&old_proposals).unwrap();
    write(
        &base.join("config.json"),
        json!({"enabled":false,"speak":false,"always_ask":["Write"]}),
    );
    let original_config = std::fs::read(base.join("config.json")).unwrap();
    let mut test = TestAppContext::single();
    let app = make_app(&mut test, paths.clone());
    app.update(&mut test, |app, cx| {
        app.workspace_id = Some(scope.clone());
        app.workspace_route_id = Some("migration-route".into());
        app.runtime.set_workspace_root(root.clone());
        app.ensure_brain_client(cx);
        app.refresh_brain_settings_from(base.clone(), cx);
    });
    test.run_until_parked();
    app.update(&mut test, |app, cx| app.load_brain_proposals(cx));
    test.run_until_parked();
    let current = brain::migration::active_dir(&base);
    let goals = brain::goal::GoalStore::open(current.clone()).goals();
    assert_eq!(goals.len(), 2, "普通身份和画布目标全部迁移");
    for id in [&identity.workspace_id, &scope] {
        assert_eq!(
            goals.iter().filter(|goal| &goal.workspace_id == id).count(),
            1,
            "来源{id}准确"
        );
    }
    app.update(&mut test, |app, cx| {
        assert_eq!(
            app.brain.proposals.items()[0].task,
            "keep the entire original task",
            "生产加载器读取完整原提议"
        );
        assert!(
            !app.brain.settings.as_ref().unwrap().speak,
            "旧静音设置被加载"
        );
        app.set_brain_speaking(true, cx);
        assert!(app.decline_brain_proposal(1), "实际消费原提议");
        assert!(!app.decline_brain_proposal(1), "不能重复消费");
    });
    brain::persistence::shared_writer()
        .flush()
        .recv()
        .unwrap()
        .unwrap();
    test.run_until_parked();
    let config: Value =
        serde_json::from_slice(&std::fs::read(current.join("config.json")).unwrap()).unwrap();
    assert_eq!(config["speak"], true, "设置写回已迁移的全局目录");
    let saved: Value = serde_json::from_slice(
        &std::fs::read(brain::migration::proposal_file(&current, &scope)).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["workspace_id"], scope, "消费后的空快照也保留身份");
    assert_eq!(
        saved["items"].as_array().unwrap().len(),
        0,
        "提议删除已持久化"
    );
    assert_eq!(
        std::fs::read(&old_proposals).unwrap(),
        original_proposals,
        "不移动或覆盖旧提议"
    );
    assert_eq!(
        std::fs::read(base.join("config.json")).unwrap(),
        original_config,
        "旧配置仍是备份"
    );
    let mut restarted = TestAppContext::single();
    let second = make_app(&mut restarted, paths);
    second.update(&mut restarted, |app, cx| {
        app.workspace_id = Some(scope);
        app.refresh_brain_settings_from(base, cx);
    });
    restarted.run_until_parked();
    second.update(&mut restarted, |app, cx| app.load_brain_proposals(cx));
    restarted.run_until_parked();
    second.read_with(&restarted, |app, _| {
        assert!(
            app.brain.settings.as_ref().unwrap().speak,
            "重开读取新全局设置"
        );
        assert!(app.brain.proposals_ready, "重开完成实际读取");
        assert!(
            app.brain.proposals.items().is_empty(),
            "重开不复活旧备份里的提议"
        );
    });
}
