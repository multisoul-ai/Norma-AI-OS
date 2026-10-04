use super::*;
use gpui::{size, AppContext, Modifiers, TestAppContext};

/// 场景：展开项目中的设置属于窗口，缩小窗口后主题卡片重排且关闭按钮仍可用。
///
/// 数据构造：一个空间、一个空项目；窗口1280×800，设置外边距32px。
///   中窗口700×700、窄窗口420×520；主题卡片至少190px宽，避免文字和单选按钮拥挤。
/// 执行过程：加载并展开项目 → 打开默认记忆页 → 点击主题 → 检查窗口遮罩 → 缩窗 → 关闭。
/// 预期结果：遮罩覆盖整个窗口、卡片按可用宽度换行；设置不会缩到项目画布或阻挡关闭。
#[test]
fn desktop_settings_overlay_fits_window_and_reflows_themes() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Settings").expect("应创建空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
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
        app.open_desktop_workspace(workspace.id.clone(), window, cx)
    });
    cx.run_until_parked();
    let project = host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        app.expand_desktop_project_area(area.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace.sessions[&(workspace.id.clone(), area.id.clone())].clone()
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let settings = cx.debug_bounds("toolbar-settings").expect("应有设置按钮");
    cx.simulate_click(settings.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let overlay = cx.debug_bounds("settings-overlay").expect("应打开设置");
    assert_eq!(
        overlay.origin,
        gpui::point(px(0.0), px(0.0)),
        "设置遮罩应从窗口原点开始，不受项目画布偏移影响"
    );
    assert_eq!(
        overlay.size,
        size(px(1280.0), px(800.0)),
        "遮罩必须覆盖Dock和整个窗口，不能局限于项目画布"
    );
    assert!(
        cx.debug_bounds("norma-memory").is_some(),
        "展开项目中的设置也必须默认展示Norma记忆"
    );
    assert!(
        cx.debug_bounds("settings-theme-paper-blue").is_none(),
        "主题不能再作为默认目的地"
    );
    let themes = cx
        .debug_bounds("settings-section-themes")
        .expect("主题应保留独立菜单");
    cx.simulate_click(themes.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    let wide = cx
        .debug_bounds("settings-theme-paper-blue")
        .expect("应展示浅色主题");
    let wide_last = cx
        .debug_bounds("settings-theme-silver-violet")
        .expect("应展示第三个主题");
    assert_eq!(wide.top(), wide_last.top(), "宽窗口的三个主题应同排");
    cx.simulate_resize(size(px(700.0), px(700.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let medium = cx
        .debug_bounds("settings-theme-paper-blue")
        .expect("中窗口应展示主题");
    let medium_last = cx
        .debug_bounds("settings-theme-silver-violet")
        .expect("第三张主题应换行");
    assert!(
        medium.size.width >= px(190.0),
        "主题卡片不能继续三列挤压文字，实际宽度={:?}",
        medium.size.width
    );
    assert!(
        medium_last.top() > medium.top(),
        "中窗口第三张卡应移到下一行"
    );
    cx.simulate_resize(size(px(420.0), px(520.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let first = cx
        .debug_bounds("settings-theme-paper-blue")
        .expect("窄窗口应展示主题");
    let second = cx
        .debug_bounds("settings-theme-linen-sage")
        .expect("窄窗口应保留第二主题");
    assert!(second.top() > first.top(), "窄窗口应改为单列而非横向裁剪");
    let close = cx.debug_bounds("settings-close").expect("关闭按钮应保留");
    assert!(
        close.right() <= px(420.0) && close.bottom() <= px(520.0),
        "关闭按钮必须在窗口内"
    );
    cx.simulate_click(close.center(), Modifiers::none());
    cx.run_until_parked();
    project.update(cx, |app, _| {
        assert!(!app.settings_panel.open, "关闭按钮必须操作项目的设置状态")
    });
    project.update_in(cx, |app, window, cx| {
        app.toggle_settings_panel(cx);
        app.focus_handle.focus(window);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("escape");
    project.update(cx, |app, _| {
        assert!(!app.settings_panel.open, "窗口级设置应支持Esc关闭")
    });
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(area.id.as_str()),
            "关闭设置不应退出项目"
        )
    });
}
