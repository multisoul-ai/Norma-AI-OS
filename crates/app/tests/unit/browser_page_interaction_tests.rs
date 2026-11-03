use super::*;
use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::TestAppContext;
use runtime::InputMode;

/// 场景：原生网页交互经生产队列选中对应卡片；删除后的迟到事件不改变用户的新选择。
/// 数据构造：隔离App、两张纯文档Browser；第一张处于聚焦及键盘选择状态，无原生WebView。
/// 执行过程：向真实BrowserStore队列发送第二张的PageInteraction并drain，再删除第二张并重放旧事件。
/// 预期结果：有效事件独占选择、退出PTY焦点并清键盘标记；失效目标不能抢走第一张的选择或焦点。
#[test]
fn test_browser_page_interaction_event_selects_live_target_and_rejects_deleted_target() {
    let directory = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(directory.path());
    let store = storage::AppStore::initialize(&paths).expect("隔离存储");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update(cx, |app, cx| {
        app.browsers.disable_native_views();
        let bounds = CanvasRect::new(0., 0., 640., 360.);
        let first = app
            .runtime
            .spawn_browser("https://first.example".into(), None, bounds);
        let second = app
            .runtime
            .spawn_browser("https://second.example".into(), None, bounds);
        app.runtime.selection.select_only(first);
        app.runtime.focus.focus_node(first);
        app.interaction.keyboard_selected_node = Some(first);
        app.browsers
            .sender
            .send(BrowserEvent::PageInteraction { node_id: second })
            .expect("原生事件必须入队");
        assert!(app.drain_browser_events(cx), "有效网页交互必须报告状态变化");
        assert!(app.runtime.selection.contains(second), "交互目标必须被选中");
        assert!(!app.runtime.selection.contains(first), "原选择必须被替换");
        assert_eq!(
            app.runtime.focus.mode(),
            InputMode::CanvasSelect,
            "网页交互必须退出PTY焦点"
        );
        assert_eq!(
            app.interaction.keyboard_selected_node, None,
            "网页交互不能保留旧键盘选择标记"
        );
        app.runtime.remove_node(second);
        app.runtime.selection.select_only(first);
        app.runtime.focus.focus_node(first);
        app.browsers
            .sender
            .send(BrowserEvent::PageInteraction { node_id: second })
            .expect("迟到事件必须经过同一队列");
        assert!(
            !app.drain_browser_events(cx),
            "已删除目标的迟到交互不能报告有效变化"
        );
        assert!(
            app.runtime.selection.contains(first),
            "迟到事件不能抢走新选择"
        );
        assert!(
            !app.runtime.selection.contains(second),
            "删除卡片不能被事件重新选中"
        );
        assert_eq!(
            app.runtime.focus.mode(),
            InputMode::NodeFocused(first),
            "失效事件不能改变当前焦点"
        );
    });
}
