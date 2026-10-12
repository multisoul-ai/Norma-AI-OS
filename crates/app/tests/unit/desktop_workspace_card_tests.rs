use super::*;
use crate::app::AppScreen;
use gpui::{point, size, Entity, Modifiers, Render, TestAppContext, Window};

struct DirectoryView(Entity<MultisoulApp>);

impl Render for DirectoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().child(
            self.0
                .update(cx, |app, cx| app.render_desktop_launchpad(window, cx)),
        )
    }
}

struct KeyboardCardView(Entity<MultisoulApp>, bool);

impl Render for KeyboardCardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |app, cx| {
            let workspace = app.desktop_workspace.workspaces[0].clone();
            let card = if self.1 {
                app.desktop_home_card(
                    0,
                    workspace,
                    canvas::Vec2::new(400., 250.),
                    canvas::Vec2::new(1280., 800.),
                    1.,
                    cx,
                )
                .into_any_element()
            } else {
                app.desktop_workspace_card(0, workspace, 400., size(px(1280.), px(800.)), 1., cx)
                    .into_any_element()
            };
            div()
                .id("keyboard-test-root")
                .w(px(400.))
                .h(px(250.))
                .child(card)
        })
    }
}

/// 两套目录中的卡片与子菜单按钮分别响应键盘，子按钮不得触发父卡片导航。
///
/// 数据构造：每套目录各 1 个空工作空间；卡片400×250，保持1280/800宽高比；无运行进程。
/// 执行过程：按 Tab 顺序聚焦菜单 → Enter 打开 → Space 关闭 → 聚焦父卡片 → Enter 打开空间。
/// 预期结果：子按钮只切换菜单且不激活空间；父卡片仍能通过键盘进入正确空间。
#[test]
fn desktop_workspace_card_keyboard_does_not_open_from_child_buttons() {
    for home in [false, true] {
        let temp = tempfile::tempdir().expect("应创建隔离目录");
        let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
        let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
        let workspace = store
            .desktop_workspace_store()
            .create("Keyboard")
            .expect("应创建空间");
        let mut test = TestAppContext::single();
        let (view, cx) = test.add_window_view(|_, cx| {
            let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
            app.update(cx, |app, _| {
                app.desktop_workspace.workspaces = vec![workspace.clone()]
            });
            KeyboardCardView(app, home)
        });
        let host = view.update(cx, |view, _| view.0.clone());
        cx.simulate_resize(size(px(1280.0), px(800.0)));
        cx.run_until_parked();
        host.update(cx, |app, cx| {
            app.screen = AppScreen::Launchpad;
            app.desktop_workspace.home_directory = home;
            app.desktop_workspace.workspaces = vec![workspace.clone()];
            app.motion.reduce_motion = true;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear());
        cx.update(|window, cx| {
            window.blur();
            window.focus_next(); // 父卡片。
            window.focus_next(); // 子菜单按钮。
            window.draw(cx).clear();
        });
        view.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("enter").expect("合法按键"),
            is_held: false,
        });
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").expect("合法按键"),
        });
        cx.run_until_parked();
        host.read_with(cx, |app, _| {
            assert_eq!(
                app.screen,
                AppScreen::Launchpad,
                "子按钮 Enter 不得进入空间（home={home}）"
            );
            assert_eq!(
                app.desktop_workspace.actions_workspace_id.as_deref(),
                Some(workspace.id.as_str()),
                "Enter 应打开菜单"
            );
            assert!(
                app.desktop_workspace.active_id.is_none(),
                "子按钮不得暗中激活空间"
            );
        });
        view.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("space").expect("合法按键"),
            is_held: false,
        });
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("space").expect("合法按键"),
        });
        cx.update(|window, cx| window.draw(cx).clear());
        host.read_with(cx, |app, _| {
            assert!(
                app.desktop_workspace.actions_workspace_id.is_none(),
                "Space 应关闭已打开的菜单"
            );
            assert_eq!(
                app.screen,
                AppScreen::Launchpad,
                "菜单 Space 不得打开父卡片"
            );
        });
        // 菜单按钮前一个可聚焦元素就是其父卡片，复用 GPUI 的 Tab 顺序。
        cx.update(|window, cx| {
            window.focus_prev();
            window.draw(cx).clear();
        });
        view.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("enter").expect("合法按键"),
            is_held: false,
        });
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").expect("合法按键"),
        });
        cx.run_until_parked();
        host.read_with(cx, |app, _| {
            assert_eq!(
                app.desktop_workspace.active_id.as_deref(),
                Some(workspace.id.as_str()),
                "父卡片必须仍可通过 Enter 打开"
            );
            assert_eq!(
                app.screen,
                AppScreen::DesktopWorkspace,
                "父卡片应进入工作空间"
            );
            assert!(
                app.desktop_workspace.expanded_area_id.is_none(),
                "空空间不得自动展开项目"
            );
        });
    }
}

/// 工作空间目录：长名称完全包含在卡片内，标题不挤占网格外间距，菜单不触发导航。
///
/// 数据构造：4 个空空间，首项名称为 40 个中文字符；窗口 1280×800。
///   3 列卡宽 = (1280-112-2×16)/3，卡高 = 卡宽×800/1280；第 4 项进入第二行。
///   GPUI 布局会取整到像素：高度容差 0.5px，约 180px 内容高的比例误差小于 0.01。
/// 执行过程：渲染真实目录 → 检查标题/菜单边界及两行间距 → 点击菜单 → 点击名称。
/// 预期结果：标题、菜单都在卡内且不重叠，空状态标题至少 16px 高并居中；
///   行距仅 16px；菜单停留目录，名称进入对应空间，不自动展开项目。
#[test]
fn desktop_workspace_card_keeps_title_inside_and_menu_local() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let first = catalog
        .create(&"工作空间".repeat(10))
        .expect("应创建长名称空间");
    let workspaces = vec![
        first.clone(),
        catalog.create("Design").expect("应创建 Design"),
        catalog.create("Development").expect("应创建 Development"),
        catalog.create("Review").expect("应创建 Review"),
    ];
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        DirectoryView(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    let viewport = size(px(1280.0), px(800.0));
    cx.simulate_resize(viewport);
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.screen = AppScreen::Launchpad;
        app.desktop_workspace.home_directory = false;
        app.desktop_workspace.switcher_open = true;
        app.desktop_workspace.workspaces = workspaces;
        app.motion.reduce_motion = true;
        cx.notify();
    });
    cx.draw(point(px(0.0), px(0.0)), viewport, |_, _| {
        gpui::AnyView::from(view.clone()).into_element()
    });
    let card = cx
        .debug_bounds("desktop-workspace-card-0")
        .expect("应有卡片");
    let title = cx
        .debug_bounds("desktop-workspace-title-0")
        .expect("应有名称");
    let actions = cx
        .debug_bounds("desktop-workspace-actions-0")
        .expect("应有菜单");
    let header = cx
        .debug_bounds("desktop-workspace-header-0")
        .expect("应有标题栏");
    let content = cx
        .debug_bounds("desktop-workspace-content-0")
        .expect("应有真实画布内容区");
    let empty_title = cx
        .debug_bounds("desktop-workspace-empty-title-0")
        .expect("空工作空间应显示可读的预览标题");
    assert!(content.top() >= header.bottom(), "画布缩略图不能覆盖标题栏");
    assert!(
        content.bottom() < card.bottom() && content.left() > card.left(),
        "内容区应留在外壳内部，不能涂掉圆角或边框"
    );
    assert!(
        ((content.size.width / content.size.height) - 1280.0 / 800.0).abs() < 0.01,
        "画布必须保持原视口宽高比（允许布局像素取整）：{content:?}"
    );
    assert!(
        f32::from(empty_title.size.height) >= 16.0,
        "空状态标题不能按整张画布的缩略比例缩成小字"
    );
    assert!(
        empty_title.center().x >= content.left() && empty_title.center().x <= content.right(),
        "空状态标题应位于预览内容区中央"
    );
    let next_row = cx
        .debug_bounds("desktop-workspace-card-3")
        .expect("应有第二行");
    assert!(title.top() >= card.top(), "名称不得出现在卡片顶边外");
    assert!(title.bottom() <= card.bottom(), "名称不得超出卡片底边");
    assert!(
        title.left() >= card.left() && title.right() <= actions.left(),
        "长名称应省略，不能挤出外壳或覆盖菜单"
    );
    assert!(actions.right() <= card.right(), "菜单必须始终位于卡片内");
    assert_eq!(
        next_row.top() - card.bottom(),
        px(16.0),
        "网格只能保留 16px 行距，不得留下外置标题的 60px 占位"
    );
    let expected_height = ((1280.0 - 112.0 - 32.0) / 3.0) * 800.0 / 1280.0;
    assert!(
        (f32::from(card.size.height) - expected_height).abs() <= 0.5,
        "内置标题不能通过增高卡片降低空间密度"
    );
    cx.simulate_click(actions.center(), Modifiers::none());
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.actions_workspace_id.as_deref(),
            Some(first.id.as_str()),
            "菜单应属于当前空间"
        );
        assert!(
            matches!(app.screen, AppScreen::Launchpad),
            "菜单点击不得进入工作空间"
        );
        assert!(
            app.desktop_workspace.active_id.is_none(),
            "菜单不能暗中激活空间"
        );
    });
    cx.simulate_click(title.center(), Modifiers::none());
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.active_id.as_deref(),
            Some(first.id.as_str()),
            "点击名称应进入对应空间"
        );
        assert!(
            matches!(app.screen, AppScreen::DesktopWorkspace),
            "名称应和主体使用同一打开入口"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "进入空间不能自动展开项目"
        );
    });
}

/// 窄窗口的空工作空间预览保留可读标题，内容不被卡片裁切。
///
/// 数据构造（含关键数值的推导过程）：
///   940×400 窗口刚进入三列布局，卡宽下限 280，高=400×280/940≈119.15；
///   内容区按画布比例适配为约 148×63；英文标题换成两行约 42px，
///   加 30px 图标就超过 63px。1280×800 时内容充足，应显示完整说明。
/// 执行过程（逐步说明系统如何处理）：
///   1. 绘制 940×400 的三列目录，读取第一张卡的内容与标题边界。
///   2. 改为 1280×800，确认完整空状态说明恢复。
/// 预期结果：
///   - 窄窗标题完整位于内容区内，标题高度至少 18px。
///   - 窄窗不显示放不下的图标和次要说明；宽窗必须显示说明。
#[test]
fn desktop_workspace_empty_preview_fits_narrow_window() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspaces = ["Alpha", "Beta", "Gamma"]
        .into_iter()
        .map(|name| catalog.create(name).expect("应创建空工作空间"))
        .collect();
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        DirectoryView(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    cx.simulate_resize(size(px(940.0), px(400.0)));
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.screen = AppScreen::Launchpad;
        app.desktop_workspace.home_directory = false;
        app.desktop_workspace.switcher_open = true;
        app.desktop_workspace.workspaces = workspaces;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let content = cx
        .debug_bounds("desktop-workspace-content-0")
        .expect("窄窗应绘制内容区");
    let title = cx
        .debug_bounds("desktop-workspace-empty-title-0")
        .expect("窄窗应绘制空状态标题");
    assert!(
        title.top() >= content.top() && title.bottom() <= content.bottom(),
        "窄窗标题必须完整位于内容区内：content={content:?} title={title:?}"
    );
    assert!(
        title.left() >= content.left() && title.right() <= content.right(),
        "窄窗标题横向也必须完整位于内容区内"
    );
    assert!(
        f32::from(title.size.height) >= 18.0,
        "窄窗标题应保持可读，不能缩成微小文字"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-empty-icon-0").is_none(),
        "窄窗应省略放不下的图标"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-empty-description-0")
            .is_none(),
        "窄窗应省略放不下的次要说明"
    );
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-workspace-empty-description-0")
            .is_some(),
        "宽窗应恢复完整的空状态说明"
    );
}

/// 工作空间标题内置后，转场的内容起点与缩略图相同，终点恢复完整画布。
///
/// 数据构造：源外壳 (100,120,400,250)，原画布 1280×800；标题 40px、上下留白各 8px，
///   缩略可用高 = 250-40-16 = 194，宽 = 194×1280/800 = 310.4，水平居中留白 44.8。
/// 执行过程：计算目录内容区 → 计算半程外壳的内容区 → 计算展开终点内容区。
/// 预期结果：起点不使用整个外壳，缩略内容避开标题；中途保持等比放大；终点不保留标题空白。
#[test]
fn desktop_workspace_content_tracks_shell_without_a_landing_jump() {
    use super::super::scene::{overview_chrome_opacity, workspace_content_rect};
    use canvas::CanvasRect;
    let source = CanvasRect::new(100.0, 120.0, 400.0, 250.0);
    let full = CanvasRect::new(0.0, 0.0, 1280.0, 800.0);
    let preview = workspace_content_rect(source, full.size, 1.0);
    assert!(
        (preview.origin.x - 144.8).abs() < 0.001,
        "缩略画布应在内容区等比居中"
    );
    assert_eq!(
        preview.origin.y, 168.0,
        "内容从外壳顶部加 40px 标题和 8px 留白开始"
    );
    assert!(
        (preview.size.x - 310.4).abs() < 0.001,
        "缩略图宽度应保持等比（允许 f32 舍入）"
    );
    assert_eq!(preview.size.y, 194.0, "缩略图高度应适配标题下方内容区");
    assert_ne!(preview, source, "内容不能覆盖标题和外壳四角");
    // 中点的每个坐标为两端平均；外壳缩放=840/400=2.1，标题可见度=0.5。
    let midway = CanvasRect::new(50.0, 60.0, 840.0, 525.0);
    let chrome = midway.size.x / source.size.x * overview_chrome_opacity(midway, source, full);
    let content = workspace_content_rect(midway, full.size, chrome);
    assert!(
        content.size.x > preview.size.x && content.size.x < full.size.x,
        "中途应连续放大，不能提前跳到全屏"
    );
    assert_eq!(
        workspace_content_rect(full, full.size, 0.0),
        full,
        "外壳淡出后完整占用画布，不能在最后一帧移除标题空白造成跳动"
    );
}
