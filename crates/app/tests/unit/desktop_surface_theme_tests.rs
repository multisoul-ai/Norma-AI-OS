use crate::app::MultisoulApp;
use gpui::{AppContext, TestAppContext};

/// 场景：工作空间与独立项目卡片和抽屉同样通透，目录内的嵌套预览不重复叠底。
/// 数据：隔离工作空间、一个空项目；六个当前主题和三个旧浅色夹具；60%×255=153。
/// 执行：构造生产路径的三种真实元素，读取其背景样式并与抽屉染色层比较。
/// 预期：两种独立卡片为60%背景，嵌套预览无背景；文字所在根元素仍完全可见。
#[test]
fn desktop_cards_share_drawer_transparency_without_stacking() {
    use gpui::{px, size, Styled};
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Glass")
        .expect("应创建空间");
    let area = storage::DesktopProjectArea {
        id: "project".into(),
        desktop_workspace_id: workspace.id.clone(),
        project_key: temp.path().into(),
        root: temp.path().into(),
        display_name: "Project".into(),
        added_at_unix_ms: 0,
        wallpaper: None,
    };
    let preview = storage::DesktopAreaPreview {
        id: area.id.clone(),
        name: area.display_name.clone(),
        nodes: Vec::new(),
    };
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        ui::component::Root::new(app, window, cx)
    });
    let host = host.expect("应创建App宿主");
    for id in ui::ThemeId::ALL.into_iter().chain([
        ui::ThemeId::PaperBlue,
        ui::ThemeId::LinenSage,
        ui::ThemeId::SilverViolet,
    ]) {
        cx.update(|window, cx| {
            host.update(cx, |app, cx| {
                app.theme_id = id;
                app.theme = ui::Theme::for_id(id);
                app.desktop_workspace.home_directory = false;
                app.desktop_workspace.active_id = Some(workspace.id.clone());
                app.desktop_workspace.workspaces = vec![workspace.clone()];
                app.desktop_workspace.areas = vec![area.clone()];
                let mut workspace_card = app.desktop_workspace_card(
                    0,
                    workspace.clone(),
                    400.0,
                    size(px(1280.0), px(800.0)),
                    1.0,
                    cx,
                );
                let mut project_card = app.desktop_project_area_card(
                    0,
                    area.clone(),
                    canvas::Vec2::new(400.0, 250.0),
                    1.0,
                    window,
                    cx,
                );
                let mut nested = app.render_desktop_project_preview(
                    &workspace.id,
                    &preview,
                    canvas::Vec2::new(200.0, 125.0),
                    0.5,
                    canvas::Vec2::new(1280.0, 800.0),
                    cx,
                );
                let tint = super::drawers::material::drawer_tint(app.desktop_theme(), true).0;
                assert_eq!(tint & 255, 153, "{id:?} 抽屉基准为60%不透明度");
                let expected = Some(gpui::Fill::from(gpui::rgba(tint)));
                assert_eq!(
                    (
                        workspace_card.style().background.clone(),
                        project_card.style().background.clone(),
                        nested.style().background.clone()
                    ),
                    (expected.clone(), expected, None),
                    "{id:?} 独立卡片必须和抽屉一致，嵌套预览不能重复叠底"
                );
                assert_eq!(
                    workspace_card.style().opacity,
                    Some(1.0),
                    "工作空间文字不能一起变淡"
                );
                assert_eq!(
                    project_card.style().opacity,
                    Some(1.0),
                    "项目文字不能一起变淡"
                );
            })
        });
    }
}

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
