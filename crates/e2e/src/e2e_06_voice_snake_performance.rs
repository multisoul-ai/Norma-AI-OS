use gpui::TestAppContext;
use runtime::{AgentTaskState, AgentTaskUpdate};
use std::time::Duration;

use crate::harness::{
    create_codex_agent, create_terminal, echo_factory, enter_canvas, make_app, read_runtime,
};

/// E2E-06：六卡场景的 Working Snake 必须按 60fps 局部重绘，且不触发根 Canvas 重绘。
///
/// 数据构造（含关键数值的推导过程）：
///   cards                   = 5 Terminal + 1 Codex Agent = 6
///   initialization settle   = 1_000ms > 卡片初始化总时长 992ms
///   focus settle            = 200ms > focus/selection 过渡 180ms
///   background settle       = 50 × 12ms = 600ms > persistence interval 120ms
///   display refresh         = 90Hz，frame interval = 11_111_112ns
///   measurement callbacks   = 90
///   measurement duration    = 90 × 11_111_112ns = 1_000_000_080ns ≈ 1s
///   expected snake renders  = 60（60fps accumulator 保留 90Hz 帧间余量）
///   expected Canvas renders = 0（Working Snake 不得失效根 CanvasSurfaceView）
///   unfocused callbacks     = 90（窗口失去键盘焦点但仍保持可见）
///   unfocused snake renders = 60（可见性而非键盘焦点决定是否续帧）
///   continuous phase        = (1s + 1s) / 1.2s mod 1 ≈ 0.667
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建五张 Terminal 和一张 Codex Agent，推进 1_000ms 排除初始化动画。
///   2. 注入 Thinking，退出 Agent 焦点以排除 Terminal cursor 的根级动画来源。
///   3. 推进 200ms 收束退出焦点产生的根级 chrome 过渡。
///   4. 暂停根与 Snake 测试循环，推进 600ms 排空此前的 Runtime/持久化通知。
///   5. 重置 Snake 逻辑时钟，分别清零根 Canvas 与目标 Snake Entity 的 render 计数。
///   6. 逐次推进 90 个 11_111_112ns 显示帧，每帧消费一次目标 Snake callback。
///   7. 读取卡片数、Snake 局部状态、根调度和根 Canvas render 计数。
///   8. 恢复真实局部 driver，确认已挂下一帧；再令窗口失去键盘焦点但保持可见。
///   9. 保留原始 phase 起点，只清零 render 计数，再消费 90 个 11_111_112ns 显示帧。
///   10. 恢复窗口焦点与真实局部 driver，确认下一帧仍能继续挂起。
///
/// 预期结果：
///   - 正断言：场景精确包含六张卡片，目标 Snake 保持 active 并请求下一显示帧。
///   - 正断言：90Hz 的 90 个 callback 中精确 60 个推进、通知并产生局部 render。
///   - 正断言：失焦但可见的 90 个 callback 仍精确产生 60 个局部 render。
///   - 正断言：约 1s 后 phase 位于 1s / 1.2s ≈ 0.833 附近，证明动画按真实时间推进。
///   - 正断言：再失焦运行约 1s 后 phase 连续到达 2s / 1.2s mod 1 ≈ 0.667。
///   - 负断言：根调度不得把 Snake 计入 Canvas motion，根 Canvas render 必须严格为 0。
///   - 负断言：键盘失焦不得停止可见 Snake，也不得触发根 Canvas 重绘或 phase 跳变。
#[test]
fn e2e_06_six_card_working_snake_runs_sixty_local_frames_without_root_canvas_repaint() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("六卡 Snake 性能测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    for _ in 0..5 {
        create_terminal(&app, cx);
    }
    let node_id = create_codex_agent(&app, cx);

    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(1_000), cx);
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "UserPromptSubmit".to_string(),
            to: AgentTaskState::Thinking,
            task_title: None,
            session_id: None,
        });
        assert!(changed, "六卡场景的 Thinking 更新应写入 Codex Agent");
        app.runtime.focus.exit_to_canvas();
        cx.notify();
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(200), cx)
    });
    cx.run_until_parked();

    app.update(cx, |app, cx| {
        app.reset_animation_display_callback_clock_for_e2e(cx);
        let reset = app.reset_working_snake_display_callback_for_e2e(node_id, cx);
        assert!(
            reset,
            "Thinking Agent 必须已创建可独立调度的 Working Snake Entity"
        );
    });
    // background settle = 50 × 12ms = 600ms，排空建卡和首次状态同步留下的持久化通知。
    for _ in 0..50 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(12));
        cx.run_until_parked();
    }
    app.update(cx, |app, cx| {
        let reset = app.reset_working_snake_display_callback_for_e2e(node_id, cx);
        assert!(reset, "测量前必须重新对齐 Working Snake 的逻辑时钟");
        app.reset_canvas_render_count_for_e2e();
        let reset_count = app.reset_working_snake_render_count_for_e2e(node_id, cx);
        assert!(
            reset_count,
            "目标 Working Snake Entity 必须支持独立 render 计数清零"
        );
    });

    let display_frame = Duration::from_nanos(11_111_112);
    let mut advanced_callbacks = 0usize;
    let mut notified_callbacks = 0usize;
    for _ in 0..90 {
        cx.cx.background_executor.advance_clock(display_frame);
        let callback = app.update(cx, |app, cx| {
            app.consume_working_snake_display_callback_for_e2e(node_id, cx)
                .expect("Thinking Agent 的 Working Snake callback 必须存在")
        });
        if callback.state_advanced {
            advanced_callbacks += 1;
        }
        if callback.repaint_notified {
            notified_callbacks += 1;
        }
        cx.run_until_parked();
    }

    let card_count = read_runtime(&app, cx, |runtime| runtime.document.nodes().len());
    let schedule = app.read_with(cx, |app, _cx| app.animation_schedule_snapshot_for_e2e());
    let snake = app
        .read_with(cx, |app, app_cx| {
            app.working_snake_snapshot_for_e2e(node_id, app_cx)
        })
        .expect("Thinking Agent 必须保留 Working Snake 局部状态");
    let canvas_render_count = app.read_with(cx, |app, _cx| app.canvas_render_count_for_e2e());

    assert_eq!(
        card_count, 6,
        "性能回归场景必须精确包含 5 Terminal + 1 Agent = 6 张卡片"
    );
    assert!(
        snake.active,
        "测量结束时 Working Snake 必须仍处于 active 状态"
    );
    assert!(
        snake.next_frame_requested,
        "前台 active Working Snake 必须继续请求下一显示帧"
    );
    assert_eq!(
        advanced_callbacks, 60,
        "90Hz 的约 1 秒 callback 必须通过余量累计精确推进 60 次 Snake 状态"
    );
    assert_eq!(
        notified_callbacks, 60,
        "90Hz 的约 1 秒 callback 必须精确通知目标 Snake Entity 重绘 60 次"
    );
    assert!(
        (0.82..=0.85).contains(&snake.phase),
        "1s / 1.2s ≈ 0.833，Snake phase 应位于 [0.82, 0.85]，实际为 {}",
        snake.phase
    );
    assert!(
        !schedule.canvas_motion_active,
        "Working Snake 已迁移到局部 Entity 后不得继续计入根 Canvas motion"
    );
    assert!(
        !schedule.canvas_ambient_active,
        "Working Snake 已迁移后不得继续让根 Canvas 保持 ambient 调度"
    );
    assert_eq!(
        canvas_render_count, 0,
        "90 个 display callback / 60 个 Working Snake 局部帧不得触发根 Canvas 重绘"
    );
    assert_eq!(
        snake.render_count, 60,
        "约 1 秒内目标 Snake Entity 应独立 render 60 次"
    );

    let resumed = app.update(cx, |app, cx| {
        app.resume_working_snake_display_driver_for_e2e(node_id, cx)
    });
    assert!(resumed, "窗口生命周期测试前必须恢复真实 Snake driver");
    cx.run_until_parked();
    let scheduled_snake = app
        .read_with(cx, |app, app_cx| {
            app.working_snake_snapshot_for_e2e(node_id, app_cx)
        })
        .expect("恢复真实 driver 后应保留 Working Snake Entity");
    assert!(
        scheduled_snake.frame_pending,
        "恢复真实 driver 后必须实际挂起一个 on_next_frame callback"
    );

    cx.deactivate_window();
    cx.run_until_parked();
    let unfocused_snake = app
        .read_with(cx, |app, app_cx| {
            app.working_snake_snapshot_for_e2e(node_id, app_cx)
        })
        .expect("窗口失去键盘焦点时应保留可见 Working Snake Entity");
    assert!(
        unfocused_snake.active,
        "窗口失去键盘焦点不应清除 Agent 的 Working Snake 业务状态"
    );
    assert!(
        unfocused_snake.next_frame_requested,
        "窗口失去键盘焦点但仍可见时 Working Snake 必须继续请求显示帧"
    );
    assert!(
        unfocused_snake.frame_pending,
        "窗口失去键盘焦点不得取消已经挂起的 Working Snake callback"
    );

    app.update(cx, |app, cx| {
        app.reset_canvas_render_count_for_e2e();
        let reset_count = app.reset_working_snake_render_count_for_e2e(node_id, cx);
        assert!(
            reset_count,
            "失焦阶段必须能独立清零目标 Working Snake Entity 的 render 计数"
        );
    });

    let mut unfocused_advanced_callbacks = 0usize;
    let mut unfocused_notified_callbacks = 0usize;
    for _ in 0..90 {
        cx.cx.background_executor.advance_clock(display_frame);
        let callback = app.update(cx, |app, cx| {
            app.consume_working_snake_display_callback_for_e2e(node_id, cx)
                .expect("失焦但可见的 Thinking Agent 必须保留 Working Snake callback")
        });
        if callback.state_advanced {
            unfocused_advanced_callbacks += 1;
        }
        if callback.repaint_notified {
            unfocused_notified_callbacks += 1;
        }
        cx.run_until_parked();
    }

    let unfocused_measured_snake = app
        .read_with(cx, |app, app_cx| {
            app.working_snake_snapshot_for_e2e(node_id, app_cx)
        })
        .expect("失焦测量结束后必须保留 Working Snake Entity");
    let unfocused_canvas_render_count =
        app.read_with(cx, |app, _cx| app.canvas_render_count_for_e2e());
    assert_eq!(
        unfocused_advanced_callbacks, 60,
        "失焦但可见时，90Hz 的约 1 秒 callback 仍必须精确推进 60 次 Snake 状态"
    );
    assert_eq!(
        unfocused_notified_callbacks, 60,
        "失焦但可见时，90Hz 的约 1 秒 callback 仍必须精确通知 60 次局部重绘"
    );
    assert_eq!(
        unfocused_measured_snake.render_count, 60,
        "失焦但可见时目标 Working Snake Entity 应独立 render 60 次"
    );
    assert!(
        (0.65..=0.68).contains(&unfocused_measured_snake.phase),
        "前台约 1s + 失焦约 1s 后，phase 应连续推进到 2s / 1.2s mod 1 ≈ [0.65, 0.68]，实际为 {}",
        unfocused_measured_snake.phase
    );
    assert_eq!(
        unfocused_canvas_render_count, 0,
        "失焦但可见的 60 个 Working Snake 局部帧不得触发根 Canvas 重绘"
    );

    cx.update(|window, _app_cx| window.activate_window());
    cx.run_until_parked();
    let resumed = app.update(cx, |app, cx| {
        app.resume_working_snake_display_driver_for_e2e(node_id, cx)
    });
    assert!(resumed, "窗口恢复焦点后必须恢复真实 Snake driver");
    cx.run_until_parked();
    let foreground_snake = app
        .read_with(cx, |app, app_cx| {
            app.working_snake_snapshot_for_e2e(node_id, app_cx)
        })
        .expect("窗口恢复前台时应保留 Working Snake Entity");
    assert!(
        foreground_snake.next_frame_requested,
        "窗口恢复前台后 active Working Snake 必须重新请求显示帧"
    );
    assert!(
        foreground_snake.frame_pending,
        "窗口恢复前台后必须实际重挂新代次的 on_next_frame callback"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
