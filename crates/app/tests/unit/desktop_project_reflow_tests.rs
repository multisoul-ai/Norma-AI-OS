use super::*;

/// 新增第五项目时从当前两列位置重排，反向删除保留真实中间帧。
///
/// 数据构造：1280×800，顶部52+12=64，底部91；4项每格高(800-64-91-24)/2=310.5，宽552；
///   左右24、列距16、行距24；5项宽(1232-2×16)/3=400；
///   先推进100ms，小于原型330ms，再移除中间B，剩余A/C/D/E。
/// 执行过程：首次同步4项 → 增加E → 推進100ms → 移除B → 完成330ms。
/// 预期结果：不直接跳目标、不按新索引借错起点；新项目真实存在、B不再占位，停止请求动画。
#[test]
fn project_reflow_is_interruptible_and_keeps_project_identity() {
    let ids = |names: &[&str]| {
        names
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>()
    };
    let size = Vec2::new(1280.0, 800.0);
    let mut layout = ProjectLayout::default();
    layout.sync("workspace", size, &ids(&["A", "B", "C", "D"]), true);
    let initial = layout.current.clone();
    assert!(!layout.is_active(), "首次打开没有虚构入场动画");
    layout.sync("workspace", size, &ids(&["A", "B", "C", "D", "E"]), true);
    assert_eq!(layout.current[2], initial[2], "新增E不能让C瞬移到新列");
    assert!(layout.is_active(), "跨列阈值应开始重排");
    layout.tick(Duration::from_millis(100));
    let c_midway = layout.current[2];
    assert_ne!(c_midway, initial[2], "100ms时C应已向目标移动");
    assert_ne!(c_midway, layout.target[2], "100ms尚未达到330ms目标");
    layout.sync("workspace", size, &ids(&["A", "C", "D", "E"]), true);
    assert_eq!(
        layout.current[1], c_midway,
        "删除B后C应从自己的实际位置继续"
    );
    layout.tick(Duration::from_millis(330));
    assert_eq!(
        layout.ids,
        ids(&["A", "C", "D", "E"]),
        "删除B后保留其余相对顺序"
    );
    assert!(
        !layout.ids.contains(&"B".to_string()),
        "已删除B不能作为占位保留"
    );
    assert_eq!(
        layout.current[1],
        CanvasRect::new(648.0, 64.0, 552.0, 310.5),
        "C最终成为右上角项目"
    );
    assert_eq!(
        layout.current[3],
        CanvasRect::new(648.0, 398.5, 552.0, 310.5),
        "E最终填右下角"
    );
    assert!(!layout.is_active(), "330ms后不再重绘静止布局");
}

/// 仅项目集合或窗口变化会重排；减少动态效果直接落到新位置。
///
/// 数据构造：3个项目、1280×800；缩窄到600×800后，两侧24、列距16，格宽(552-16)/2=268，高150.75。
/// 执行过程：反复提交相同项目输入，再以减少动态效果提交窄窗口。
/// 预期结果：重复同步返回false；窄窗同步返回true，但不产生动画，也不变更项目顺序。
#[test]
fn project_layout_ignores_content_changes_and_respects_reduced_motion() {
    let mut layout = ProjectLayout::default();
    let ids = vec!["A".into(), "B".into(), "C".into()];
    let size = Vec2::new(1280.0, 800.0);
    assert!(layout.sync("workspace", size, &ids, true), "首帧应计算布局");
    assert!(
        !layout.sync("workspace", size, &ids, true),
        "Agent输出和重绘不能重算同一项目网格"
    );
    assert!(
        layout.sync("workspace", Vec2::new(600.0, 800.0), &ids, false),
        "窗口变化应更新几何"
    );
    assert_eq!(
        layout.current[1],
        CanvasRect::new(308.0, 223.75, 268.0, 150.75),
        "窄窗保持上排两列"
    );
    assert_eq!(layout.ids, ids, "窗口变化不改变项目顺序");
    assert!(!layout.is_active(), "减少动态效果不启用空间移动");
}

struct ReflowHost(gpui::Entity<MultisoulApp>);
impl gpui::Render for ReflowHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        self.0.clone()
    }
}

/// 外层项目重排、展开与返回不改变内部 Agent 顺序、布局策略、viewport 或 PTY。
///
/// 数据构造：1280×800，A/B/C分别含0/1/7个Agent；C另有80×24的真实sh终端。
///   旧项目总览偏移-320px；添加D/E触发3→5项，100ms处于330ms重排中途。
///   100×16ms=1600ms足够镜头收敛；自定义7列配置与外层两行明显不同。
/// 执行过程：绘制3项 → 增加C内部Agent → 验证外层不变 → 添加D/E并推进100ms
///   → 从实际中间位置展开C → 返回 → 完成重排 → 比较真实会话数据。
/// 预期结果：旧滚动不影响项目起点，新增Agent不增项目槽位，B/C快照和PTY保持；
///   空A仍在左上，不新增假项目，展开起点不跳到最终格子。
#[test]
fn desktop_project_reflow_preserves_agents_and_camera_origin() {
    use canvas::{AgentProfile, AgentSpec, CanvasNodeKind};
    use gpui::{prelude::*, px, size, TestAppContext};
    let temp = tempfile::tempdir().expect("隔离临时数据");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Project layout").expect("创建测试Workspace");
    let add = |name: &str| {
        let path = temp.path().join(name);
        std::fs::create_dir(&path).expect("创建项目目录");
        catalog.add_project(&workspace.id, &path).expect("注册项目")
    };
    let a = add("A");
    let b = add("B");
    let c = add("C");
    catalog
        .set_overview_scroll_y(&workspace.id, -320)
        .expect("构造旧滚动偏移");
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        ReflowHost(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.workspaces = catalog.list().expect("读取目录");
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let (a_session, b_session, c_session) = host.read_with(cx, |app, _| {
        let get = |id: &String| {
            app.desktop_workspace.sessions[&(workspace.id.clone(), id.clone())].clone()
        };
        (get(&a.id), get(&b.id), get(&c.id))
    });
    let agent = || {
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
            definition_id: None,
            mobile_control: false,
        })
    };
    b_session.update(cx, |app, cx| {
        app.runtime
            .document
            .add_node(agent(), CanvasRect::new(42.0, 22.0, 300.0, 220.0));
        cx.notify();
    });
    let terminal = c_session.update(cx, |app, cx| {
        // 7列是用户既有Agent布局；相隔130px放置7个120px卡片，明确区别于项目两行。
        app.layout_strategy.track_count = 7;
        for index in 0..7 {
            app.runtime.document.add_node(
                agent(),
                CanvasRect::new(index as f32 * 130.0, 50.0, 120.0, 240.0),
            );
        }
        let terminal = app
            .runtime
            .spawn_terminal(CanvasRect::new(0.0, 320.0, 640.0, 280.0), "/bin/sh", 80, 24)
            .expect("启动真实终端");
        cx.notify();
        terminal
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let actions = cx
        .debug_bounds("desktop-project-actions-0")
        .expect("项目菜单应存在");
    assert_eq!(actions.size.width, px(20.0), "项目更多按钮应为20px");
    assert_eq!(
        actions.size.height,
        px(20.0),
        "项目更多按钮不能越过28px标题栏"
    );
    let initial = host.read_with(cx, |app, _| {
        app.desktop_workspace.project_layout.current.clone()
    });
    assert_eq!(initial.len(), 3, "0/1/7个Agent仍只分配三个项目区域");
    assert_eq!(
        initial[0],
        // 左栏默认收起，1280px 全宽中两列各 552px；两列加 16px 间距共 1120px，左右各 80px。
        // 可用高度 800-64-91=645px；两行各 310.5px 加 24px 间距恰好填满。
        CanvasRect::new(80.0, 64.0, 552.0, 310.5),
        "空A保留左上槽位，旧偏移不能使其离屏"
    );
    c_session.update(cx, |app, cx| {
        app.runtime
            .document
            .add_node(agent(), CanvasRect::new(930.0, 50.0, 120.0, 240.0));
        cx.notify();
    });
    host.update_in(cx, |app, window, cx| {
        app.sync_desktop_project_layout(window, cx)
    });
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.project_layout.current, initial,
            "新增Agent不得触发外层项目重排"
        )
    });
    let b_before = b_session.read_with(cx, |app, _| app.runtime.document.to_snapshot());
    let (c_before, strategy) = c_session.read_with(cx, |app, _| {
        (app.runtime.document.to_snapshot(), app.layout_strategy)
    });
    let grid_before = c_session.read_with(cx, |app, _| {
        let runtime::NodeView::Term { snapshot, .. } =
            app.runtime.node_view(terminal).expect("读取终端网格");
        (snapshot.cols, snapshot.lines)
    });
    add("D");
    add("E");
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = false;
        app.desktop_workspace.areas = catalog.list_projects(&workspace.id).expect("读取新增项目");
        app.sync_desktop_project_layout(window, cx);
        app.desktop_workspace
            .project_layout
            .tick(Duration::from_millis(100));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let drawn = cx
        .debug_bounds("desktop-project-area-2")
        .expect("中间帧C项目必须可见");
    host.update_in(cx, |app, window, cx| {
        let mut shell = super::super::camera::canvas_rect_from_bounds(drawn);
        let drawer = app.desktop_assistant_layout(window);
        let scale = window.scale_factor();
        shell.origin.x -= (drawer.left_space * scale).round() / scale;
        let expected = super::super::preview_canvas::project_canvas_rect(
            shell,
            super::super::desktop_scene_rect(drawer.canvas_width, 800.0).size,
            1.0,
        );
        app.expand_desktop_project_area(c.id.clone(), window, cx);
        let actual = app
            .desktop_workspace
            .transition
            .as_ref()
            .expect("展开应创建镜头")
            .rect();
        // GPUI在设备像素上取整，只容忍1逻辑px，与错误的目标槽位偏差区分。
        assert!(
            (actual.origin.x - expected.origin.x).abs() < 1.0
                && (actual.origin.y - expected.origin.y).abs() < 1.0,
            "展开必须从重排中真实C的位置开始"
        );
        for _ in 0..100 {
            app.desktop_workspace
                .transition
                .as_mut()
                .expect("镜头存在")
                .tick(Duration::from_millis(16));
        }
        app.finish_desktop_zoom(window, cx);
        app.collapse_desktop_project_area(window, cx);
        for _ in 0..100 {
            app.desktop_workspace
                .transition
                .as_mut()
                .expect("返回镜头存在")
                .tick(Duration::from_millis(16));
        }
        app.finish_desktop_zoom(window, cx);
        app.desktop_workspace
            .project_layout
            .tick(Duration::from_millis(330));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    b_session.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.document.to_snapshot(),
            b_before,
            "B内部Agent的顺序、bounds和viewport都必须保持"
        )
    });
    a_session.read_with(cx, |app, _| {
        assert!(
            app.runtime.document.nodes().is_empty(),
            "空A不能因重排自动生成Agent"
        )
    });
    c_session.update(cx, |app, _| {
        assert_eq!(
            app.runtime.document.to_snapshot(),
            c_before,
            "C内部卡片顺序、位置和viewport不得受外层网格影响"
        );
        assert_eq!(
            app.layout_strategy, strategy,
            "Agent的7列设置不被项目两行覆盖"
        );
        let runtime::NodeView::Term { snapshot, .. } =
            app.runtime.node_view(terminal).expect("原PTY仍存在");
        assert_eq!(
            (snapshot.cols, snapshot.lines),
            grid_before,
            "纯呈现重排与镜头不能resize PTY"
        );
        assert_eq!(
            app.runtime.running_terminal_count(),
            1,
            "应复用唯一终端进程"
        );
        app.runtime.kill_all();
    });
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.project_layout.ids.len(),
            5,
            "仅新增D/E两个项目槽位"
        );
        assert!(
            !app.desktop_workspace.project_layout.is_active(),
            "完成重排后停止请求帧"
        );
        assert_eq!(
            app.desktop_workspace.sessions[&(workspace.id.clone(), c.id.clone())].entity_id(),
            c_session.entity_id(),
            "全程复用同一C会话"
        );
    });
}

/// 重排中返回目录，镜头结束后继续调度剩余外层重排。
///
/// 数据构造：4→5项目重排推进100ms，剩余230ms；Workspace镜头刚结束且无待执行帧。
/// 执行过程：在Launchpad完成镜头 → 检查重新请求帧 → 推进剩余重排。
/// 预期结果：目录缩略不能卡在中间位置；结束镜头不将用户送回项目页，330ms后准确落定。
#[test]
fn desktop_project_reflow_resumes_after_returning_to_directory() {
    use super::super::camera::{DesktopCanvasSubject, DesktopCanvasTransition};
    use gpui::{AppContext as _, TestAppContext};
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("隔离存储");
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        ReflowHost(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    host.update_in(cx, |app, window, cx| {
        let layout = &mut app.desktop_workspace.project_layout;
        let ids = vec!["A".into(), "B".into(), "C".into(), "D".into()];
        layout.sync("space", Vec2::new(1280.0, 800.0), &ids, true);
        let mut more = ids;
        more.push("E".into());
        layout.sync("space", Vec2::new(1280.0, 800.0), &more, true);
        layout.tick(Duration::from_millis(100));
        app.screen = crate::app::AppScreen::Launchpad;
        let frame = CanvasRect::new(0.0, 0.0, 1280.0, 800.0);
        app.desktop_workspace.transition = Some(DesktopCanvasTransition::new(
            DesktopCanvasSubject::Workspace("space".into()),
            frame,
            frame,
        ));
        // 对应帧回调开始时已消费旧pending标记，镜头在该回调内结束。
        app.desktop_workspace.transition_frame_pending = false;
        app.finish_desktop_zoom(window, cx);
        assert!(
            app.desktop_workspace.transition_frame_pending,
            "目录不会重新render工作空间，结束镜头必须主动续约重排帧"
        );
        assert!(
            app.desktop_workspace.transition.is_none(),
            "镜头已经结束不能依靠新导航恢复"
        );
        assert_eq!(
            app.screen,
            crate::app::AppScreen::Launchpad,
            "结束不能切回项目页"
        );
        app.desktop_workspace
            .project_layout
            .tick(Duration::from_millis(230));
        assert!(
            !app.desktop_workspace.project_layout.is_active(),
            "剩余230ms后应到达5项目最终网格"
        );
    });
}
