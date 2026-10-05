use crate::app::{AppScreen, MultisoulApp};
use crate::settings::wallpaper::{scoped::WallpaperContext, state::WallpaperSelection};
use gpui::{AppContext, TestAppContext};

fn fixture() -> (tempfile::TempDir, storage::DatabasePaths, storage::AppStore) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    (temp, paths, store)
}

/// WP-06：项目发起保存后立即切换工作空间，迟到请求仍只写原区域。
///
/// 数据构造：同项目目录 1，工作空间 2，区域 2；甲和乙初始壁纸分别为默认和 nebula。
///   项目请求覆盖 Aurora 后，在同一前台轮次立即切到乙，后台请求尚未处理。
/// 执行过程：项目捕获甲范围 → 宿主切乙 → 执行异步写入 → 检查存储和宿主实际背景。
/// 预期结果：甲项目保存 Aurora；乙项目无覆盖，乙背景保持 nebula；全局选择不变。
#[test]
fn deferred_project_save_stays_with_original_workspace_after_navigation() {
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
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::new_with_database_context(cx, paths, store);
        app.desktop_host = Some(host.downgrade());
        app.wallpaper_settings.scoped.bind(Some(WallpaperContext {
            workspace: alpha.clone(),
            area: Some(first.clone()),
        }));
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
        Some(aurora.reference()),
        "迟到保存必须写入原甲区域"
    );
    assert_eq!(
        catalog.list_projects(&beta.id).expect("应读乙项目")[0].wallpaper,
        None,
        "迟到保存不得写入乙区域"
    );
    host.update(cx, |app, _| {
        assert_eq!(
            app.wallpaper_settings.selection, nebula,
            "乙应保持自己的壁纸"
        );
        assert_eq!(
            app.wallpaper_settings.scoped.global, default,
            "项目保存不得污染全局背景"
        );
        assert!(
            !app.wallpaper_settings.scoped.busy,
            "保存完成必须解除等待状态"
        );
    });
}

/// WP-07：数据库拒绝保存后恢复原范围，不能回滚用户后来切到的空间。
///
/// 数据构造：工作空间 2，触发器 1，拒绝所有壁纸更新；甲默认，乙 Aurora。
/// 执行过程：甲请求 Nebula → 切乙 → 后台触发器拒绝写入 → 检查甲存档和乙背景。
/// 预期结果：甲恢复默认，乙仍 Aurora；没有遗留 saving 状态和未保存的 Nebula。
#[test]
fn failed_save_restores_original_scope_without_touching_new_workspace() {
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
    connection.execute_batch("CREATE TRIGGER reject_wallpaper BEFORE UPDATE OF wallpaper_json ON desktop_workspaces BEGIN SELECT RAISE(FAIL,'simulated storage failure'); END;").expect("应注入写入故障");
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
            app.wallpaper_settings.selection, aurora,
            "失败不能回滚乙背景"
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
            !app.wallpaper_settings.scoped.busy,
            "失败必须结束 saving 状态"
        );
    });
}

/// WP-10：在素材库底部请求删除，确认对话框仍位于窗口可见范围，取消不改引用。
///
/// 数据构造（含关键数值的推导过程）：
///   两个工作空间 Alpha/Beta 同时引用 shared.webp；项目覆盖为 0。
///   设置内容滚动到 y = -1000px，模拟浏览到长素材库底部；窗口对话框独立于该滚动范围。
/// 执行过程（逐步说明系统如何处理）：
///   1. 请求删除素材，后台查询两个工作空间的引用。
///   2. 查询完成后检查窗口的对话框层与受影响范围。
///   3. 取消确认，等待前台状态同步，再读取两个工作空间。
/// 预期结果：
///   - 正断言：窗口有活动确认对话框，Alpha/Beta 均列为受影响范围。
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
            .scoped
            .delete_prompt
            .as_ref()
            .expect("应查询到引用");
        assert!(
            prompt.usage.workspaces.contains(&"Alpha".into()),
            "确认必须列出甲引用"
        );
        assert!(
            prompt.usage.workspaces.contains(&"Beta".into()),
            "确认必须列出乙引用"
        );
        assert!(prompt.usage.projects.is_empty(), "无项目时不能虚构项目引用");
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
            app.wallpaper_settings.scoped.delete_prompt.is_none(),
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
