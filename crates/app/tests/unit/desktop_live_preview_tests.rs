use super::*;
use gpui::{size, Entity, Modifiers, Render, TestAppContext, Window};

struct PreviewHost(Entity<MultisoulApp>);

impl Render for PreviewHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}

/// 项目缩略图、展开画布和目录中的二级缩略图复用真实卡片，所有 chrome 等比缩小。
///
/// 数据构造：1280×800 窗口，项目逻辑画布为 (1280-28)×(800-127)=1252×673；
///   两个项目（一个空画布）、一张真实 /bin/sh 终端，初始网格80×24；原标题32px、关闭按钮18px。
///   几何容差1px来自 GPUI 布局取整；整个场景只变换显示，不能改变网格或重建会话。
/// 执行过程：进入项目总览 → 取得真实标题/关闭按钮 → 展开项目 → 比较同一比例 → 返回总览
///   → 点击缩小的终端正文 → 验证真实焦点且未展开 → 重放缓存帧后键入 printf 并回车
///   → 检查独立输出行（不能只匹配命令回显）→ 进入目录检查缩略 → 点击进入工作空间。
/// 预期结果：三层均含真实 card-titlebar/node-close；大小比一致；没有旧 Message Agent 输入；
///   缩略时命中原卡片，目录中的卡片不落入相邻项目；目录点击只打开工作空间，原 node/session 身份保留。
#[test]
fn desktop_live_preview_scales_the_original_canvas_and_routes_input() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Preview").expect("应创建空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let other_root = temp.path().join("other-project");
    std::fs::create_dir(&other_root).expect("应创建第二项目");
    catalog
        .add_project(&workspace.id, &other_root)
        .expect("应添加另一个真实画布以验证多视图输入");
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        PreviewHost(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.workspaces = catalog.list().expect("应读取空间");
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let session = host.update(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace.id.clone(), area.id.clone())].clone()
    });
    let node = session.update(cx, |app, cx| {
        app.motion.reduce_motion = true;
        let id = app
            .runtime
            .spawn_terminal(CanvasRect::new(0.0, 0.0, 640.0, 400.0), "/bin/sh", 80, 24)
            .expect("应创建真实终端");
        cx.notify();
        id
    });
    host.update(cx, |_, cx| cx.notify());
    cx.update(|window, cx| window.draw(cx).clear());
    let small_title = cx
        .debug_bounds("card-titlebar")
        .expect("项目缩略必须保留真实卡片标题栏");
    let small_close = cx
        .debug_bounds("node-close")
        .expect("项目缩略必须保留原始关闭按钮");
    assert!(
        cx.debug_bounds("desktop-agent-input-0-0").is_none(),
        "不能添加独立 Message Agent 输入框"
    );
    let before = session.read_with(cx, |app, _| app.runtime.document.to_snapshot());
    let grid_before = session.read_with(cx, |app, _| {
        let runtime::NodeView::Term { snapshot, .. } =
            app.runtime.node_view(node).expect("应有终端快照");
        (snapshot.cols, snapshot.lines)
    });
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = false;
        app.expand_desktop_project_area(area.id.clone(), window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let first_frame = cx
        .debug_bounds("card-titlebar")
        .expect("转场起点应有真实标题");
    assert!(
        (f32::from(first_frame.origin.x - small_title.origin.x)).abs() < 1.0
            && (f32::from(first_frame.origin.y - small_title.origin.y)).abs() < 1.0,
        "动画起点必须与缩略图重合，不能跳到节点fit结果：{first_frame:?} vs {small_title:?}"
    );
    host.update(cx, |app, cx| {
        // 5×16ms=80ms，小于弹簧收敛时间，检查稳定内容缓存被整体变换。
        let transition = app
            .desktop_workspace
            .transition
            .as_mut()
            .expect("应有展开动画");
        for _ in 0..5 {
            transition.tick(std::time::Duration::from_millis(16));
        }
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let midway = cx
        .debug_bounds("card-titlebar")
        .expect("动画中仍应有真实标题");
    assert!(
        midway.size.width > first_frame.size.width,
        "缓存画布必须随镜头放大"
    );
    session.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.document.viewport, before.viewport,
            "动画不能改变真实viewport"
        );
        let runtime::NodeView::Term { snapshot, .. } =
            app.runtime.node_view(node).expect("应保留终端");
        assert_eq!(
            (snapshot.cols, snapshot.lines),
            grid_before,
            "镜头动画不能resize PTY"
        );
    });
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        app.motion.reduce_motion = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let full_title = cx
        .debug_bounds("card-titlebar")
        .expect("展开应保留同一标题栏");
    let full_close = cx.debug_bounds("node-close").expect("展开应保留同一按钮");
    let scale = small_title.size.width / full_title.size.width;
    assert!(scale > 0.0 && scale < 1.0, "缩略比例必须小于展开态");
    assert!(
        (f32::from(small_title.size.height - full_title.size.height * scale)).abs() < 1.0,
        "标题高度应与画布等比，不能钳制成另一套字号/高度"
    );
    assert!(
        (f32::from(small_close.size.width - full_close.size.width * scale)).abs() < 1.0,
        "关闭按钮也必须等比，不能沿用缩略图独立 chrome"
    );
    session.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.document.nodes()[0].bounds,
            before.nodes[0].bounds,
            "展开不能重新排列原节点"
        );
        assert_eq!(
            app.runtime.running_terminal_count(),
            1,
            "展开不能重建或丢失终端"
        );
    });
    host.update_in(cx, |app, window, cx| {
        app.collapse_desktop_project_area(window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let title = cx
        .debug_bounds("card-titlebar")
        .expect("返回总览仍绘制真实卡片");
    assert_eq!(
        title, small_title,
        "返回必须落在原始缩略位置，不在末帧更换卡片"
    );
    cx.simulate_click(
        gpui::point(title.center().x, title.bottom() + px(25.0)),
        Modifiers::none(),
    );
    session.update_in(cx, |app, window, _| {
        assert!(
            app.focus_handle.is_focused(window),
            "真实卡片必须持有键盘/IME焦点，不能被项目外壳抢走"
        )
    });
    session.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.focus.mode(),
            runtime::InputMode::NodeFocused(node),
            "缩略正文应通过逆变换命中原终端"
        )
    });
    cx.update(|window, cx| window.draw(cx).clear());
    // 重放三次缓存帧，覆盖真实窗口中点击后持续刷新的输入处理器。
    for _ in 0..3 {
        host.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| window.draw(cx).clear());
    }
    cx.simulate_input("printf 'LIVE_PREVIEW_OK\\n'");
    cx.simulate_keystrokes("enter");
    let mut text = String::new();
    // 100×10ms=1秒，只等待本地shell的短printf，不等待模型或网络。
    for _ in 0..100 {
        session.update(cx, |app, _| {
            app.runtime.drain_all();
            let runtime::NodeView::Term { snapshot, .. } =
                app.runtime.node_view(node).expect("原终端应仍存在");
            text = snapshot
                .rows
                .iter()
                .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
        });
        if text.lines().any(|line| line.trim() == "LIVE_PREVIEW_OK") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        text.lines().any(|line| line.trim() == "LIVE_PREVIEW_OK"),
        "总览输入必须经原窗口IME/键盘路由到真实终端：{text}"
    );
    host.update(cx, |app, _| {
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "操作总览卡片不能被项目外壳强行展开"
        )
    });
    host.update_in(cx, |app, window, cx| app.show_desktop_launchpad(window, cx));
    cx.run_until_parked();
    host.update_in(cx, |app, window, _| {
        assert!(
            app.focus_handle.is_focused(window),
            "进入只读目录必须移走真实终端焦点，防止键入隐藏卡片"
        )
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let directory_title = cx
        .debug_bounds("card-titlebar")
        .expect("目录也必须复用真实卡片");
    let first_preview = cx
        .debug_bounds("desktop-workspace-preview-0-0")
        .expect("目录应保留第一个项目外壳");
    assert!(first_preview.contains(&directory_title.center()), "第一个项目的真实卡片必须位于自身缩略壳内，不能复用相邻项目的镜头：{directory_title:?} / {first_preview:?}");
    assert!(
        directory_title.size.width < small_title.size.width,
        "目录应整体再次缩小"
    );
    cx.simulate_click(directory_title.center(), Modifiers::none());
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.active_id.as_deref(),
            Some(workspace.id.as_str()),
            "目录点击应进入对应空间"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "目录不能越级进入项目"
        );
        assert_eq!(
            app.desktop_workspace.sessions[&(workspace.id.clone(), area.id.clone())].entity_id(),
            session.entity_id(),
            "缩略与展开必须保持同一会话"
        );
    });
    session.update(cx, |app, _| app.runtime.kill_all());
}
