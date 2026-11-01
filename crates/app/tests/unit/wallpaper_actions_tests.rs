use crate::app::{AppScreen, MultisoulApp};
use crate::settings::wallpaper::state::WallpaperSelection;
use gpui::{AppContext, TestAppContext};

fn fixture() -> (tempfile::TempDir, storage::DatabasePaths, storage::AppStore) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    (temp, paths, store)
}

/// 全局壁纸：项目发起保存并立即切换空间，所有已打开视图仍收到同一个选择。
/// 数据构造：两个空间保留不同旧壁纸，一个已打开项目，选择 Aurora。
/// 执行过程：项目请求保存 → 宿主切换空间 → 等待后台完成 → 检查全局存储和两个视图。
/// 预期结果：全局值持久化为 Aurora，宿主和项目一致，旧项目字段没有新覆盖。
#[test]
fn global_wallpaper_save_survives_navigation_and_updates_open_project() {
    let (temp, paths, store) = fixture();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目");
    let catalog = store.desktop_workspace_store();
    let default = WallpaperSelection::default();
    let nebula = WallpaperSelection::Builtin {
        rel: "wallpapers/1358524.webp",
    };
    let aurora = WallpaperSelection::Builtin {
        rel: "wallpapers/1350954.webp",
    };
    let alpha = catalog
        .create_with_wallpaper("Alpha", &default.reference())
        .expect("应创建甲");
    let beta = catalog
        .create_with_wallpaper("Beta", &nebula.reference())
        .expect("应创建乙");
    let first = catalog
        .add_project(&alpha.id, &project)
        .expect("应添加甲项目");
    let second = catalog
        .add_project(&beta.id, &project)
        .expect("应添加乙项目");
    let mut test = TestAppContext::single();
    let (host, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone())
    });
    cx.run_until_parked();
    let settings = storage::SettingsStore::new(paths.app_db_path().to_owned());
    let second_host =
        cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
    cx.update(|window, cx| {
        crate::workspace::desktop::sessions::register(
            settings.path().to_owned(),
            window.window_handle(),
            host.clone(),
            cx,
        );
        crate::workspace::desktop::sessions::register(
            settings.path().to_owned(),
            window.window_handle(),
            second_host.clone(),
            cx,
        );
    });
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::new_with_database_context(cx, paths, store);
        app.desktop_host = Some(host.downgrade());
        app
    });
    host.update(cx, |app, cx| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.workspaces = catalog.list().expect("应加载目录");
        app.desktop_workspace.active_id = Some(alpha.id.clone());
        app.desktop_workspace.areas = vec![first.clone()];
        app.desktop_workspace
            .sessions
            .insert((alpha.id.clone(), first.id.clone()), session.clone());
        app.bind_desktop_wallpaper(cx);
    });
    session.update(cx, |app, cx| {
        app.request_wallpaper_selection(aurora.clone(), cx)
    });
    host.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(beta.id.clone());
        app.desktop_workspace.areas = vec![second.clone()];
        app.bind_desktop_wallpaper(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        catalog.list_projects(&alpha.id).expect("应读甲项目")[0].wallpaper,
        None,
        "全局保存不得再写项目覆盖"
    );
    assert_eq!(
        catalog.list_projects(&beta.id).expect("应读乙项目")[0].wallpaper,
        None,
        "迟到保存不得写入乙区域"
    );
    assert_eq!(
        settings.get_string("wallpaper.value").expect("应读设置"),
        Some("wallpapers/1350954.webp".into()),
        "壁纸应保存到全局设置"
    );
    session.update(cx, |app, _| {
        assert_eq!(
            app.wallpaper_settings.selection, aurora,
            "已打开项目应同步全局壁纸"
        );
    });
    second_host.update(cx, |app, _| {
        assert_eq!(
            app.wallpaper_settings.selection, aurora,
            "另一个顶层窗口也必须同步"
        );
        assert!(
            !app.wallpaper_settings.preferences.busy,
            "另一个窗口应解除保存状态"
        );
    });
    host.update(cx, |app, _| {
        assert_eq!(
            app.wallpaper_settings.selection, aurora,
            "切换后的空间也应显示全局选择"
        );
        assert_eq!(
            app.wallpaper_settings.preferences.global, aurora,
            "项目发起的选择应更新全局背景"
        );
        assert!(
            !app.wallpaper_settings.preferences.busy,
            "保存完成必须解除等待状态"
        );
    });
}

/// 场景：全局壁纸保存失败且用户已切换空间，回滚到最后全局值。
/// 数据构造：两个空间保存不同旧壁纸；settings 触发器拒绝壁纸写入。
/// 执行过程：请求 Nebula → 切到另一个空间 → 等待失败 → 检查存档和渲染。
/// 预期结果：两个空间旧数据不变，当前背景恢复全局默认，并结束保存状态。
#[test]
fn failed_global_save_restores_selection_after_navigation() {
    let (_temp, paths, store) = fixture();
    let catalog = store.desktop_workspace_store();
    let default = WallpaperSelection::default();
    let aurora = WallpaperSelection::Builtin {
        rel: "wallpapers/1350954.webp",
    };
    let alpha = catalog
        .create_with_wallpaper("Alpha", &default.reference())
        .expect("应创建甲");
    let beta = catalog
        .create_with_wallpaper("Beta", &aurora.reference())
        .expect("应创建乙");
    let connection = rusqlite::Connection::open(paths.app_db_path()).expect("应打开故障注入连接");
    connection.execute_batch("CREATE TRIGGER reject_wallpaper BEFORE INSERT ON settings WHEN NEW.key='wallpaper.value' BEGIN SELECT RAISE(FAIL,'simulated storage failure'); END;").expect("应注入写入故障");
    drop(connection);
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.workspaces = catalog.list().expect("应加载目录");
        app.desktop_workspace.active_id = Some(alpha.id.clone());
        app.bind_desktop_wallpaper(cx);
        app.request_wallpaper_selection(
            WallpaperSelection::Builtin {
                rel: "wallpapers/1358524.webp",
            },
            cx,
        );
        app.desktop_workspace.active_id = Some(beta.id.clone());
        app.bind_desktop_wallpaper(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        catalog
            .get(&alpha.id)
            .expect("应读甲")
            .expect("应存在")
            .wallpaper,
        Some(default.reference()),
        "失败不能改变甲存档"
    );
    host.update(cx, |app, _| {
        assert_eq!(
            app.wallpaper_settings.selection, default,
            "失败必须回滚到原全局背景"
        );
        assert_eq!(
            app.desktop_workspace
                .workspaces
                .iter()
                .find(|workspace| workspace.id == alpha.id)
                .expect("甲缓存应存在")
                .wallpaper,
            Some(default.reference()),
            "甲的乐观预览应恢复"
        );
        assert!(
            !app.wallpaper_settings.preferences.busy,
            "失败必须结束 saving 状态"
        );
    });
}

/// WP-10：在素材库底部请求删除，确认对话框仍位于窗口可见范围，取消不改引用。
///
/// 数据构造（含关键数值的推导过程）：
///   两个工作空间保留旧 shared.webp 引用，设置删除同一个素材。
///   设置内容滚动到 y = -1000px，模拟浏览到长素材库底部；窗口对话框独立于该滚动范围。
/// 执行过程（逐步说明系统如何处理）：
///   1. 请求删除素材，打开窗口级确认。
///   2. 检查窗口的对话框层与文件名。
///   3. 取消确认，等待前台状态同步，再读取两个工作空间。
///
/// 预期结果：
///   - 正断言：窗口有活动确认对话框，待删除文件为 shared.webp。
///   - 负断言：取消后对话框和 pending prompt 均消失，两个引用均不得改变。
#[test]
fn wallpaper_delete_confirmation_is_window_scoped_and_cancel_preserves_references() {
    use ui::component::WindowExt;

    let (_temp, paths, store) = fixture();
    let catalog = store.desktop_workspace_store();
    let custom = storage::WallpaperReference::Custom {
        filename: "shared.webp".into(),
    };
    let alpha = catalog
        .create_with_wallpaper("Alpha", &custom)
        .expect("应创建甲");
    let beta = catalog
        .create_with_wallpaper("Beta", &custom)
        .expect("应创建乙");
    let mut test = TestAppContext::single();
    test.update(ui::init_components);
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        ui::component::Root::new(app, window, cx)
    });
    let host = host.expect("应保留窗口宿主");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.settings_viewport
            .content_scroll
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(-1000.0)));
        app.request_wallpaper_delete("shared.webp".into(), window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            window.has_active_dialog(cx),
            "长列表滚动位置不得把删除确认藏到屏幕外"
        );
    });
    host.update(cx, |app, _| {
        let prompt = app
            .wallpaper_settings
            .preferences
            .delete_prompt
            .as_ref()
            .expect("应查询到引用");
        assert_eq!(
            prompt.filename, "shared.webp",
            "确认应针对所选文件，不再列出旧项目范围"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let bounds = cx
        .debug_bounds("wallpaper-delete-cancel")
        .expect("取消按钮必须在模态层可见");
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(!window.has_active_dialog(cx), "取消后不得留下模态层");
    });
    host.update(cx, |app, _| {
        assert!(
            app.wallpaper_settings.preferences.delete_prompt.is_none(),
            "取消后不得锁住素材库"
        );
    });
    assert_eq!(
        catalog
            .get(&alpha.id)
            .expect("应读取甲")
            .expect("甲应存在")
            .wallpaper,
        Some(custom.clone()),
        "取消不得清除甲壁纸"
    );
    assert_eq!(
        catalog
            .get(&beta.id)
            .expect("应读取乙")
            .expect("乙应存在")
            .wallpaper,
        Some(custom),
        "取消不得清除乙壁纸"
    );
}
