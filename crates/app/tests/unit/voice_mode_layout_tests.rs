use crate::app::MultisoulApp;
use gpui::{px, size, AppContext, TestAppContext};

/// 场景：英文模式说明按两行短句呈现，必须完整留在自身卡片内。
/// 构造：与真实验收相同的1280×850窗口、英文和墨黑主题、隔离空设置。
/// 执行：打开真实设置渲染入口，绘制后读取三张卡片和说明边界。
/// 预期：每段说明超过一行且最多三行；Helper行高15.4px，半像素舍入后单行15.5px、三行46.5px。
/// 同时要求说明使用至少100px宽度，避免以逐字竖排满足换行断言。
#[test]
fn english_voice_mode_descriptions_wrap_inside_each_card() {
    let temp = tempfile::tempdir().expect("隔离设置目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("隔离设置存储");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(850.0)));
    cx.update(|_, cx| {
        ui::i18n::set_app_locale(ui::i18n::Locale::En, cx);
        ui::sync_component_theme(ui::ThemeId::InkBlack, cx);
    });
    host.expect("应用宿主")
        .update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    for (mode, selector, hint_selector) in [
        (
            "legacy",
            "settings-voice-desktop-mode-legacy",
            "settings-voice-desktop-mode-legacy-hint",
        ),
        (
            "doubao",
            "settings-voice-desktop-mode-doubao",
            "settings-voice-desktop-mode-doubao-hint",
        ),
        (
            "gpt-live",
            "settings-voice-desktop-mode-gpt-live",
            "settings-voice-desktop-mode-gpt-live-hint",
        ),
    ] {
        let card = cx.debug_bounds(selector).expect("模式卡片");
        let hint = cx.debug_bounds(hint_selector).expect("模式说明");
        assert!(
            hint.size.height > px(20.0),
            "{mode}英文说明必须换行：{hint:?}"
        );
        assert!(
            hint.size.height <= px(46.5),
            "{mode}说明不能超过三行：{hint:?}"
        );
        assert!(
            hint.size.width >= px(100.0),
            "{mode}说明不能逐字竖排：{hint:?}"
        );
        assert!(
            hint.left() >= card.left() && hint.right() <= card.right(),
            "{mode}说明不能穿过卡片边界：{hint:?}, {card:?}"
        );
        assert!(hint.bottom() <= card.bottom(), "{mode}说明不能越过卡片底边");
    }
}
