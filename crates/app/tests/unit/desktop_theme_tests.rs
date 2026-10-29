use crate::app::MultisoulApp;
use gpui::{AppContext, TestAppContext};

/// 项目内和宿主的主题设置同步到同一桌面下全部已加载项目，并保留最后一次选择。
///
/// 数据构造：1 个宿主 + 2 个项目 Entity，共享隔离数据库；选取中性和蓝色两种深色主题。
/// 执行过程：A 切到墨黑 → 核对宿主和 B → 宿主切到暗色 → 核对 A、B 与持久化设置。
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
    let neutral = ui::ThemeId::InkBlack;
    let dark = ui::ThemeId::SapphireNight;
    host.update(&mut test, |app, cx| app.set_theme(dark, cx));
    a.update(&mut test, |app, cx| app.set_theme(neutral, cx));
    assert_eq!(
        host.read_with(&test, |app, _| app.theme_id),
        neutral,
        "项目修改主题必须立即同步宿主"
    );
    assert_eq!(
        b.read_with(&test, |app, _| app.theme_id),
        neutral,
        "其他工作空间的已加载项目也必须同步"
    );
    assert_eq!(
        a.read_with(&test, |app, _| app.theme.text_primary),
        ui::Theme::for_id(neutral).text_primary,
        "当前项目使用墨黑文字 token"
    );
    assert!(
        host.read_with(&test, |app, _| app.theme_id) == ui::ThemeId::InkBlack,
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
        neutral,
        "A 不能残留此前墨黑主题"
    );
    let (saved, _) = super::ThemeSettingsState::load_from_store(store.settings_store());
    assert_eq!(saved, dark, "多个入口的异步保存不得把最后选择回退成墨黑");
}

/// 场景：已有用户在取消浅色后重新启动，偏好恢复到受支持的深色目录。
/// 数据构造：隔离 SettingsStore，依次写入三种旧浅色、两个退役深色、六个当前深色及未知值。
/// 执行过程：使用生产 load_from_store 恢复主题；不模拟设置解析实现。
/// 预期结果：浅色转墨黑，已保存的有效深色保持原值，未知值回退墨黑。
#[test]
fn saved_theme_preferences_restore_only_supported_dark_profiles() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("settings.sqlite"));
    for (saved, expected) in [
        ("paper-blue", ui::ThemeId::InkBlack),
        ("linen-sage", ui::ThemeId::InkBlack),
        ("silver-violet", ui::ThemeId::InkBlack),
        ("ink-black", ui::ThemeId::InkBlack),
        ("graphite", ui::ThemeId::Graphite),
        ("warm-obsidian", ui::ThemeId::WarmObsidian),
        ("sapphire-night", ui::ThemeId::SapphireNight),
        ("midnight-violet", ui::ThemeId::Moonstone),
        ("forest-mint", ui::ThemeId::DeepPetrol),
        ("moonstone", ui::ThemeId::Moonstone),
        ("deep-petrol", ui::ThemeId::DeepPetrol),
        ("unknown", ui::ThemeId::InkBlack),
    ] {
        store
            .set_string(super::THEME_SETTING_KEY, saved)
            .expect("写入偏好");
        let (restored, _) = super::ThemeSettingsState::load_from_store(store.clone());
        assert_eq!(restored, expected, "{saved} 应恢复到正确的深色主题");
        assert!(restored.is_dark(), "恢复结果不得继续是浅色");
        assert!(
            ui::ThemeId::ALL.contains(&restored),
            "恢复值必须在可选目录内"
        );
    }
}
