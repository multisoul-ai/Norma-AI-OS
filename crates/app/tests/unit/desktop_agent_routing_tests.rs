use super::*;
use gpui::TestAppContext;

/// 同一目录在两个桌面工作空间中的 Agent 拥有独立 Hook 和路由，关闭一个不影响另一个。
///
/// 数据构造：同一个真实目录，2 个工作空间各含1个项目区域；使用临时 socket 和数据库。
///   注册数 = 桌面区域A(1) + 桌面区域B(1) = 2，停止A后剩余1。
/// 执行过程：通过生产加载入口准备A、B → 核对Hook配置和CLI路由 → 停止A → 核对B → 重载A。
/// 预期结果：A/B的工作目录相同、路由身份和token不同；A关闭后路由消失，B不变；重载A换发凭证。
#[test]
fn desktop_agent_routes_are_ready_and_isolated_before_restore() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("创建项目");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化数据库");
    let identities = store.workspace_identity_store();
    let catalog = store.desktop_workspace_store();
    let workspace_a = catalog.create("A").expect("创建A");
    let workspace_b = catalog.create("B").expect("创建B");
    let area_a = catalog
        .add_project(&workspace_a.id, &root)
        .expect("添加区域A");
    let area_b = catalog
        .add_project(&workspace_b.id, &root)
        .expect("添加区域B");
    let mut test = TestAppContext::single();
    test.update(|cx| {
        crate::norma::endpoint::install_at_path(temp.path().join("norma.sock"), cx)
            .expect("隔离endpoint")
    });
    let (host, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.desktop_workspace.active_id = Some(workspace_a.id.clone());
        app.desktop_workspace.areas = vec![area_a.clone()];
        app.ensure_desktop_area_session(area_a.id.clone(), window, cx);
        // 项目仍在后台加载时修改主题，完成加载后不得使用启动时的过期主题。
        app.set_theme(ui::ThemeId::SapphireNight, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.active_id = Some(workspace_b.id.clone());
        app.desktop_workspace.areas = vec![area_b.clone()];
        app.ensure_desktop_area_session(area_b.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let (a, b) = host.read_with(cx, |app, _| {
        (
            app.desktop_workspace.sessions[&(workspace_a.id.clone(), area_a.id.clone())].clone(),
            app.desktop_workspace.sessions[&(workspace_b.id.clone(), area_b.id.clone())].clone(),
        )
    });
    let config_a = a
        .read_with(cx, |app, _| app.runtime.agent_hook_config().cloned())
        .expect("区域A必须在恢复Agent前注入Hook配置");
    let config_b = b
        .read_with(cx, |app, _| app.runtime.agent_hook_config().cloned())
        .expect("区域B必须注入Hook配置");
    assert_eq!(
        a.read_with(cx, |app, _| app.theme_id),
        ui::ThemeId::SapphireNight,
        "后台加载完成后采用宿主最新主题"
    );
    let legacy = identities
        .resolve_or_create(&root, "Project")
        .expect("保留旧项目寻址身份");
    assert_ne!(
        legacy.workspace_id, config_a.workspace_id,
        "桌面画布不得覆盖手机/旧项目身份"
    );
    assert_ne!(
        config_a.workspace_id, config_b.workspace_id,
        "相同目录不能覆盖另一个画布的路由身份"
    );
    assert_ne!(
        config_a.token, config_b.token,
        "相同node_id的回调必须由独立token隔离"
    );
    assert!(
        !config_a.workspace_route_id.is_empty(),
        "A必须有CLI来源路由"
    );
    assert!(
        !config_b.workspace_route_id.is_empty(),
        "B必须有CLI来源路由"
    );
    assert_eq!(
        a.read_with(cx, |app, _| app
            .runtime
            .workspace_root()
            .map(std::path::Path::to_path_buf)),
        Some(area_a.root.clone()),
        "A保留所选项目目录"
    );
    assert_eq!(
        b.read_with(cx, |app, _| app
            .runtime
            .workspace_root()
            .map(std::path::Path::to_path_buf)),
        Some(area_a.root.clone()),
        "B允许共用目录"
    );
    cx.update(|_, cx| {
        let summaries = crate::norma::endpoint::workspace_summaries(cx);
        assert!(
            summaries
                .iter()
                .any(|item| item.workspace_id == config_a.workspace_id),
            "CLI必须能寻址A"
        );
        assert!(
            summaries
                .iter()
                .any(|item| item.workspace_id == config_b.workspace_id),
            "CLI必须能寻址B"
        );
        assert!(
            crate::workspace::sessions::find(paths.app_db_path(), &config_a.workspace_id, cx)
                .is_none(),
            "桌面项目生命周期应归桌面宿主，不能重复注册为旧后台项目"
        );
    });
    a.update(cx, |app, cx| app.stop_project(cx));
    cx.update(|_, cx| {
        let summaries = crate::norma::endpoint::workspace_summaries(cx);
        assert!(
            !summaries
                .iter()
                .any(|item| item.workspace_id == config_a.workspace_id),
            "关闭A必须撤销其路由"
        );
        assert!(
            summaries
                .iter()
                .any(|item| item.workspace_id == config_b.workspace_id),
            "关闭A不能撤销B路由"
        );
    });
    assert!(
        a.read_with(cx, |app, _| app.runtime.agent_hook_config().is_none()),
        "关闭A必须撤销Hook配置"
    );
    assert_eq!(
        b.read_with(cx, |app, _| app.runtime.agent_hook_config().cloned()),
        Some(config_b),
        "B配置不能被A关闭影响"
    );
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace
            .sessions
            .remove(&(workspace_a.id.clone(), area_a.id.clone()));
        app.desktop_workspace.active_id = Some(workspace_a.id.clone());
        app.desktop_workspace.areas = vec![area_a.clone()];
        app.ensure_desktop_area_session(area_a.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let reloaded = host.read_with(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace_a.id, area_a.id)].clone()
    });
    let fresh = reloaded
        .read_with(cx, |app, _| app.runtime.agent_hook_config().cloned())
        .expect("重载必须注册Hook");
    assert_eq!(
        fresh.workspace_id, config_a.workspace_id,
        "重载保留画布的稳定寻址身份"
    );
    assert_ne!(
        fresh.workspace_route_id, config_a.workspace_route_id,
        "旧路由凭证不能继续有效"
    );
    assert_ne!(fresh.token, config_a.token, "旧Hook token不能进入新会话");
    reloaded.update(cx, |app, cx| app.stop_project(cx));
    b.update(cx, |app, cx| app.stop_project(cx));
}
