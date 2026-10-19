use super::*;
use gpui::{size, AppContext, TestAppContext};

/// 打开网页后，右栏应使用原型的阅读器栏，内容贴齐栏边，而不是内缩卡片。
///
/// 数据构造：1280×800 空工作空间，右栏展开；随后放入一个已选中的
/// https://www.apple.com/apple-intelligence/ 标签。标签栏高 52px，上下文条高 40px。
/// 执行过程：先绘制空右栏，再写入标签并重绘。
/// 预期结果：空栏没有上下文条和地址栏；有网页时上下文条紧贴标签栏，标题是静态文字，
/// 没有地址输入、文件按钮或前进后退，内容左缘只留 1px 左边框。
#[test]
fn open_browser_tab_uses_prototype_reader_chrome() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Reader Chrome")
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
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleRight, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-content-context").is_none(),
        "没有标签时不应出现页面上下文条"
    );
    assert!(
        cx.debug_bounds("desktop-content-address-bar").is_none(),
        "没有标签时不应出现地址输入"
    );
    host.update_in(cx, |app, _, cx| {
        app.browsers.disable_native_views();
        app.desktop_workspace.content_tabs = vec![storage::DesktopContentTab {
            id: "apple".into(),
            workspace_id: workspace.id.clone(),
            source_area_id: None,
            source_project_name: "Reader Chrome".into(),
            kind: storage::DesktopContentKind::Browser,
            resource: "https://www.apple.com/apple-intelligence/".into(),
            version: None,
            title: "Apple Intelligence".into(),
            position: 0,
            selected: true,
            source_removed: false,
        }];
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let drawer = cx
        .debug_bounds("desktop-assistant-right")
        .expect("应找到右栏");
    let context = cx
        .debug_bounds("desktop-content-context")
        .expect("网页标签应显示上下文条");
    let title = cx
        .debug_bounds("desktop-content-context-title")
        .expect("网页标签应显示静态页面标题");
    let body = cx
        .debug_bounds("desktop-content-body")
        .expect("应找到贴边内容区");
    assert_eq!(
        context.top(),
        drawer.top() + px(52.0),
        "上下文条应紧贴 52px 标签栏"
    );
    assert!(
        title.top() >= context.top() && title.bottom() <= context.bottom(),
        "页面标题应留在上下文条内部"
    );
    assert!(
        cx.debug_bounds("desktop-content-address-bar").is_none(),
        "打开的网页不应再显示地址输入和光标"
    );
    assert!(
        cx.debug_bounds("desktop-content-files").is_none(),
        "标签栏不应再放文件按钮"
    );
    assert!(
        cx.debug_bounds("desktop-browser-back").is_none(),
        "上下文条不应再放后退按钮"
    );
    assert!(
        cx.debug_bounds("desktop-content-open-original").is_some(),
        "空地址时应提供打开原网页"
    );
    assert!(
        cx.debug_bounds("desktop-content-kind-badge").is_some(),
        "上下文条应显示内容类型标记"
    );
    assert_eq!(
        body.left() - drawer.left(),
        px(1.0),
        "内容应贴齐右栏内容区，只留 1px 左边框，不能再内缩 12px"
    );
    assert!(
        (body.top() - context.bottom()).abs() <= px(1.0),
        "内容应紧接上下文条，不能再留内缩圆角卡片"
    );
}
