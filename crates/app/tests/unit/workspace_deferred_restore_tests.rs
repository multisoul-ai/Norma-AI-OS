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
