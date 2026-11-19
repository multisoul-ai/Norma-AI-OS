use super::*;
use gpui::{px, size, AppContext, TestAppContext};

/// 场景：窄窗任务浮层的搜索框有焦点时进入全屏，仍应单次Escape退出。
/// 数据构造：600×800窗口、已打开任务浮层、两行未发送草稿；不启动PTY。
/// 执行过程：聚焦生产搜索框→执行全屏按钮操作→重绘→发送一次真实Escape事件。
/// 预期结果：全屏退出、浮层保持关闭；两行草稿未丢失，也没有发送新任务。
#[test]
fn staff_fullscreen_from_search_restores_keyboard_escape() {
    use gpui::Focusable;
    let dir = tempfile::tempdir().expect("isolated project");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let mut source = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        source = Some(app.clone());
        ui::component::Root::new(app, window, cx)
    });
    let source = source.expect("app");
    cx.simulate_resize(size(px(600.), px(800.)));
    source.update_in(cx, |app, window, cx| {
        app.screen = crate::app::AppScreen::DesktopWorkspace;
        app.staff.enabled = true;
        app.staff.sidebar_open = true;
        app.ensure_staff_inputs(window, cx);
        app.staff
            .input
            .clone()
            .expect("composer")
            .update(cx, |input, cx| {
                input.set_value("第一行\n第二行", window, cx);
            });
        app.staff
            .search
            .as_ref()
            .expect("search")
            .focus_handle(cx)
            .focus(window);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    source.update_in(cx, |app, window, cx| {
        app.toggle_staff_fullscreen(window, cx);
        assert!(
            app.focus_handle.is_focused(window),
            "搜索卸载前须把焦点交回页面"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        source.read_with(cx, |app, _| app.staff.fullscreen),
        "前置应进入全屏"
    );
    cx.simulate_keystrokes("escape");
    source.read_with(cx, |app, cx| {
        assert!(
            !app.staff.fullscreen,
            "搜索框卸载后Escape仍须到达幕僚根节点"
        );
        assert!(!app.staff.sidebar_open, "不能重新打开浮层");
        assert_eq!(
            app.staff
                .input
                .as_ref()
                .expect("composer")
                .read(cx)
                .value()
                .as_ref(),
            "第一行\n第二行",
            "切换全屏必须保留两行草稿"
        );
        assert!(app.staff.tasks.is_empty(), "切换与Escape不能发送草稿");
    });
}
