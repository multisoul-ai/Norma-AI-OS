use super::*;
use gpui::TestAppContext;

/// 首次附着默认启动手机连接，停用后打开另一窗口不能再次启动。
///
/// 数据构造（含关键数值的推导过程）：
///   一个隔离 SQLite 路径、两个窗口引用；两次 attach 对应同一个进程服务。
///   测试在后台任务执行前取消，避免真实网络监听或外部 relay 连接。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 首次 attach 注册输入来源并安排后台启动。
///   2. 用户停用服务，再次 attach 模拟新窗口复用进程服务。
///   3. 主动退出清理全局注册表。
///
/// 预期结果：启动任务存在且未伪报已开启；复用停用状态，不重建启动任务。
#[test]
fn mobile_connection_starts_by_default_and_respects_current_process_disable() {
    let dir = tempfile::tempdir().expect("隔离 App 数据目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let mut cx = TestAppContext::single();
    let first = cx.update(|cx| attach(store.clone(), cx));
    first.read_with(&cx, |host, _| {
        assert!(host.busy, "首次启动必须安排手机连接后台初始化");
        assert!(host.operation.is_some(), "初始化必须由可取消的后台任务承载");
        assert!(!host.enabled(), "任务尚未完成时不能伪报连接已开启");
        assert!(
            host.command_source().is_some(),
            "自动启动前必须接好手机命令来源"
        );
        assert!(
            host.snapshot_source().is_some(),
            "自动启动前必须接好状态快照来源"
        );
    });
    first.update(&mut cx, |host, cx| host.disable(cx));
    let second = cx.update(|cx| attach(store, cx));
    assert_eq!(
        first.entity_id(),
        second.entity_id(),
        "新窗口必须复用同一个服务"
    );
    second.read_with(&cx, |host, _| {
        assert!(!host.busy, "主动停用后新窗口不能重新安排启动");
        assert!(host.operation.is_none(), "停用后不得保留初始化任务");
        assert!(!host.enabled(), "主动停用后连接必须保持关闭");
        assert!(host.cancelled.load(Ordering::Acquire), "迟到的启动必须取消");
    });
    cx.update(shutdown);
}
