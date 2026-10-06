use super::*;
use gpui::TestAppContext;

/// 场景：正常启动的激活壳挂载工作台后，手机能定位同一桌面宿主，关窗后保持其身份。
///
/// 数据构造：独立数据库 A 和无关数据库 B；一个真实 GateRoot 窗口；不读用户数据库或模型凭据。
/// 执行过程：创建激活壳 → 使用隔离数据库挂载正式工作台 → 查询桌面宿主与无关数据库。
/// 预期结果：A 恰有当前工作台；B 没有宿主；不得因启动入口不同导致手机目录不可用。
#[test]
fn gated_workbench_registers_exact_desktop_host() {
    let dir = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let database = paths.app_db_path().to_owned();
    let mut test = TestAppContext::single();
    let (gate, cx) = test.add_window_view(|window, cx| {
        let mut gate = GateRoot::new(BootPhase::Activation, window, cx);
        gate.mount_workbench_from_context(paths, store, window, cx);
        gate
    });
    let expected = gate.read_with(cx, |gate, _| gate.workbench.clone().expect("应挂载工作台"));
    cx.update(|_, cx| {
        let hosts = crate::workspace::desktop::sessions::hosts(&database, cx);
        assert_eq!(hosts.len(), 1, "正常启动入口必须注册可供手机定位的桌面宿主");
        assert_eq!(
            hosts[0].entity_id(),
            expected.entity_id(),
            "手机必须复用激活壳内的真实工作台"
        );
        assert!(
            crate::workspace::desktop::sessions::hosts(&dir.path().join("other.sqlite"), cx)
                .is_empty(),
            "不能把宿主路由到其他数据库"
        );
    });
}

/// 场景：手机服务保留工作台后，从 Dock 重开仍复用同一宿主和项目会话。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离数据库 A；一个工作空间和一个项目会话，不启动 PTY 或网络服务。
///   首次注册宿主数 = 1；关窗保留后 = 1；重开后仍 = 1，不能变为 1 + 1 = 2。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 正式 GateRoot 挂载工作台，保存当前工作空间和项目会话。
///   2. 移除窗口，调用生产关窗处理并保留后台宿主。
///   3. 使用同一数据库再次挂载 GateRoot，模拟 Dock 的重开入口。
///   4. 按手机使用的工作空间和区域身份重新定位项目会话。
///
/// 预期结果：
///   - 正断言：宿主和项目会话身份不变，当前工作空间仍保留，宿主数恰为 1。
///   - 负断言：重开后不能残留另一份无窗口宿主，不能定位到无关数据库。
#[test]
fn gated_workbench_reopen_reuses_retained_mobile_host() {
    let dir = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let database = paths.app_db_path().to_owned();
    let workspace = store
        .desktop_workspace_store()
        .create("Mobile")
        .expect("应创建工作空间");
    let area = store
        .desktop_workspace_store()
        .add_project(&workspace.id, dir.path())
        .expect("应关联隔离项目");
    let mut test = TestAppContext::single();
    let (original, project) = {
        let (gate, cx) = test.add_window_view(|window, cx| {
            let mut gate = GateRoot::new(BootPhase::Activation, window, cx);
            gate.mount_workbench_from_context(paths.clone(), store.clone(), window, cx);
            gate
        });
        let original = gate.read_with(cx, |gate, _| gate.workbench.clone().expect("应挂载工作台"));
        let project =
            cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
        original.update(cx, |app, _| {
            app.desktop_workspace.active_id = Some(workspace.id.clone());
            app.desktop_workspace
                .sessions
                .insert((workspace.id.clone(), area.id.clone()), project.clone());
        });
        cx.update(|window, _| window.remove_window());
        cx.run_until_parked();
        (original, project)
    };
    test.update(|cx| crate::workspace::desktop::sessions::windows_closed(true, cx));
    let (gate, cx) = test.add_window_view(|window, cx| {
        let mut gate = GateRoot::new(BootPhase::Activation, window, cx);
        gate.mount_workbench_from_context(paths, store, window, cx);
        gate
    });
    let reopened = gate.read_with(cx, |gate, _| gate.workbench.clone().expect("应重开工作台"));
    cx.update(|_, cx| {
        assert_eq!(
            reopened.entity_id(),
            original.entity_id(),
            "Dock 重开必须复用后台宿主，不能创建第二个实例"
        );
        assert_eq!(
            crate::workspace::desktop::sessions::hosts(&database, cx).len(),
            1,
            "重开后手机必须能唯一定位宿主"
        );
        assert_eq!(
            reopened.read(cx).desktop_workspace.active_id.as_deref(),
            Some(workspace.id.as_str()),
            "当前工作空间不能因重开丢失"
        );
        let located = crate::workspace::desktop::mobile::session(&database, &area, cx)
            .expect("重开后手机应能定位原项目");
        assert_eq!(
            located.entity_id(),
            project.entity_id(),
            "重开不能重建或替换手机已关联的项目会话"
        );
        assert!(
            crate::workspace::desktop::sessions::detached(&database, cx).is_none(),
            "重开后不应残留另一份无窗口宿主"
        );
        assert!(
            crate::workspace::desktop::sessions::hosts(&dir.path().join("other.sqlite"), cx)
                .is_empty(),
            "不能跨数据库复用后台宿主"
        );
    });
}
