use crate::app::{execute_high_level_agent_command, MultisoulApp};
use crate::motion::{DispatchEffectPhase, DispatchVisualOrigin};
use gpui::{point, px, size, AppContext, TestAppContext};
use norma::protocol::{AgentProfile, AgentTaskAssignment, CardCommand, CardCommandResult};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// Workspace 中已接受的任务必须在窗口层绘制流星，不能被项目画布的隐藏 Chrome 条件过滤。
///
/// 数据构造（含关键数值的推导过程）：
///   Workspace = 1；项目 = 1；Agent = 1（Alan/Codex）；成功回执 = 1。
///   窗口 = 1280×800；项目原点 = (14, 52+12) = (14,64)。
///   Dock 中心 = (1280/2, 800-22-52/2) = (640,752)，在项目画布外。
///   特效覆盖区域必须为窗口 (0,0,1280,800)，而非项目 (14,64,1252,645)。
///   单帧上限 50ms；19×50ms = 950ms > Flight(480ms) + Impact(430ms) = 910ms。
///   Reduce Motion 收束：4×50ms = 200ms > ReducedPulse(150ms)。
///   隐藏项目收到新回执后发出 1 次通知，宿主流星重绘通知应为 0 次。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用隔离数据库加载并展开真实 Workspace 项目，挂接生产宿主观察器。
///   2. 用 Echo PTY 创建 Alan，从 Norma 生产入口派发任务并通知项目更新。
///   3. 绘制整个宿主窗口，检查实际渲染树中的流星覆盖层与真实回执目标。
///   4. 推进特效至结束，只通知项目，检查宿主移除已结束的覆盖层。
///   5. 开启 Reduce Motion 后派发新任务，检查 Impact 反馈仍绘制且没有 Flight。
///   6. 返回工作空间目录，后台派发并统计宿主通知；再检查 Settings 打开与关闭通知。
///
/// 预期结果：
///   - 正断言：唯一成功回执创建唯一流星，窗口层覆盖 Dock 并命中 Alan。
///   - 负断言：覆盖层不能局限于项目画布；完成后不能残留旧特效。
///   - 正断言：Reduce Motion 下保留命中反馈；负断言：不再跨屏飞行。
///   - 负断言：后台流星不通知宿主；正断言：Settings 仍通知宿主，关闭时有最后一帧。
#[test]
fn desktop_workspace_dispatch_receipt_is_painted_over_shared_dock() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离数据库");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Dispatch").expect("应创建 Workspace");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主窗口");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let project = host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        app.expand_desktop_project_area(area.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.expanded_desktop_project().expect("应展开项目")
    });
    project.update_in(cx, |app, window, cx| {
        app.runtime.set_pty_factory(Box::new(|_, _, _, tx| {
            Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
        }));
        let visible = app.visible_canvas_rect(window);
        let zoom = app.runtime.document.viewport.zoom;
        execute_high_level_agent_command(
            app,
            CardCommand::CreateAgent {
                name: Some("Alan".to_string()),
                profile: AgentProfile::Codex,
                activate: true,
            },
            visible,
            zoom,
            agent::AgentDispatchSource::norma(),
            None,
        )
        .expect("应创建 Alan");
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let dispatch = |text: &str, cx: &mut gpui::VisualTestContext| {
        project.update_in(cx, |app, window, cx| {
            let visible = app.visible_canvas_rect(window);
            let zoom = app.runtime.document.viewport.zoom;
            let result = execute_high_level_agent_command(
                app,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: text.to_string(),
                    }],
                    force_untracked: false,
                },
                visible,
                zoom,
                agent::AgentDispatchSource::norma(),
                None,
            )
            .expect("应接受 Norma 派发");
            let CardCommandResult::AgentTasksDispatched { receipts } = result else {
                panic!("派发应返回真实任务回执");
            };
            assert_eq!(receipts.len(), 1, "唯一指派必须返回唯一回执");
            let effects = app.motion.dispatch_receipt_presentations();
            assert_eq!(effects.len(), 1, "唯一成功回执必须创建唯一流星状态");
            assert_eq!(
                effects[0].target_node_id, receipts[0].node_id,
                "流星必须命中 Alan"
            );
            assert_eq!(
                effects[0].origin,
                DispatchVisualOrigin::CommandDock,
                "Norma 派发必须从 Dock 起飞"
            );
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
    };
    dispatch("验证 Workspace 流星", cx);
    let overlay = cx
        .debug_bounds("dispatch-receipt-effect-overlay")
        .expect("Workspace 必须实际绘制已接受任务的流星，不能仅创建状态");
    assert_eq!(
        overlay.origin,
        point(px(0.0), px(0.0)),
        "流星层必须从窗口原点开始"
    );
    assert_eq!(
        overlay.size,
        size(px(1280.0), px(800.0)),
        "流星层必须覆盖共享 Dock"
    );
    assert_ne!(
        overlay.origin,
        point(px(14.0), px(64.0)),
        "流星不能被项目 Scene 偏移和裁剪"
    );
    project.update(cx, |app, cx| {
        // tick 单帧截断至 50ms；19×50=950ms > 480+430=910ms，确保特效结束。
        for _ in 0..19 {
            app.motion.tick(Duration::from_millis(50));
        }
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("dispatch-receipt-effect-overlay").is_none(),
        "特效结束后宿主必须移除旧覆盖层"
    );
    project.update(cx, |app, _| app.motion.reduce_motion = true);
    dispatch("验证减少动态效果反馈", cx);
    project.update(cx, |app, _| {
        let effects = app.motion.dispatch_receipt_presentations();
        assert_eq!(
            effects[0].phase,
            DispatchEffectPhase::Impact,
            "Reduce Motion 必须保留命中反馈"
        );
        assert_ne!(
            effects[0].phase,
            DispatchEffectPhase::Flight,
            "Reduce Motion 不得跨屏飞行"
        );
    });
    assert!(
        cx.debug_bounds("dispatch-receipt-effect-overlay").is_some(),
        "Reduce Motion 反馈也必须实际绘制"
    );
    host.update_in(cx, |app, window, cx| {
        app.navigate_desktop_shortcut(
            crate::shortcuts::ShortcutCommandId::ShowWorkspaceDirectory,
            window,
            cx,
        );
        app.finish_desktop_zoom(window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    project.update(cx, |app, cx| {
        assert!(app.background_project, "返回目录后项目必须进入后台");
        // 每帧最多 50ms；4×50=200ms > ReducedPulse(150ms)，清除上一条回执。
        for _ in 0..4 {
            app.motion.tick(Duration::from_millis(50));
        }
        cx.notify();
    });
    cx.run_until_parked();
    let host_notifications = Rc::new(Cell::new(0));
    let observed_notifications = host_notifications.clone();
    let _observer = cx.update(|_, cx| {
        cx.observe(&host, move |_, _| {
            observed_notifications.set(observed_notifications.get() + 1);
        })
    });
    dispatch("验证后台派发不重绘当前窗口", cx);
    assert_eq!(
        host_notifications.get(),
        0,
        "隐藏项目的流星不可触发当前宿主重绘"
    );
    assert!(
        cx.debug_bounds("dispatch-receipt-effect-overlay").is_none(),
        "隐藏项目的流星不可出现在目录中"
    );
    project.update(cx, |app, cx| {
        app.settings_panel.open = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(host_notifications.get(), 1, "Settings 打开必须继续通知宿主");
    project.update(cx, |app, cx| {
        app.settings_panel.open = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        host_notifications.get(),
        2,
        "Settings 关闭必须通知最后一帧，后台流星不能干扰该语义"
    );
    project.update(cx, |app, _| app.runtime.kill_all());
}
