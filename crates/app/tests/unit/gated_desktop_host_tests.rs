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
