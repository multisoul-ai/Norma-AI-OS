//! E2E-05：Master/detail 布局动画（L1 headless GPUI）。
//!
//! 通过公开 `Cmd+M` 快捷键驱动真实 App action 与全局动画帧循环，再读取卡片实际
//! 渲染矩形。canonical document bounds 会先写入终点，因此不能用它代替视觉中间帧。

use std::time::Duration;

use gpui::TestAppContext;

use crate::harness::{
    create_claude_agent, create_codex_agent, echo_factory, enter_canvas, make_app, read_runtime,
};

/// E2E-05：空闲 250ms 帧循环中的 `Cmd+M` 必须在 32ms 内唤醒并推进 Agent 布局动画。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards = Codex(1) + Claude(1) = 2，Claude 是最后创建且被选中的 Master target。
///   预热时间 = 1200ms > 初始化动画 992ms，也 > 布局动画 420ms，确保进入 250ms 空闲巡检。
///   首帧验收预算 = 32ms = 2 × 16ms active Canvas interval，必须已经出现中间帧。
///   收束预算 = 布局动画 420ms + 最多一个 active frame 16ms = 436ms。
///   32ms 后补时 = 436ms - 32ms = 404ms，覆盖动画时长与帧调度量化。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App，进入空 Canvas，依次创建 Codex 与 Claude Agent。
///   2. 推进 1200ms 让初始化与自动 reflow 完成，记录 Claude 的 canonical/visual 起点。
///   3. 经真实 keyboard seam 发送 `cmd-m`，读取已写入 document 的终点与即时视觉矩形。
///   4. 推进 32ms 并运行 GPUI executor，读取动画中间帧。
///   5. 再推进 404ms，读取 436ms 收束预算后的视觉终点。
///
/// 预期结果：
///   - 正断言：`Cmd+M` 写入的新 canonical 终点不同于旧布局。
///   - 正断言：快捷键当下视觉矩形仍在旧起点，避免首帧闪跳。
///   - 正断言：32ms 时视觉矩形已离开起点且尚未到达终点，证明空闲 timer 被即时唤醒。
///   - 正断言：累计 436ms 后视觉矩形精确到达 canonical 终点。
///   - 负断言：32ms 中间帧不得仍停在起点，也不得直接跳到终点。
#[test]
fn e2e_05_cmd_m_wakes_idle_agent_layout_animation_within_32ms() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Master layout 动画测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _codex_id = create_codex_agent(&app, cx);
    let claude_id = create_claude_agent(&app, cx);

    // settle budget = 1200ms > initialization(992ms) 且 > layout(420ms)，
    // 一次推进即可让当前 timer 醒来并把已有动画收束，再进入 250ms idle timer。
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(1200));
    cx.run_until_parked();

    let old_canonical = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .node(claude_id)
            .expect("Claude Agent 节点在 Cmd+M 前应存在")
            .bounds
    });
    let old_visual = app
        .update(cx, |app, _| app.node_visual_rect_e2e(claude_id))
        .expect("Claude Agent 在 Cmd+M 前应有实际渲染矩形");

    cx.simulate_keystrokes("cmd-m");
    cx.run_until_parked();

    let target_canonical = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .node(claude_id)
            .expect("Claude Agent 节点在 Cmd+M 后应继续存在")
            .bounds
    });
    let immediate_visual = app
        .update(cx, |app, _| app.node_visual_rect_e2e(claude_id))
        .expect("Cmd+M 后 Claude Agent 应继续有实际渲染矩形");

    // first-frame budget = 2 × active interval(16ms) = 32ms。
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(32));
    cx.run_until_parked();
    let visual_at_32ms = app
        .update(cx, |app, _| app.node_visual_rect_e2e(claude_id))
        .expect("32ms 时 Claude Agent 应有动画中间帧");

    // settle budget = layout duration(420ms) + one active frame(16ms) = 436ms。
    // remaining = 436ms - elapsed(32ms) = 404ms。
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(404));
    cx.run_until_parked();
    let visual_at_436ms = app
        .update(cx, |app, _| app.node_visual_rect_e2e(claude_id))
        .expect("累计 436ms 时 Claude Agent 应有收束后的视觉矩形");

    assert_ne!(
        target_canonical, old_canonical,
        "Cmd+M 必须为选中的 Claude Agent 写入不同于旧布局的 Master canonical 终点"
    );
    assert_eq!(
        old_visual, old_canonical,
        "预热 1200ms 后 Claude Agent 的视觉起点应与旧 canonical 布局一致"
    );
    assert_eq!(
        immediate_visual, old_visual,
        "Cmd+M 当下应保留旧视觉起点，由后续动画帧连续过渡"
    );
    assert_ne!(
        visual_at_32ms, old_visual,
        "32ms = 2 × 16ms 后视觉矩形必须离开起点；仍相等说明 250ms idle timer 未被唤醒"
    );
    assert_ne!(
        visual_at_32ms, target_canonical,
        "32ms 远小于 420ms 布局时长，视觉矩形不得直接跳到 canonical 终点"
    );
    assert_eq!(
        visual_at_436ms, target_canonical,
        "累计 436ms 后视觉矩形必须精确收束到 Cmd+M 写入的 canonical 终点"
    );

    app.update(cx, |app, _| app.runtime.kill_all());
}
