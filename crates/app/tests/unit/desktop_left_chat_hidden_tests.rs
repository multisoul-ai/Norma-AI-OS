use super::*;
use gpui::{size, AppContext, TestAppContext};
use runtime::desktop_drawers::DesktopDrawerEvent;

/// 左栏暂时隐藏 Chat：只呈现运行概览，不显示标签条和底部对话输入框。
///
/// 数据构造：隔离空工作空间，1280×800 真实 GPUI 视图树，默认 `left_view` 为 Chat。
/// 执行过程：打开工作空间，展开左抽屉并绘制。
/// 预期结果：运行概览与左栏标题存在；`desktop-left-view-tabs`、`desktop-assistant-chat-input` 不存在。
#[test]
fn left_drawer_hides_chat_tabs_and_input_but_shows_run_overview() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Chat Hidden")
        .expect("应创建工作空间");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleLeft, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-assistant-brand").is_some(),
        "左栏标题仍应显示"
    );
    assert!(
        cx.debug_bounds("desktop-run-overview").is_some(),
        "左栏应直接呈现运行概览"
    );
    assert!(
        cx.debug_bounds("desktop-left-view-tabs").is_none(),
        "Chat / Run Overview 标签条应隐藏"
    );
    assert!(
        cx.debug_bounds("desktop-assistant-chat-input").is_none(),
        "底部对话输入框应隐藏"
    );
}
