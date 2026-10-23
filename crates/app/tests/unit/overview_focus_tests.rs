use super::*;
use gpui::TestAppContext;

fn host(cx: &mut TestAppContext) -> (gpui::Entity<MultisoulApp>, &mut gpui::VisualTestContext) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    // 目录在整个测试期间必须存在，直接泄漏给测试进程回收。
    std::mem::forget(temp);
    cx.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store))
}

/// 场景：概览动作之后键盘焦点必须回到“当前正在显示的那份概览”，悬浮时是悬浮面板，否则是左栏。
///
/// 数据构造：隔离存储的桌面外壳，没有项目；分别在“未悬浮”和“已悬浮”两种状态下取焦点句柄。
/// 执行过程：调用 `visible_overview_focus`，与左栏句柄、悬浮面板句柄比较。
/// 预期结果：
///   - 正断言：未悬浮时是左栏概览句柄；悬浮时是悬浮面板句柄。
///   - 负断言：悬浮时不能是没有渲染的左栏句柄（否则方向键失效）；两个句柄本身互不相同。
#[test]
fn overview_actions_refocus_the_overview_that_is_visible() {
    let mut test = TestAppContext::single();
    let (app, cx) = host(&mut test);
    app.update(cx, |app, cx| {
        let left = app.left_overview_focus(cx);
        let floating = app.overview_float_focus(cx);
        assert_ne!(left, floating, "两份概览各有自己的焦点句柄");
        assert_eq!(
            app.visible_overview_focus(cx),
            left,
            "未悬浮时焦点在左栏概览"
        );
        app.desktop_workspace.drawers.overview_floating = true;
        assert_eq!(
            app.visible_overview_focus(cx),
            floating,
            "悬浮时焦点在悬浮面板"
        );
        assert_ne!(
            app.visible_overview_focus(cx),
            left,
            "悬浮时不能交给没渲染的左栏"
        );
    });
}
