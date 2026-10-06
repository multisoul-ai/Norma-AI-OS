use super::*;
use crate::app::AppScreen;
use gpui::{size, AppContext, Modifiers, TestAppContext};

fn home_text_contrast(text: u32, surface: u32, wallpaper: u32) -> f32 {
    let composite = |foreground: u32, background: [f32; 3]| {
        let alpha = (foreground & 0xff) as f32 / 255.0;
        [24, 16, 8].map(|shift| {
            ((foreground >> shift) & 0xff) as f32 / 255.0 * alpha
                + background[(24 - shift) / 8] * (1.0 - alpha)
        })
    };
    let luminance = |color: [f32; 3]| {
        let linear = color.map(|channel| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        });
        linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
    };
    let background = composite(surface, composite(wallpaper, [0.0; 3]));
    let foreground = luminance(composite(text, background));
    let background = luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

/// 首页卡片的项目数量和空态提示在亮、暗壁纸上都保持可读对比。
///
/// 数据构造（含关键数值的推导过程）：
///   6 个现有主题；壁纸用纯白与纯黑覆盖 sRGB 亮度两端。
///   卡片使用对应主题的 Dock 材质；合成色 = 前景 × alpha + 背景 × (1-alpha)。
///   不透明度范围 65%–85%：255×0.65≈166，255×0.85≈217，与顶部材质一致。
///   WCAG 小字号对比下限 = 4.5:1；相对亮度按 sRGB 线性化及 0.2126/0.7152/0.0722 加权。
/// 执行过程：
///   1. 读取首页卡片材质及两级辅助文字颜色。
///   2. 分别叠加到纯白、纯黑壁纸，计算项目数量与空态提示的最终对比。
///
/// 预期结果：
///   - 正断言：数量、空态提示及实心主按钮文字的对比都达到 4.5:1。
///   - 负断言：不能使用接近不透明的浅色底，也不能透明到让壁纸覆盖文字。
#[test]
fn desktop_home_card_text_remains_readable_over_wallpaper() {
    use super::super::home::home_theme;
    for id in ui::ThemeId::ALL {
        let theme = home_theme(id);
        let surface = theme.panel;
        let chrome = crate::workspace::desktop::workspace_chrome_theme(id);
        assert_eq!(
            surface, chrome.dock,
            "{id:?} 卡片必须与顶部和底部使用相同材质"
        );
        let alpha = surface & 0xff;
        assert!(alpha >= 166, "{id:?} 卡片材质需支撑亮壁纸上的文字");
        assert!(alpha <= 217, "{id:?} 卡片必须透出壁纸，不能成为不透明色块");
        assert!(
            home_text_contrast(theme.text_secondary, surface, 0xffffffff) >= 4.5,
            "{id:?} 项目数量必须在纯白壁纸上保持至少 4.5:1 对比"
        );
        assert!(
            home_text_contrast(theme.text_tertiary, surface, 0xffffffff) >= 4.5,
            "{id:?} 空态提示必须在纯白壁纸上保持至少 4.5:1 对比"
        );
        assert!(
            home_text_contrast(theme.text_secondary, surface, 0x000000ff) >= 4.5,
            "{id:?} 项目数量必须在纯黑壁纸上保持至少 4.5:1 对比"
        );
        assert!(
            home_text_contrast(theme.text_tertiary, surface, 0x000000ff) >= 4.5,
            "{id:?} 空态提示必须在纯黑壁纸上保持至少 4.5:1 对比"
        );
    }
    let theme = home_theme(ui::ThemeId::SapphireNight);
    assert!(
        home_text_contrast(0xffffffff, theme.accent, 0xffffffff) >= 4.5,
        "实心蓝色主按钮的白色文字应达到 4.5:1 对比"
    );
}

/// 顶部透明材质透出壁纸，同时保留工作空间、项目和状态文字的可读性。
///
/// 数据构造（含关键数值的推导过程）：
///   6 个主题，纯白/纯黑壁纸覆盖最亮/最暗背景；共 6×2 种组合。
///   顶栏必须取对应主题的 dock/dock_line，三级文字均使用浅色以配合深色材质。
///   材质不透明度范围 65%–85%：255×0.65≈166，255×0.85≈217。
///   最终色 = 材质×alpha + 壁纸×(1-alpha)；小字号 WCAG 对比下限为 4.5:1。
/// 执行过程：
///   1. 读取首页、项目总览和画布共用的顶部材质及三级文字。
///   2. 合成两端壁纸，分别计算主标题、空间/状态文字、分隔符的对比。
///
/// 预期结果：
///   - 正断言：三级文字在两端壁纸上均达到 4.5:1。
///   - 负断言：顶部不能恢复浅色标题栏，也不能透明到削弱文字辨识。
#[test]
fn desktop_top_bar_translucent_text_remains_readable_over_wallpaper() {
    for id in ui::ThemeId::ALL {
        let theme = crate::workspace::desktop::workspace_chrome_theme(id);
        let original = ui::Theme::for_id(id);
        assert_eq!(
            theme.title_bar, original.dock,
            "{id:?} 首页与画布顶栏应复用底栏材质"
        );
        assert_eq!(
            theme.dock_line, original.dock_line,
            "{id:?} 顶栏分隔线应与底栏一致"
        );
        assert_ne!(
            theme.title_bar, original.title_bar,
            "{id:?} 顶栏不能恢复旧标题栏底色"
        );
        let alpha = theme.title_bar & 0xff;
        assert!(alpha >= 166, "{id:?} 顶部需保留足够材质厚度支撑文字");
        assert!(alpha <= 217, "{id:?} 顶部不能继续遮住几乎全部壁纸");
        assert!(
            home_text_contrast(theme.text_primary, theme.title_bar, 0xffffffff) >= 4.5,
            "{id:?} 主标题必须在纯白壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_primary, theme.title_bar, 0x000000ff) >= 4.5,
            "{id:?} 主标题必须在纯黑壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_secondary, theme.title_bar, 0xffffffff) >= 4.5,
            "{id:?} 工作空间和保存状态必须在纯白壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_secondary, theme.title_bar, 0x000000ff) >= 4.5,
            "{id:?} 工作空间和保存状态必须在纯黑壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_tertiary, theme.title_bar, 0xffffffff) >= 4.5,
            "{id:?} 层级分隔符必须在纯白壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_tertiary, theme.title_bar, 0x000000ff) >= 4.5,
            "{id:?} 层级分隔符必须在纯黑壁纸上达到 4.5:1"
        );
    }
}

/// 首次启动的空目录提供居中的创建入口，取消无副作用，创建后退出欢迎态。
///
/// 数据构造：隔离数据库含0个工作空间；窗口1280×800，工具栏52px，内容中心y=(52+800)/2=426；
///   窄窗360×480，左右至少24px；几何容差1px来自GPUI像素取整。浅色主题覆盖空态字体与布局。
/// 执行过程：启动空目录 → 检查欢迎态及唯一创建入口 → 缩窗/换主题 → 创建弹窗取消
///   → 再次创建并命名 → 返回首页 → 搜索不存在名称。
/// 预期结果：不显示空列表搜索和继续工作分组；窄窗标题/按钮不截断；取消不写入，
///   首次创建仅写入1个空工作空间；无搜索结果不冒充首次启动欢迎页。
#[test]
fn desktop_home_first_workspace_welcome_and_creation() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let welcome = cx
        .debug_bounds("desktop-home-welcome")
        .expect("空目录应显示欢迎面板");
    assert!(
        (welcome.center().x - px(640.0)).abs() <= px(1.0),
        "欢迎面板应水平居中"
    );
    assert!(
        (welcome.center().y - px(426.0)).abs() <= px(1.0),
        "欢迎面板应居中于工具栏下方，不能挤在左上角"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-search").is_none(),
        "无工作空间时不能显示无用搜索"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-scroll").is_none(),
        "欢迎态不能沿用空列表滚动及分组布局"
    );
    let create = cx
        .debug_bounds("desktop-workspace-create")
        .expect("欢迎态必须有创建入口");
    assert!(
        welcome.contains(&create.center()),
        "创建按钮应在欢迎面板内，不能只留在工具栏"
    );
    assert_eq!(
        cx.debug_bounds("desktop-home-wallpaper")
            .expect("欢迎态复用设置壁纸")
            .size,
        size(px(1280.0), px(800.0)),
        "壁纸应覆盖整个窗口"
    );
    host.update(cx, |app, cx| {
        app.theme_id = ui::ThemeId::ALL
            .into_iter()
            .find(|id| !id.is_dark())
            .expect("应有浅色主题");
        app.theme = ui::Theme::for_id(app.theme_id);
        app.motion.reduce_motion = true;
        cx.notify();
    });
    cx.simulate_resize(size(px(360.0), px(480.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let welcome = cx
        .debug_bounds("desktop-home-welcome")
        .expect("窄窗仍显示欢迎面板");
    let title = cx
        .debug_bounds("desktop-home-welcome-title")
        .expect("应显示欢迎标题");
    let create = cx
        .debug_bounds("desktop-workspace-create")
        .expect("窄窗应保留创建按钮");
    assert!(
        welcome.left() >= px(24.0) && welcome.right() <= px(336.0),
        "360px窄窗左右各保留至少24px"
    );
    assert!(
        welcome.top() >= px(52.0) && welcome.bottom() <= px(480.0),
        "欢迎内容不能覆盖工具栏或越出底部"
    );
    assert!(
        title.left() >= welcome.left() && title.right() <= welcome.right(),
        "标题应随窄窗换行，不能撑破面板"
    );
    assert!(
        create.top() > title.bottom() && create.bottom() <= welcome.bottom(),
        "创建按钮不能与标题重叠或被裁断"
    );
    cx.simulate_click(create.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    let cancel = cx
        .debug_bounds("desktop-workspace-create-cancel")
        .expect("应打开原有命名弹窗");
    cx.simulate_click(cancel.center(), Modifiers::none());
    assert!(
        catalog.list().expect("读取目录").is_empty(),
        "取消不能创建默认命名的工作空间"
    );
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_click(create.center(), Modifiers::none());
    cx.simulate_input("My first workspace");
    cx.update(|window, cx| window.draw(cx).clear());
    let confirm = cx
        .debug_bounds("desktop-workspace-create-confirm")
        .expect("应显示创建确认按钮");
    cx.simulate_click(confirm.center(), Modifiers::none());
    cx.run_until_parked();
    let created = catalog.list().expect("读取创建结果");
    assert_eq!(created.len(), 1, "只应创建一个工作空间");
    assert_eq!(
        created[0].name, "My first workspace",
        "必须保留用户输入名称"
    );
    assert_eq!(
        created[0].project_count, 0,
        "首次创建仍从空画布开始，不能塞入示例项目"
    );
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.screen,
            AppScreen::DesktopWorkspace,
            "创建成功后应进入新工作空间"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "不能跳入不存在的项目"
        );
        app.show_desktop_launchpad(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-home-card-0").is_some(),
        "返回首页应展示刚创建的工作空间"
    );
    assert!(
        cx.debug_bounds("desktop-home-welcome").is_none(),
        "创建成功后不能继续显示首次欢迎态"
    );
    host.update(cx, |app, cx| {
        app.desktop_workspace.search_query = "missing workspace".into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-workspace-search").is_some(),
        "搜索无结果时应保留搜索入口"
    );
    assert!(
        cx.debug_bounds("desktop-home-welcome").is_none(),
        "搜索无结果不能显示首次创建欢迎态"
    );
    assert!(
        cx.debug_bounds("desktop-home-card-0").is_none(),
        "搜索不能保留不匹配的工作空间"
    );
}

/// 场景：启动首页使用主预览和紧凑卡片，名称、搜索与新建入口随窄窗重排。
///
/// 数据构造：5 个独立空工作空间，窗口1280×800；窄窗420×600。
///   3 个空间在首组，余下2个在第二组；窄窗只保留一列，不挤压名称。
///   原型1586×992等比到1280×800：主卡高约370px，末行底部约758px。
/// 执行过程：渲染首页 → 检查主卡/侧卡和工具栏 → 改主题、缩窗 → 滚到底 → 搜索 → 打开结果。
/// 预期结果：主卡宽于侧卡，第三项与侧卡同列；所有控件在窗口内；
///   滚动区域不能覆盖工具栏；搜索只显示匹配项，点击后打开正确空间，不自动展开项目。
#[test]
fn desktop_home_layout_search_and_navigation() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let first = catalog.create("Design").expect("应创建首空间");
    catalog.create("Development").expect("应创建第二空间");
    catalog
        .create("Long workspace name for collaboration review")
        .expect("应创建长名称空间");
    catalog.create("Archive").expect("应创建第四空间");
    catalog.create("Notes").expect("应创建第五空间");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.screen = AppScreen::Launchpad;
        app.desktop_workspace.workspaces = catalog.list().expect("应加载空间");
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let wallpaper = cx
        .debug_bounds("desktop-home-wallpaper")
        .expect("首页必须使用设置中的壁纸层");
    assert_eq!(
        wallpaper.size,
        size(px(1280.0), px(800.0)),
        "壁纸必须覆盖整个首页，不能只画在缩略图中"
    );
    let featured = cx
        .debug_bounds("desktop-home-card-0")
        .expect("首页应有大预览卡");
    let side = cx
        .debug_bounds("desktop-home-card-1")
        .expect("应有紧凑侧卡");
    let next = cx
        .debug_bounds("desktop-home-card-2")
        .expect("应有第二张侧卡");
    assert!(featured.size.width > side.size.width, "主预览应占更大宽度");
    assert_eq!(side.left(), next.left(), "两张侧卡应同列");
    assert!(next.top() > side.bottom(), "侧卡不能互相覆盖");
    assert!(
        featured.size.height <= px(375.0),
        "主卡高度应接近原型的370px，不能撑高首屏"
    );
    let last = cx.debug_bounds("desktop-home-card-4").expect("应显示末卡");
    assert!(
        last.bottom() <= px(768.0),
        "五个工作空间应在800px首屏内完整展示并保留至少32px边距"
    );
    let panel = cx
        .debug_bounds("desktop-home-preview-panel-0")
        .expect("主卡应有独立缩略图底板");
    assert!(
        panel.size.width >= featured.size.width - px(28.0),
        "主预览底板必须铺满卡片宽度，仅留两侧内边距"
    );
    let title = cx
        .debug_bounds("desktop-home-title-0")
        .expect("应显示主卡名称");
    let count = cx
        .debug_bounds("desktop-home-count-0")
        .expect("应显示项目数量");
    assert!(
        (title.center().y - count.center().y).abs() <= px(4.0),
        "宽窗主卡名称和项目数量应位于同一行"
    );
    assert!(
        cx.debug_bounds("desktop-home-empty-1").is_some(),
        "空空间应显示独立的空状态插画"
    );
    assert!(
        cx.debug_bounds("desktop-canvas-command-bar").is_none(),
        "首页不应保留旧底部搜索条"
    );
    for theme_id in ui::ThemeId::ALL {
        host.update(cx, |app, cx| {
            app.theme_id = theme_id;
            app.theme = ui::Theme::for_id(theme_id);
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear());
        cx.update(|_, cx| {
            let foreground: gpui::Hsla = rgba(ui::Theme::for_id(theme_id).text_primary).into();
            assert_eq!(
                ui::component::Theme::global(cx).colors.foreground,
                foreground,
                "首页输入组件必须跟随当前主题 {theme_id:?}"
            );
        });
    }
    cx.simulate_resize(size(px(420.0), px(600.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let create = cx
        .debug_bounds("desktop-workspace-create")
        .expect("窄窗应保留新建入口");
    let search = cx
        .debug_bounds("desktop-workspace-search")
        .expect("窄窗应保留搜索");
    assert!(create.right() <= px(420.0), "新建按钮不能越出窗口");
    assert!(search.top() >= create.bottom(), "窄窗搜索应移到下一行");
    let scroll = cx
        .debug_bounds("desktop-workspace-scroll")
        .expect("首页应保留滚动区域");
    assert!(
        scroll.top() >= search.bottom(),
        "滚动卡片必须裁剪在搜索栏下方，不能透过玻璃工具栏形成文字重叠"
    );
    let side = cx
        .debug_bounds("desktop-home-card-1")
        .expect("窄窗应保留侧卡");
    let featured = cx
        .debug_bounds("desktop-home-card-0")
        .expect("窄窗应保留主卡");
    assert!(side.top() > featured.bottom(), "窄窗卡片应按单列排列");
    let title = cx
        .debug_bounds("desktop-home-title-0")
        .expect("窄窗必须显示主卡标题");
    let count = cx
        .debug_bounds("desktop-home-count-0")
        .expect("窄窗必须显示项目数量");
    // 标题行高23px、数量行高20px，换行后必须分别容纳完整的一行文字。
    assert!(
        title.size.height >= px(23.0),
        "窄窗主卡标题不能被数量挤压裁断"
    );
    assert!(title.bottom() <= count.top(), "窄窗主卡标题与数量不能重叠");
    assert!(
        count.bottom() <= featured.bottom() - px(10.0),
        "数量必须留在卡片内并保留底边距"
    );
    host.update(cx, |app, cx| {
        // 5 张卡片内容不足 2000px；请求 -10000px，验证滚动上限裁剪而非固定滚动量。
        app.desktop_directory_scroll
            .set_offset(gpui::point(px(0.0), px(-10000.0)));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let last = cx
        .debug_bounds("desktop-home-card-4")
        .expect("滚到底应能显示最后一个空间");
    assert_eq!(
        last.bottom(),
        px(600.0 - 32.0),
        "末卡下面只应保留32px留白，不能多出工具栏高度"
    );
    host.update(cx, |app, cx| {
        app.desktop_directory_scroll
            .set_offset(gpui::point(px(0.0), px(0.0)));
        app.desktop_workspace.search_query = "Design".to_string();
        app.motion.reduce_motion = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-home-card-1").is_none(),
        "不匹配的卡片应移除"
    );
    let result = cx
        .debug_bounds("desktop-home-card-0")
        .expect("应保留搜索结果");
    cx.simulate_click(result.center(), Modifiers::none());
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.active_id.as_deref(),
            Some(first.id.as_str()),
            "必须打开搜索到的空间"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "打开空间不能自动展开项目"
        );
    });
}
