use crate::app::MultisoulApp;
use gpui::{AppContext, TestAppContext};

/// 项目内和宿主的主题设置同步到同一桌面下全部已加载项目，并保留最后一次选择。
///
/// 数据构造：1 个宿主 + 2 个项目 Entity，共享隔离数据库；选取明暗两种主题。
/// 执行过程：A 切到浅色 → 核对宿主和 B → 宿主切到暗色 → 核对 A、B 与持久化设置。
/// 预期结果：全部 Entity 颜色一致；不得保留旧主题；最终持久化为最后选择的暗色。
#[test]
fn desktop_theme_change_reaches_host_and_sibling_projects() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("隔离存储");
    let mut test = TestAppContext::single();
    let host =
        test.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
    let a =
        test.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
    let b =
        test.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
    a.update(&mut test, |app, _| {
        app.desktop_host = Some(host.downgrade())
    });
    b.update(&mut test, |app, _| {
        app.desktop_host = Some(host.downgrade())
    });
    host.update(&mut test, |app, _| {
        app.desktop_workspace
            .sessions
            .insert(("workspace-a".into(), "area-a".into()), a.clone());
        app.desktop_workspace
            .sessions
            .insert(("workspace-b".into(), "area-b".into()), b.clone());
    });
    let light = ui::ThemeId::ALL
        .into_iter()
        .find(|id| !id.is_dark())
        .expect("应有浅色主题");
    let dark = ui::ThemeId::SapphireNight;
    host.update(&mut test, |app, cx| app.set_theme(dark, cx));
    a.update(&mut test, |app, cx| app.set_theme(light, cx));
    assert_eq!(
        host.read_with(&test, |app, _| app.theme_id),
        light,
        "项目修改主题必须立即同步宿主"
    );
    assert_eq!(
        b.read_with(&test, |app, _| app.theme_id),
        light,
        "其他工作空间的已加载项目也必须同步"
    );
    assert_eq!(
        a.read_with(&test, |app, _| app.theme.text_primary),
        ui::Theme::for_id(light).text_primary,
        "当前项目使用浅色文字 token"
    );
    assert!(
        !host.read_with(&test, |app, _| app.theme_id).is_dark(),
        "宿主不能保留旧暗色主题"
    );
    host.update(&mut test, |app, cx| app.set_theme(dark, cx));
    test.background_executor.run_until_parked();
    assert_eq!(
        a.read_with(&test, |app, _| app.theme_id),
        dark,
        "宿主修改主题必须同步 A"
    );
    assert_eq!(
        b.read_with(&test, |app, _| app.theme_id),
        dark,
        "宿主修改主题必须同步 B"
    );
    assert_ne!(
        a.read_with(&test, |app, _| app.theme_id),
        light,
        "A 不能残留此前浅色主题"
    );
    let (saved, _) = super::ThemeSettingsState::load_from_store(store.settings_store());
    assert_eq!(saved, dark, "多个入口的异步保存不得把最后选择回退成浅色");
}
