use crate::app::background_project::PreparedProject;
use crate::app::{AppScreen, MultisoulApp};
use gpui::{AppContext, KeyDownEvent, Keystroke, TestAppContext};

/// 场景：焦点在嵌入项目时按 Ctrl+1～5，没有待办也必须安全显示宿主提示。
/// 数据构造：隔离空间含一个已加载、无卡片无想法的子项目，宿主持有该项目实体。
/// 执行过程：子项目在 update 中通过真实 on_key_down 处理各数字键，等待延后操作完成。
/// 预期结果：无重入读取崩溃；宿主显示中英文“没有待办”，子项目不保存宿主提示、不创建卡片。
#[test]
fn idea_assignment_shortcuts_from_project_focus_show_host_notice() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Shortcut focus").expect("应创建空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let prepared =
        PreparedProject::load_for_desktop(paths.clone(), area.clone()).expect("应准备嵌入项目");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::from_background_project(prepared, cx);
        app.desktop_host = Some(host.downgrade());
        app
    });
    host.update(cx, |app, _| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.expanded_area_id = Some(area.id.clone());
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area.id.clone()), session.clone());
    });
    cx.run_until_parked();

    for digit in 1..=5 {
        host.update(cx, |app, _| app.desktop_workspace.ideas_notice = None);
        session.update_in(cx, |app, window, cx| {
            window.focus(&app.focus_handle);
            app.on_key_down(
                &KeyDownEvent {
                    keystroke: Keystroke::parse(&format!("ctrl-{digit}"))
                        .expect("应解析 Ctrl+数字"),
                    is_held: false,
                },
                window,
                cx,
            );
        });
        cx.run_until_parked();
        host.read_with(cx, |app, _| {
            let notice = app
                .desktop_workspace
                .ideas_notice
                .as_ref()
                .expect("项目快捷键应在宿主显示提示");
            assert_eq!(
                notice.resolve_for(ui::i18n::Locale::ZhCn),
                "没有待办想法可派发",
                "Ctrl+{digit} 应显示中文空待办提示"
            );
            assert_eq!(
                notice.resolve_for(ui::i18n::Locale::En),
                "No ideas to assign",
                "Ctrl+{digit} 提示应随语言切换"
            );
        });
        session.read_with(cx, |app, _| {
            assert!(
                app.desktop_workspace.ideas_notice.is_none(),
                "提示应由宿主持有"
            );
            assert!(
                app.runtime.document.nodes().is_empty(),
                "无待办不能创建卡片"
            );
        });
    }
}
