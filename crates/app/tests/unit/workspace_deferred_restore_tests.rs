use super::*;
use canvas::{CanvasRect, TermSpec};

/// 恢复过程中关窗暂停，重开继续恢复原卡且不重复启动已经运行的终端。
///
/// 数据构造：2张Term卡，1张已经运行；默认80×24网格，echo工厂不启动外部程序。
/// 执行过程：排入恢复 → 立即detach → 推进2×16ms让旧恢复退出 → attach → 推进3×16ms。
/// 预期结果：后台仅保留原有1个PTY；重开恢复第二张；第一张终端ID不变；完成后清理恢复计划。
#[test]
fn interrupted_restore_resumes_when_window_reattaches() {
    let mut owner = gpui::TestAppContext::single();
    let root = tempfile::tempdir().expect("隔离项目");
    let (app, cx) = owner.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let (first, second, original_id) = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.workspace_state.current_root = Some(root.path().to_owned());
            app.project_window = Some(window.window_handle());
            let rect = CanvasRect::new(0., 0., 640., 400.);
            let first = app
                .runtime
                .spawn_terminal(rect, "/bin/sh", 80, 24)
                .expect("原PTY");
            let second = app
                .runtime
                .document
                .add_node(CanvasNodeKind::Term(TermSpec), rect);
            let original_id = app
                .runtime
                .node_run_state(first)
                .expect("原运行状态")
                .clone();
            let pending = PendingLiveSessionRestore::new(
                root.path().to_owned(),
                &app.runtime,
                vec![],
                vec![],
            );
            app.spawn_deferred_live_session_restore(pending, cx);
            app.detach_project_window(cx);
            (first, second, original_id)
        })
    });
    cx.run_until_parked();
    for _ in 0..2 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    assert_eq!(
        app.read_with(cx, |app, _| app.runtime.live_len()),
        1,
        "后台不能继续批量启动"
    );
    cx.update(|window, cx| app.update(cx, |app, cx| app.attach_project_window(window, cx)));
    cx.run_until_parked();
    for _ in 0..3 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    app.read_with(cx, |app, _| {
        assert!(
            app.runtime.node_view(second).is_some(),
            "重开不能漏掉尚未恢复的卡片"
        );
        assert_eq!(
            app.runtime.node_run_state(first),
            Some(original_id),
            "原PTY身份不能改变"
        );
        assert!(app.background_restore.is_none(), "完成后不能保留旧恢复计划");
    });
}

/// 桌面项目总览应恢复真实会话，不能因为尚未展开项目而永远显示 session gone。
///
/// 数据构造：1 个桌面 host、1 个未展开项目、2 张已保存 Term 节点；原 live 数为 0，停止前另加 1 张未启动节点。
///   每批恢复间隔为 16ms；推进 3×16ms 足以处理 2 张卡片并结束恢复计划；echo 工厂不启动外部进程。
/// 执行过程：项目保持 background=true 且无绑定窗口 → 激活总览 → 推进恢复时钟 → 绑定展开窗口 → 排入恢复后立即停止项目。
/// 预期结果：展开前两个节点都有运行会话，恢复计划已清空；展开不重启原会话，也不在总览抢占窗口；停止后迟到任务不能重启会话。
#[test]
fn desktop_overview_restores_live_sessions_before_project_expansion() {
    let mut test = gpui::TestAppContext::single();
    let root = tempfile::tempdir().expect("应创建隔离项目");
    let (host, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        );
        app.desktop_host = Some(host.downgrade());
        app.background_project = true;
        app.workspace_state.current_root = Some(root.path().to_owned());
        app
    });
    let (first, second) = session.update(cx, |app, cx| {
        let first = app.runtime.document.add_node(
            CanvasNodeKind::Term(TermSpec),
            CanvasRect::new(0., 0., 640., 400.),
        );
        let second = app.runtime.document.add_node(
            CanvasNodeKind::Term(TermSpec),
            CanvasRect::new(660., 0., 640., 400.),
        );
        app.background_restore = Some(PendingLiveSessionRestore::new(
            root.path().to_owned(),
            &app.runtime,
            vec![],
            vec![],
        ));
        app.activate_desktop_area_overview(cx);
        (first, second)
    });
    cx.run_until_parked();
    for _ in 0..3 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    let first_state = session.read_with(cx, |app, _| {
        assert!(
            app.runtime.node_view(first).is_some(),
            "第一张真实终端在总览就应恢复，不能显示 session gone"
        );
        assert!(
            app.runtime.node_view(second).is_some(),
            "第二张真实终端也应恢复，不能等用户展开"
        );
        assert_eq!(app.runtime.live_len(), 2, "总览应启动两张存储卡片的会话");
        assert!(
            app.project_window.is_none(),
            "恢复会话不应把未展开项目绑定到前台窗口"
        );
        assert!(
            app.background_restore.is_none(),
            "恢复完成必须清理计划，不能在展开时重复执行"
        );
        app.runtime
            .node_run_state(first)
            .expect("第一张应有运行身份")
            .clone()
    });
    session.update_in(cx, |app, window, cx| {
        app.attach_desktop_area_window(window, cx)
    });
    cx.run_until_parked();
    session.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.node_run_state(first),
            Some(first_state),
            "展开应复用总览启动的会话身份"
        );
        assert_eq!(app.runtime.live_len(), 2, "展开不能重复启动额外会话");
    });
    let third = session.update(cx, |app, cx| {
        let third = app.runtime.document.add_node(
            CanvasNodeKind::Term(TermSpec),
            CanvasRect::new(1320., 0., 640., 400.),
        );
        let pending =
            PendingLiveSessionRestore::new(root.path().to_owned(), &app.runtime, vec![], vec![]);
        app.spawn_deferred_live_session_restore(pending, cx);
        app.stop_project(cx);
        third
    });
    cx.run_until_parked();
    // 每个节点间隔 16ms；推进 3×16ms 覆盖已有两张及第三张未启动卡片，不能让迟到任务复活项目。
    for _ in 0..3 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    session.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.running_terminal_count(),
            0,
            "停止必须结束所有运行终端，迟到恢复不能重新启动"
        );
        assert!(
            app.runtime.node_view(third).is_none(),
            "停止后迟到任务不能启动第三张尚未恢复的卡片"
        );
        assert!(
            app.runtime.node_run_state(third).is_none(),
            "未恢复的第三张卡片不能获得运行身份"
        );
    });
}
