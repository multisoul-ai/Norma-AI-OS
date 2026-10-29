use crate::app::MultisoulApp;
use gpui::{AppContext, TestAppContext};

/// 场景：六个现有主题必须贯穿桌面表面，浅色主题不得被替换成蓝宝石之夜。
/// 数据构造：隔离 App，依次使用 ThemeId::ALL；颜色以用户选择的主题为基准。
/// 执行过程：调用实际 apply_theme，再读取桌面绘制所使用的主题。
/// 预期结果：正文与强调色保持当前选择，表面底色色相一致，浅色正文不是白色。
#[test]
fn desktop_surface_follows_each_selected_theme() {
    let mut test = TestAppContext::single();
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("隔离存储");
    let app = test.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
    for id in ui::ThemeId::ALL {
        app.update(&mut test, |app, cx| app.apply_theme(id, cx));
        let surface = app.read_with(&test, |app, _| app.desktop_theme());
        let chosen = ui::Theme::for_id(id);
        assert_eq!(
            surface.text_primary, chosen.text_primary,
            "{id:?} 桌面正文必须跟随所选主题"
        );
        assert_eq!(surface.accent, chosen.accent, "{id:?} 强调色不得回退蓝色");
        assert_eq!(
            surface.panel >> 8,
            chosen.canvas_from >> 8,
            "{id:?} 表面底色色相必须来自所选主题"
        );
        if !id.is_dark() {
            assert_ne!(
                surface.text_primary >> 8,
                0xffffff,
                "{id:?} 浅色表面不得固定白字"
            );
        }
    }
}

/// 场景：桌面渲染同步输入框时，不得把刚选中的浅色组件主题改回深色。
/// 数据构造：同一个 App 依次遍历六个主题，分别模拟收起和展开的输入区域。
/// 执行过程：apply_theme 后调用生产 sync_desktop_input_theme，读取组件全局前景。
/// 预期结果：两种布局均保留所选主题前景；旧的 SapphireNight 前景不会污染浅色输入。
#[test]
fn desktop_theme_propagation_preserves_component_foreground() {
    let mut test = TestAppContext::single();
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("隔离存储");
    let app = test.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
    for id in ui::ThemeId::ALL {
        for expanded in [false, true] {
            app.update(&mut test, |app, cx| {
                app.apply_theme(id, cx);
                app.sync_desktop_input_theme(expanded, cx);
                let expected: gpui::Hsla = gpui::rgba(ui::Theme::for_id(id).text_primary).into();
                assert_eq!(
                    ui::component::Theme::global(cx).colors.foreground,
                    expected,
                    "{id:?} expanded={expanded} 输入框必须保留本轮选择的前景色"
                );
            });
        }
    }
}

/// 场景：浅色抽屉不能继续把固定黑色重阴影投在卡片正文上。
/// 数据构造：六主题，侧栏原 alpha=112、浮层原 alpha=187；浅色上限 38/255≈15%。
/// 执行过程：读取实际抽屉阴影解析函数，检查浅色透明度与主题色相，回归深色原值。
/// 预期结果：浅色使用克制的主题阴影；深色强度不被本轮浅色设计修改。
#[test]
fn desktop_theme_shadows_are_restrained_on_light_surfaces() {
    use super::surface_theme::shadow_color;
    for id in ui::ThemeId::ALL {
        let theme = ui::Theme::for_id(id);
        for dark_alpha in [112, 187] {
            let color = shadow_color(theme, dark_alpha);
            if id.is_dark() {
                assert_eq!(color, u32::from(dark_alpha), "{id:?} 保留深色黑色阴影强度");
            } else {
                assert!(color & 255 <= 38, "{id:?} 阴影不透明度不能压出深灰竖带");
                assert_eq!(
                    color >> 8,
                    theme.shadow_color >> 8,
                    "{id:?} 阴影应带主题色相"
                );
                assert_ne!(color, 0x00000070, "{id:?} 不得使用固定黑色重阴影");
            }
        }
    }
}
