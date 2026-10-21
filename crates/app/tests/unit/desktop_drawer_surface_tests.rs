use super::*;
use gpui::{size, AppContext, Focusable, Modifiers, TestAppContext};

/// 从启动页进入桌面时，已有 Cmd+K 输入应继续服务左栏和底部原 Dock。
///
/// 数据构造：隔离空工作空间；先在 Launchpad 调用真实 Cmd+K 聚焦入口，留下非侧栏来源。
/// 执行过程：进入工作空间，再直接把键盘焦点移到复用的左栏 InputState 并重绘。
/// 预期结果：左栏对话和底部原输入同时可见，复用已有命令状态。
#[test]
fn launchpad_command_input_reused_by_left_drawer_keeps_center_navigation() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Input Reuse")
        .expect("应创建工作空间");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.focus_command_input(window, cx);
        assert!(
            !app.command_dock
                .as_ref()
                .expect("Cmd+K 应创建输入")
                .sidebar_origin,
            "启动页 Cmd+K 必须保留原有非侧栏来源"
        );
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.command_dock
            .as_ref()
            .expect("左栏应复用已有输入")
            .input
            .focus_handle(cx)
            .focus(window);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("workspace-command-dock").is_some(),
        "进入工作空间后底部应保留原 Command Dock"
    );
    assert!(
        cx.debug_bounds("command-dock-input").is_some(),
        "键盘焦点到左栏时底部原输入仍应可见"
    );
}

/// 双侧抽屉空态应呈现原型的对话输入与透明内容画布。
///
/// 数据构造：隔离数据库中创建一个空桌面工作空间；1280×800 窗口同时展开左右抽屉，
/// 不创建任何内容标签，因此右侧只应绘制空态。
/// 执行过程：打开工作空间、结束镜头动画并绘制真实 GPUI 树。
/// 预期结果：左侧有可输入的对话框；右侧空态存在且地址栏不占据空画布。
#[test]
fn drawer_empty_surface_matches_conversation_and_content_layout() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Drawer Surface")
        .expect("应创建工作空间");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleLeft, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleRight, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-assistant-chat-input").is_some(),
        "左抽屉应有可输入的 Norma 对话框"
    );
    assert!(
        cx.debug_bounds("desktop-assistant-brand").is_some(),
        "左栏标题应显示独立的产品标识与 Norma 名称"
    );
    assert!(
        cx.debug_bounds("desktop-content-home").is_some(),
        "右抽屉无标签时应显示网页输入与文件入口主页"
    );
    assert!(
        cx.debug_bounds("desktop-assistant-left-frost").is_some(),
        "左抽屉应绘制与壁纸对齐的毛玻璃底层"
    );
    assert!(
        cx.debug_bounds("desktop-assistant-right-frost").is_some(),
        "右抽屉应绘制与壁纸对齐的毛玻璃底层"
    );
    assert!(
        cx.debug_bounds("desktop-content-address-bar").is_none(),
        "右抽屉无标签时不应有常驻地址栏挤占画布"
    );
    // 场景：双栏拖宽后底部原 Dock 和中间画布使用相同的水平边界。
    // 数据构造：1280px 窗口，左栏 420px、右栏 520px，中央宽 340px。
    // 执行过程：更新抽屉宽度并重绘宿主，再读取真实 GPUI 边界。
    // 预期结果：画布和 Dock 都从 x=420 开始，到 x=760 结束。
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.drawer_widths.left = Some(420.0);
        app.desktop_workspace.drawer_widths.right = Some(520.0);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let canvas = cx
        .debug_bounds("desktop-assistant-canvas-region")
        .expect("应找到中央画布区域");
    let dock = cx
        .debug_bounds("desktop-assistant-dock-region")
        .expect("应找到原 Dock 区域");
    assert_eq!(canvas.left(), px(420.0), "画布左边缘应跟随左栏宽度");
    assert_eq!(canvas.right(), px(760.0), "画布右边缘应避开右栏");
    assert_eq!(dock.left(), canvas.left(), "底部 Dock 与画布左边缘一致");
    assert_eq!(dock.right(), canvas.right(), "底部 Dock 与画布右边缘一致");
    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.drawer_widths.left = None;
        app.desktop_workspace.drawer_widths.right = None;
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleRight, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let reader_trigger = cx
        .debug_bounds("desktop-open-right-drawer")
        .expect("收起的右栏应保留图标入口");
    assert_eq!(
        reader_trigger.size.width,
        px(32.0),
        "右栏入口应只容纳图标，不显示文字标签"
    );
    // 抽屉本身继续滑动，中央布局在点击展开后立即采用目标宽度，
    // 与原型的固定画布边界一致，避免卡片先缩窄再等 220ms 才有右侧边界。
    host.update_in(cx, |app, window, _| {
        app.motion.reduce_motion = false;
        app.desktop_workspace.drawer_motion =
            super::motion::DesktopAssistantDrawerMotion::default();
        app.desktop_workspace.drawer_motion.settle_left(true);
        app.desktop_workspace.drawers.right_open = true;
        app.desktop_workspace.drawer_motion.retarget(true, true);
        let layout = app.desktop_assistant_layout(window);
        assert!(
            (layout.canvas_width - 409.6).abs() < 0.5,
            "抽屉开启动画首帧中央就应采用 1280-396.8-473.6=409.6px 目标宽度；实际 {}px",
            layout.canvas_width
        );
    });
}

/// 场景：原型中左侧 Norma 光环本身可点击启动同一语音入口。
/// 数据构造：隔离的 1280×800 空工作空间，未配置语音能力，光环中心约为 (83, 213)。
/// 执行过程：先确认设置未打开，再点击光环中心并绘制下一帧。
/// 预期结果：沿用现有语音入口的未就绪处理，打开语音设置；点击前不能已有设置遮挡。
#[test]
fn assistant_orb_activates_existing_voice_entry() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Orb Voice")
        .expect("应创建工作空间");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    let host = host.expect("应创建宿主");
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleLeft, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("settings-overlay").is_none(),
        "点击光环前不应已有设置面板"
    );
    let orb = cx
        .debug_bounds("desktop-assistant-orb")
        .expect("左侧应绘制可点击的 Norma 光环");
    let ring = cx
        .debug_bounds("desktop-assistant-orb-art")
        .expect("光环应有独立的渐变柔光图形，而非三层按钮边框");
    assert_eq!(ring.size.width, orb.size.width, "渐变光环应铺满点击区域");
    assert_eq!(ring.size.height, orb.size.height, "渐变光环应保持圆形比例");
    host.update(cx, |app, _| {
        assert_ne!(
            app.voice_settings.readiness(),
            crate::settings::voice::VoiceEntryState::Ready,
            "隔离数据必须处于待配置状态，才能验证语音设置入口"
        );
    });
    cx.simulate_click(orb.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    host.update(cx, |app, _| {
        assert!(app.settings_panel.open, "光环点击必须使语音设置状态打开");
        assert_eq!(
            app.settings_panel.active_section,
            crate::settings::panel::SettingsSection::NormaVoice,
            "光环点击必须直达语音设置，不应打开普通设置首页"
        );
    });
}

/// 双侧抽屉同时展开时，中央画布的卡片必须能按单列完整摆放。
///
/// 数据构造：1280px 窗口的中央画布宽约 500px；只展开左侧时中央宽约 973px。
/// 执行过程：分别取两种宽度下的嵌入项目布局策略。
/// 预期结果：窄画布每行一张，宽画布保留项目原有的每行三张设置。
#[test]
fn embedded_project_uses_one_column_between_two_drawers() {
    let strategy = crate::canvas::layout_strategy::CanvasLayoutStrategy::default();
    assert_eq!(
        crate::canvas::layout_strategy::embedded_project_layout_strategy(strategy, 472.0)
            .track_count,
        1,
        "双侧抽屉之间应采用单列，避免 Agent 卡片被右侧抽屉裁切"
    );
    assert_eq!(
        crate::canvas::layout_strategy::embedded_project_layout_strategy(strategy, 945.0)
            .track_count,
        3,
        "宽画布应保留原项目的卡片列数"
    );
}

/// 场景：旧项目以宽画布保存了两张终端，其中第二张位于双抽屉的可视范围外。
/// 数据构造：终端分别在 x=20 与 x=850，另有一个会在桌面隐藏的 Rect 节点排在前面；
/// 双抽屉的逻辑画布宽 472px，项目场景本身已在两边留 14px。
/// 执行过程：计算嵌入画布的展示位置并再次读取持久文档快照。
/// 预期结果：两张终端按单列完整落在可视区，隐藏节点不占格，原坐标完全不变。
#[test]
fn embedded_drawer_presentation_keeps_saved_nodes_and_skips_hidden_cards() {
    use canvas::{CanvasNodeKind, CanvasRect, Document, RectNode, TermSpec};

    let mut document = Document::new();
    let hidden = document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x123456 }),
        CanvasRect::new(0.0, 0.0, 300.0, 300.0),
    );
    let first = document.add_node(
        CanvasNodeKind::Term(TermSpec),
        CanvasRect::new(20.0, 20.0, 350.0, 280.0),
    );
    let second = document.add_node(
        CanvasNodeKind::Term(TermSpec),
        CanvasRect::new(850.0, 20.0, 350.0, 280.0),
    );
    let saved = document.to_snapshot();
    let visible = crate::app::embedded_canvas::embedded_presentation_layout(
        document.nodes(),
        CanvasRect::new(0.0, 0.0, 472.0, 710.0),
        1.0,
        crate::canvas::layout_strategy::CanvasLayoutStrategy::default(),
    );

    assert!(
        !visible.contains_key(&hidden),
        "隐藏的旧卡片不得占用展示网格"
    );
    let first_bounds = visible.get(&first).expect("首张终端必须可见");
    let second_bounds = visible.get(&second).expect("第二张终端必须可见");
    assert_eq!(
        first_bounds.origin.x, 0.0,
        "不应在项目场景留白之内再套一层留白"
    );
    assert_eq!(first_bounds.size.x, 472.0, "卡片应占满项目场景的可用宽度");
    assert!(
        first_bounds.origin.x + first_bounds.size.x <= 472.0,
        "首张卡片不能被右抽屉裁切"
    );
    assert!(
        second_bounds.origin.x + second_bounds.size.x <= 472.0,
        "旧宽画布卡片必须完整进入窄画布"
    );
    assert!(
        second_bounds.origin.y > first_bounds.origin.y,
        "可见卡片应连续单列排列"
    );
    assert_eq!(
        document.to_snapshot(),
        saved,
        "抽屉适配不得改写持久画布坐标"
    );
}

/// 场景：已保存的宽画布节点在桌面项目中切换双侧抽屉并返回；之后在窄画布删除非 master。
/// 数据构造：1280px 宿主、一个真实项目，两个旧终端节点分别位于 x=20 和 x=850。
/// 执行过程：挂接项目后同步 472px 窄画布，再同步 945px 宽画布；回到窄画布后以第二张为 master 并删除第一张。
/// 预期结果：抽屉切换只改展示层，不改文档快照、不新建 PTY。删除则重排保存坐标到稳定区 master，窄画布展示仍按抽屉宽度适配。
#[test]
fn embedded_drawer_resize_changes_only_presentation_not_saved_canvas() {
    use canvas::{CanvasNodeKind, CanvasRect, TermSpec, Vec2};

    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_path = temp.path().join("project");
    std::fs::create_dir(&project_path).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Saved Layout").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_path)
        .expect("应注册项目");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(area.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    let project = host.read_with(cx, |app, _| {
        app.expanded_desktop_project().expect("项目应展开")
    });
    project.update_in(cx, |app, window, _| {
        app.runtime
            .spawn_terminal(CanvasRect::new(20.0, 20.0, 350.0, 280.0), "/bin/sh", 80, 24)
            .expect("应创建真实终端以验证抽屉缩放后的 PTY 网格");
        app.runtime.document.add_node(
            CanvasNodeKind::Term(TermSpec),
            CanvasRect::new(850.0, 20.0, 350.0, 280.0),
        );
        app.sync_desktop_embedded_canvas_size(Vec2::new(472.0, 710.0), window);
    });
    let (saved, terminal_count) = project.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_presentation_bounds.len(),
            2,
            "旧节点在窄画布应进入展示层"
        );
        let first = app.runtime.document.nodes()[0].id;
        let shown = app.desktop_presentation_bounds[&first];
        let expected = crate::cards::term::card::term_resize(shown, 1.0);
        let runtime::NodeView::Term { snapshot, .. } = app
            .runtime
            .node_view(first)
            .expect("首张终端应保留真实 PTY");
        let runtime::NodeResize::TermGrid { cols, rows } = expected;
        assert_eq!(
            snapshot.cols, cols as usize,
            "窄画布 PTY 列数应与展示宽度一致"
        );
        assert_eq!(
            snapshot.rows.len(),
            rows as usize,
            "窄画布 PTY 行数应与展示高度一致"
        );
        (
            app.runtime.document.to_snapshot(),
            app.runtime.running_terminal_count(),
        )
    });
    project.update(cx, |app, _| {
        let first = app.runtime.document.nodes()[0].id;
        let shown = app.desktop_presentation_bounds[&first];
        let visual =
            app.motion
                .resolve_node_visual_at(first, shown, app.theme, std::time::Instant::now());
        assert_eq!(
            visual.rect, shown,
            "抽屉重排时绘制位置必须和点击命中位置同步"
        );
    });
    project.update_in(cx, |app, window, _| {
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
    });
    project.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.document.to_snapshot(),
            saved,
            "抽屉尺寸切换不能持久改写节点位置"
        );
        assert_eq!(
            app.runtime.running_terminal_count(),
            terminal_count,
            "抽屉尺寸切换不能重建 PTY"
        );
        let first = app.runtime.document.nodes()[0].id;
        let shown = app
            .presented_node_bounds(first)
            .expect("宽画布仍须展示终端");
        let runtime::NodeResize::TermGrid { cols, .. } =
            crate::cards::term::card::term_resize(shown, 1.0);
        let runtime::NodeView::Term { snapshot, .. } =
            app.runtime.node_view(first).expect("终端会话应延续");
        assert_eq!(
            snapshot.cols, cols as usize,
            "恢复宽画布时 PTY 也应恢复列数"
        );
    });
    project.update_in(cx, |app, window, _| {
        app.sync_desktop_embedded_canvas_size(Vec2::new(472.0, 710.0), window);
        let first = app.runtime.document.nodes()[0].id;
        let second = app.runtime.document.nodes()[1].id;
        let original_bounds = app
            .runtime
            .document
            .nodes()
            .iter()
            .map(|node| storage::LayoutOriginalBounds {
                node_id: node.id,
                bounds: node.bounds,
            })
            .collect();
        assert!(
            app.master_layout.restore_from_snapshot(
                storage::LayoutStateSnapshot {
                    mode: storage::LayoutMode::MasterDetail,
                    master_id: Some(second),
                    original_bounds,
                },
                &app.runtime,
            ),
            "应在关闭非主卡片前建立 master 布局"
        );
        assert!(
            app.remove_node_and_reflow(first, window),
            "应关闭首张项目卡片"
        );
    });
    project.read_with(cx, |app, _| {
        assert_eq!(app.runtime.document.nodes().len(), 1, "只删除目标卡片");
        let zoom = app.runtime.document.viewport.zoom;
        let saved_after = app.runtime.document.nodes()[0].bounds;
        assert_ne!(
            saved_after, saved.nodes[1].bounds,
            "删除非 master 后必须重排保存坐标，不能把被删卡片旁边的旧格子留着"
        );
        assert!(
            saved_after.size.x * zoom > 600.0,
            "重排必须写稳定区的 master 宽度，不能把 472px 抽屉画布写入保存坐标：got {}",
            saved_after.size.x * zoom
        );
        let shown = app
            .presented_node_bounds(app.runtime.document.nodes()[0].id)
            .expect("窄画布仍须展示剩余卡片");
        assert!(
            shown.size.x * zoom <= 480.0,
            "窄画布展示仍按抽屉宽度适配，不得把稳定区宽度直接画出来：shown={}",
            shown.size.x * zoom
        );
        assert_eq!(
            app.desktop_presentation_bounds.len(),
            1,
            "展示层应去掉已关闭的卡片"
        );
        assert_eq!(
            app.master_layout.master_id(),
            Some(app.runtime.document.nodes()[0].id),
            "关闭非主卡片应保留现有 master 布局"
        );
        assert_eq!(
            app.master_layout.to_snapshot().original_bounds.len(),
            1,
            "关闭的卡片不应残留在 master 原始坐标中"
        );
    });
}
