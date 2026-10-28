use super::*;
use ui::i18n::{LanguagePreference, Locale};

/// 语言设置缺省跟随系统，后台串行写入保留连续切换的最后一次选择。
///
/// 数据构造（含关键数值的推导过程）：
///   独立数据库 1 个；系统语言列表 [zh-CN]。
///   连续操作 = English → System → SimplifiedChinese，共 3 次写入。
///
/// 执行过程：
///   1. 从空数据库加载 → System / ZhCn。
///   2. 连续提交三次语言选择，等待后台写入屏障。
///   3. 从同一 SettingsStore 重载 → 最后的显式中文。
///
/// 预期结果：
///   - 正断言：空数据库为 System；最后的显式中文被保存并恢复。
///   - 负断言：中间的英文不能覆盖最终中文。
#[tokio::test]
async fn language_settings_restore_latest_background_write() {
    let temp = tempfile::TempDir::new().expect("应创建隔离的语言设置目录");
    let store = storage::SettingsStore::new(temp.path().join("settings.sqlite"));
    let loaded = PreparedLanguageSettings::load_from_store(store.clone(), &["zh-CN".into()]);
    assert_eq!(
        loaded.preference,
        LanguagePreference::System,
        "没有记录应跟随系统"
    );
    assert_eq!(loaded.locale, Locale::ZhCn, "中文系统应解析为中文界面");
    let writer = LanguagePersistence::start(store.clone());
    writer
        .select(LanguagePreference::English)
        .expect("后台队列应接受英文");
    writer
        .select(LanguagePreference::System)
        .expect("后台队列应接受跟随系统");
    writer
        .select(LanguagePreference::SimplifiedChinese)
        .expect("后台队列应接受中文");
    writer
        .flush()
        .await
        .expect("写入屏障应等待前面的语言设置全部保存");
    let restored = PreparedLanguageSettings::load_from_store(store, &["en".into()]);
    assert_eq!(
        restored.preference,
        LanguagePreference::SimplifiedChinese,
        "最终选择必须被持久化"
    );
    assert_eq!(
        restored.locale,
        Locale::ZhCn,
        "显式中文不能被重启后的英文系统覆盖"
    );
    assert_ne!(
        restored.preference,
        LanguagePreference::English,
        "中间英文不能覆盖最后的中文选择"
    );
    writer.shutdown().await;
}

/// 无效语言偏好按系统恢复；无法写入时后台返回错误，退出屏障等待结束。
///
/// 数据构造：有效临时 SQLite 中语言值为 "fr"；另一个 store 指向目录，必然不能作为 SQLite 文件打开。
/// 执行过程：恢复无效记录 → 跟随英文系统；向目录 store 提交中文 → 收到失败 → 退出屏障完成。
/// 预期结果：无效值为 System；失败不是成功，失败选择是中文，错误不丢失。
#[tokio::test]
async fn invalid_preference_falls_back_and_writer_reports_failure() {
    let temp = tempfile::TempDir::new().expect("应创建测试目录");
    let store = storage::SettingsStore::new(temp.path().join("valid.sqlite"));
    store
        .set_string(LANGUAGE_KEY, "fr")
        .expect("应能写入无效偏好以测试容错");
    let loaded = PreparedLanguageSettings::load_from_store(store, &["en-US".into()]);
    assert_eq!(
        loaded.preference,
        LanguagePreference::System,
        "无效值应跟随系统"
    );
    assert_eq!(loaded.locale, Locale::En, "跟随英文系统应为英文");
    let writer = LanguagePersistence::start(storage::SettingsStore::new(temp.path().to_path_buf()));
    writer
        .select(LanguagePreference::SimplifiedChinese)
        .expect("队列仍应接收中文选择");
    let result = writer.results.recv().await.expect("写入失败必须回传结果");
    assert_eq!(
        result.preference,
        LanguagePreference::SimplifiedChinese,
        "错误必须关联所选中文"
    );
    assert!(
        result.result.is_err(),
        "目录不能作为 SQLite 数据库，必须回报失败"
    );
    assert!(result.result.is_err(), "不能把未保存的偏好报告为成功");
    writer.shutdown().await;
}

/// 无存储的界面仍切换语言，并用所选语言提示无法持久化。
///
/// 数据构造：AppLanguage 使用 System / En，store=None；两次选择分别为中文与英文。
/// 执行过程：安装全局 → 选择中文 → 读取错误 → 选择英文 → 再读取同一类错误。
/// 预期结果：语言立即生效；中文错误包含“设置存储不可用”，英文错误不残留中文。
#[test]
fn language_selection_without_storage_reports_localized_failure() {
    let test_cx = gpui::TestAppContext::single();
    test_cx.update(|cx| {
        install(
            PreparedLanguageSettings {
                preference: LanguagePreference::System,
                locale: Locale::En,
                store: None,
            },
            cx,
        );
        select_language(LanguagePreference::SimplifiedChinese, cx);
        assert_eq!(Locale::current(), Locale::ZhCn, "存储失败不能阻止界面切换");
        let error = cx
            .global::<AppLanguage>()
            .save_error
            .as_ref()
            .expect("无存储必须有提示");
        assert!(error.contains("设置存储不可用"), "失败应按所选中文显示");
        select_language(LanguagePreference::English, cx);
        let error = cx
            .global::<AppLanguage>()
            .save_error
            .as_ref()
            .expect("无存储必须继续提示");
        assert!(
            error.contains("settings storage is unavailable"),
            "再次切换后提示应为英文"
        );
        assert!(!error.contains("设置存储不可用"), "已有错误不得残留中文");
    });
}

/// 重复选择同一语言也具有唯一序号，迟到的错误不能匹配最新选择。
///
/// 数据构造：选择 Chinese → English → Chinese，共 3 次；首尾偏好相同但序号分别 1 与 3。
/// 执行过程：启动隔离 writer → 连续选择 → 读取首个完成结果 → 等待退出屏障 → 恢复设置。
/// 预期结果：首个结果序号不是最新序号；最终偏好仍为中文，英文不覆盖最后一次写入。
#[tokio::test]
async fn language_repeated_choice_has_distinct_completion_sequence() {
    let temp = tempfile::TempDir::new().expect("应创建隔离的语言设置目录");
    let store = storage::SettingsStore::new(temp.path().join("settings.sqlite"));
    let writer = LanguagePersistence::start(store.clone());
    let first = writer
        .select(LanguagePreference::SimplifiedChinese)
        .expect("队列应接受中文");
    writer
        .select(LanguagePreference::English)
        .expect("队列应接受英文");
    let latest = writer
        .select(LanguagePreference::SimplifiedChinese)
        .expect("队列应接受最后的中文");
    let completed = writer
        .results
        .recv()
        .await
        .expect("首次写入必须返回关联序号");
    assert_eq!(completed.sequence, first, "首个结果必须保留首次选择序号");
    assert_ne!(
        completed.sequence, latest,
        "重复偏好不能使旧错误被当成最新结果"
    );
    writer.shutdown().await;
    assert_eq!(
        store
            .get_string(LANGUAGE_KEY)
            .expect("应读取保存值")
            .as_deref(),
        Some("zh-CN"),
        "退出屏障必须等待最新中文写入完成"
    );
    assert_ne!(
        store
            .get_string(LANGUAGE_KEY)
            .expect("应读取保存值")
            .as_deref(),
        Some("en"),
        "中间的英文不能覆盖最终中文"
    );
}

/// 通用设置在墨黑主题的桌面上保持组件文字可读，不被桌面深色输入主题覆盖。
///
/// 数据构造：一个隔离 App；使用默认墨黑主题；Settings 打开，桌面没有展开项目。
/// 执行过程：设置墨黑主题 → 打开通用设置 → 渲染桌面 → 读取组件库 foreground。
/// 预期结果：组件文字等于墨黑产品主题的柔白文字；不等于桌面默认深色主题的墨黑文字。
#[test]
fn language_controls_keep_settings_theme_over_desktop() {
    let mut test = gpui::TestAppContext::single();
    let window = test.add_window(|_, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| {
                Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
            }),
        )
    });
    let id = ui::ThemeId::InkBlack;
    window
        .update(&mut test, |app, window, cx| {
            app.set_theme(id, cx);
            app.settings_panel.open();
            app.settings_panel
                .select(crate::settings::panel::SettingsSection::General);
            let _ = app.render_desktop_workspace(window, cx);
            let actual = ui::component::Theme::global(cx).colors.foreground;
            let expected: gpui::Hsla = gpui::rgba(ui::Theme::for_id(id).text_primary).into();
            let desktop: gpui::Hsla = gpui::rgba(ui::Theme::sapphire_night().text_primary).into();
            assert_eq!(actual, expected, "语言控件必须保持设置页的墨黑主题文字");
            assert_ne!(
                actual, desktop,
                "桌面深色输入主题不能使墨黑语言选项文字变白"
            );
        })
        .expect("应能渲染设置背后的桌面");
}

/// 退出前等待后台保存，等待期间的最新语言选择也必须保存，等待不会关闭 writer。
///
/// 数据构造：隔离数据库 1 个；手动暂停后台队列；English → Chinese 共 2 次选择。
/// 执行过程：暂停 writer → 选择英文 → poll 退出保存 future → 选择中文 → 释放 writer → 等待。
/// 预期结果：后台未完成时 future 不可结束；最终中文保存；writer 仍接受下一次英文选择。
#[tokio::test]
async fn language_quit_barrier_waits_for_latest_choice_without_stopping_writer() {
    use futures::FutureExt;
    let test = gpui::TestAppContext::single();
    let temp = tempfile::TempDir::new().expect("应创建隔离测试目录");
    let store = storage::SettingsStore::new(temp.path().join("settings.sqlite"));
    test.update(|cx| {
        install(
            PreparedLanguageSettings::load_from_store(store.clone(), &["en".into()]),
            cx,
        )
    });
    let writer =
        test.read_global::<AppLanguage, _>(|state, _| state.writer.clone().expect("应有 writer"));
    let resume = writer.pause();
    test.update(|cx| select_language(LanguagePreference::English, cx));
    let mut async_cx = test.to_async();
    let barrier = wait_for_latest_save(&mut async_cx);
    futures::pin_mut!(barrier);
    assert!(
        barrier.as_mut().now_or_never().is_none(),
        "未保存的英文选择必须阻止退出继续"
    );
    test.update(|cx| select_language(LanguagePreference::SimplifiedChinese, cx));
    resume.send(()).expect("应释放后台 writer");
    barrier.await.expect("退出保存屏障应完成");
    assert_eq!(
        store
            .get_string(LANGUAGE_KEY)
            .expect("应读取语言")
            .as_deref(),
        Some("zh-CN"),
        "等待期间的新选择也必须保存"
    );
    assert_ne!(
        store
            .get_string(LANGUAGE_KEY)
            .expect("应读取语言")
            .as_deref(),
        Some("en"),
        "旧的英文不能覆盖最新中文"
    );
    writer
        .select(LanguagePreference::English)
        .expect("取消退出时 writer 仍须可用");
    writer.flush().await.expect("后续保存应继续完成");
    assert_eq!(
        store
            .get_string(LANGUAGE_KEY)
            .expect("应读取语言")
            .as_deref(),
        Some("en"),
        "屏障不能提前终止 writer"
    );
    writer.shutdown().await;
}
