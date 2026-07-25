use std::time::Duration;

use gpui::TestAppContext;
use runtime::NodeInput;

use crate::harness::{
    create_terminal, enter_canvas, make_app, read_grid, runtime_drain_step, scripted_factory,
};

fn grid_text(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    node_id: canvas::NodeId,
) -> String {
    read_grid(app, cx, node_id)
        .expect("Terminal 节点存在时必须可读取 GridSnapshot")
        .rows
        .iter()
        .flatten()
        .map(|cell| cell.c)
        .collect()
}

/// E2E-19：8ms burst 的三帧 Terminal 输出被合并，但预算边界后必须显示最终 frame-3。
///
/// 数据构造（含关键数值的推导过程）：
///   scripted outputs = frame-1、frame-2、frame-3，共 3 帧
///   write timestamps = 同一 executor turn，跨度 0ms ≤ 8ms
///   Runtime drain step = 16ms
///   before drain = 15ms < 16ms
///   repaint deadline = first drain 16ms + repaint budget 16ms = 32ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建一个 scripted Terminal，完成初始化动画并清零 Canvas render 计数。
///   2. 同一 turn 连续写入三个 trigger，PTY 依次排入三份清屏输出。
///   3. mock clock 先推进 15ms，确认 Runtime 尚未 drain。
///   4. 再推进 1ms，Runtime 一次 drain 到最终 grid，但 repaint gate 仍 pending。
///   5. 再推进 16ms 到 repaint deadline，确认 Canvas 至少实际渲染一次。
///
/// 预期结果：
///   - 负断言：15ms 时 grid 不得提前出现 frame-3。
///   - 正断言：16ms drain 后 grid 包含最终 frame-3。
///   - 负断言：最终 grid 不得残留 frame-1 或 frame-2。
///   - 正断言：32ms 预算边界后必须有实际 Canvas render，最终输出不能永远 pending。
#[test]
fn e2e_19_terminal_burst_is_coalesced_without_losing_final_grid_state() {
    let mut test_context = TestAppContext::single();
    let clear_home = b"\x1b[2J\x1b[H";
    let rules = vec![
        (
            b"emit-one".to_vec(),
            [clear_home.as_slice(), b"frame-1"].concat(),
        ),
        (
            b"emit-two".to_vec(),
            [clear_home.as_slice(), b"frame-2"].concat(),
        ),
        (
            b"emit-three".to_vec(),
            [clear_home.as_slice(), b"frame-3"].concat(),
        ),
    ];
    let (app, cx) = make_app(&mut test_context, scripted_factory(rules));
    let workspace = tempfile::TempDir::new().expect("E2E-19 workspace tempdir 应创建成功");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_terminal(&app, cx);

    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_secs(2), cx);
        app.reset_canvas_render_count_for_e2e();
    });
    cx.run_until_parked();
    app.update(cx, |app, _cx| {
        app.reset_canvas_render_count_for_e2e();
        for trigger in ["emit-one", "emit-two", "emit-three"] {
            app.runtime
                .send_node_input(node_id, NodeInput::Text(trigger.to_string()))
                .expect("三次 scripted Terminal 输入都应成功写入 PTY");
        }
    });

    let runtime_step = runtime_drain_step();
    // before_drain = 16ms - 1ms = 15ms
    let before_drain = runtime_step.saturating_sub(Duration::from_millis(1));
    cx.cx.background_executor.advance_clock(before_drain);
    cx.run_until_parked();
    let before_text = grid_text(&app, cx, node_id);

    assert!(
        !before_text.contains("frame-3"),
        "15ms 尚未到 Runtime drain 边界，grid 不得提前显示 frame-3"
    );

    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    let drained_text = grid_text(&app, cx, node_id);
    let renders_before_deadline = app.read_with(cx, |app, _cx| app.canvas_render_count_for_e2e());

    assert!(
        drained_text.contains("frame-3"),
        "16ms Runtime drain 必须消费全部 burst 并保留最终 frame-3"
    );
    assert!(
        !drained_text.contains("frame-1"),
        "最终 grid 不得停留或残留被后续清屏覆盖的 frame-1"
    );
    assert!(
        !drained_text.contains("frame-2"),
        "最终 grid 不得停留或残留被后续清屏覆盖的 frame-2"
    );
    assert_eq!(
        renders_before_deadline, 0,
        "首次 drain 只应置 repaint pending，16ms 预算到期前不得实际绘制 Canvas"
    );

    cx.cx.background_executor.advance_clock(runtime_step);
    cx.run_until_parked();
    let renders_after_deadline = app.read_with(cx, |app, _cx| app.canvas_render_count_for_e2e());

    assert!(
        renders_after_deadline >= 1,
        "32ms repaint deadline 后必须实际渲染最终 grid，单次 pending 不能永久丢失"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
