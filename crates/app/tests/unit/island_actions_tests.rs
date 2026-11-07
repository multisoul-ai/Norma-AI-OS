use super::*;
use crate::app::MultisoulApp;
use gpui::{AppContext, TestAppContext};

/// 场景：定位后台空间时工作窗口已关闭，必须重接原宿主而非新建Runtime。
/// 数据：一个宿主和一个后台项目Entity，先打开窗口、关窗并保留后台，再重开。
/// 执行：连续打开两次核对去重；移除旧窗口并执行关闭生命周期，再重开。
/// 预期：窗口号变化但原宿主/项目身份不变；已有窗口不会重复创建。
#[test]
fn island_reopen_reuses_retained_host_and_runtime() {
    let mut cx = TestAppContext::single();
    let host = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let project = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update(&mut cx, |app, _| {
        app.desktop_workspace
            .sessions
            .insert(("B".into(), "area".into()), project.clone());
    });
    let first = cx
        .update(|cx| crate::workspace::windowing::reopen_desktop_host(&host, cx))
        .expect("首次开窗");
    let same = cx
        .update(|cx| crate::workspace::windowing::reopen_desktop_host(&host, cx))
        .expect("重复定位");
    assert!(first == same, "已有宿主窗口必须复用");
    cx.update(|cx| {
        first
            .update(cx, |_, window, _| window.remove_window())
            .expect("关闭窗口");
    });
    cx.update(|cx| crate::workspace::desktop::sessions::windows_closed(true, cx));
    let reopened = cx
        .update(|cx| crate::workspace::windowing::reopen_desktop_host(&host, cx))
        .expect("重接后台宿主");
    assert!(first != reopened, "旧窗口确实关闭后必须重开");
    host.read_with(&cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.sessions[&("B".into(), "area".into())].entity_id(),
            project.entity_id(),
            "重开不能创建新的项目Runtime"
        )
    });
}

/// 场景：两个宿主都有相同 area ID，后台 B 必须精确路由，不能按当前空间猜测。
/// 数据：宿主1的当前空间A、后台B与宿主2的B各有独立 Entity；目标携带来源和空间。
/// 执行：解析后台目标，再改变可见空间、删除原后台项目、释放来源。
/// 预期：返回始终是原 Entity；不会改 active_id，也不会回退到同名项目或另一宿主。
#[test]
fn island_action_routes_exact_background_owner_and_rejects_stale_target() {
    let mut cx = TestAppContext::single();
    let temp = tempfile::tempdir().expect("创建隔离数据目录");
    let mut make = |name| {
        cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                storage::DatabasePaths::isolated_test(&temp.path().join(name)),
            )
        })
    };
    let first = make("first");
    let second = make("second");
    let a = make("a");
    let b = make("b");
    let other = make("other");
    first.update(&mut cx, |app, _| {
        app.desktop_workspace.active_id = Some("A".into());
        app.desktop_workspace
            .sessions
            .insert(("A".into(), "same".into()), a.clone());
        app.desktop_workspace
            .sessions
            .insert(("B".into(), "same".into()), b.clone());
    });
    second.update(&mut cx, |app, _| {
        app.desktop_workspace
            .sessions
            .insert(("B".into(), "same".into()), other.clone());
    });
    cx.update(|cx| {
        register_source(&first, cx);
        register_source(&second, cx);
    });
    let target = IslandTarget {
        source: first.entity_id().as_u64(),
        workspace: "B".into(),
        area: "same".into(),
    };
    cx.update(|cx| {
        assert_eq!(
            target.session(cx).map(|s| s.entity_id()),
            Some(b.entity_id()),
            "必须选择后台 B 的原宿主"
        );
        assert_eq!(
            first.read(cx).desktop_workspace.active_id.as_deref(),
            Some("A"),
            "审批路由不能切换可见空间"
        );
    });
    first.update(&mut cx, |app, _| {
        app.desktop_workspace.active_id = Some("B".into());
        app.desktop_workspace
            .sessions
            .remove(&("B".into(), "same".into()));
    });
    cx.update(|cx| {
        assert!(
            target.session(cx).is_none(),
            "目标删除后不能转到另一个宿主的同名项目"
        )
    });
    cx.update(|cx| unregister_source(target.source, cx));
    cx.update(|cx| assert!(target.host(cx).is_none(), "释放来源后迟到操作必须失效"));
}
