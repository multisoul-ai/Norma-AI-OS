//! Agent 完成态边框的可见流程验收（L1 headless GPUI）。

use gpui::TestAppContext;
use runtime::{AgentTaskState, AgentTaskUpdate};
use std::time::Duration;

use crate::harness::{create_codex_agent, echo_factory, enter_canvas, make_app};

/// E2E-05：未读的 Agent 完成提示仅在卡片边框呼吸，查看后恢复静态外观。
///
/// 数据构造（含关键数值的推导过程）：
///   一个新建 Codex Agent；Done 事件生成未读完成提示。
///   呼吸淡入 = 150ms，推进 200ms > 150ms 后读取稳定描边。
///   完成亮起 = 520ms；再推进 600ms 后总计 800ms > 520ms，亮起应结束。
///   查看后再推进 200ms > 150ms 淡出，读取静态描边。
///
/// 执行过程：
///   1. 创建卡片，记录空闲视觉。
///   2. 注入 Done 并渲染，推进 200ms，读取边框、外缘柔光与阴影投影。
///   3. 再推进 600ms，检查一次性亮起已结束但柔光保留。
///   4. 标记已查看，重新渲染并推进 200ms，读取恢复状态。
///
/// 预期结果：
///   - 正断言：未读时边框变色加粗、外缘亮起后持续呼吸；查看后恢复。
///   - 负断言：填充与阴影颜色不变，也不新增整卡彩色光晕。
#[test]
fn e2e_05_agent_done_neon_stays_on_border() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("边框验收应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);
    cx.run_until_parked();
    let idle = app.update(cx, |app, _| {
        app.agent_card_border_e2e_snapshot(node_id)
            .expect("新建 Agent 应有边框视觉")
    });

    app.update(cx, |app, cx| {
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "Stop".to_string(),
            to: AgentTaskState::Done,
            task_title: None,
            session_id: None,
        });
        assert!(changed, "Done 事件应更新 Agent 任务状态");
        cx.notify();
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(200), cx)
    });
    cx.run_until_parked();
    let unread = app.update(cx, |app, _| {
        app.agent_card_border_e2e_snapshot(node_id)
            .expect("未读完成提示应有边框视觉")
    });

    assert!(unread.border_is_emphasized, "未读完成态应显示加粗呼吸描边");
    assert!(unread.breathing_border_active, "未读完成态应启用呼吸描边");
    assert!(
        unread.border_glow_intensity_milli > 0,
        "未读完成态应有外缘柔光"
    );
    assert!(unread.border_flash_milli > 0, "刚完成时应短暂增强亮起");
    assert_ne!(
        unread.stroke_color, idle.stroke_color,
        "未读完成态描边应变色"
    );
    assert_eq!(
        unread.fill_color & 0xffffff00,
        idle.fill_color & 0xffffff00,
        "未读完成态不得把霓虹色混入卡片填充"
    );
    assert_eq!(
        unread.shadow_color & 0xffffff00,
        idle.shadow_color & 0xffffff00,
        "未读完成态不得染色整卡阴影"
    );
    assert_eq!(unread.shadow_count, 1, "未读完成态只能保留一层基础阴影");
    assert_ne!(unread.shadow_count, 5, "未读完成态不得叠加四层整卡光晕");

    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(600), cx)
    });
    cx.run_until_parked();
    let sustained = app.update(cx, |app, _| {
        app.agent_card_border_e2e_snapshot(node_id)
            .expect("未读完成提示应持续显示")
    });
    assert_eq!(
        sustained.border_flash_milli, 0,
        "一次性亮起应在 520ms 后结束"
    );
    assert!(
        sustained.border_glow_intensity_milli > 0,
        "亮起结束后外缘柔光仍应呼吸"
    );

    app.update(cx, |app, cx| {
        assert!(
            app.runtime.mark_card_viewed(node_id),
            "未读完成卡片应能标记为已查看"
        );
        cx.notify();
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(200), cx)
    });
    cx.run_until_parked();
    let viewed = app.update(cx, |app, _| {
        app.agent_card_border_e2e_snapshot(node_id)
            .expect("已查看 Agent 应有边框视觉")
    });
    assert!(!viewed.breathing_border_active, "查看后应停止呼吸描边");
    assert_eq!(
        viewed.border_glow_intensity_milli, 0,
        "查看后外缘柔光应消失"
    );
    assert_ne!(
        viewed.stroke_color, unread.stroke_color,
        "查看后不能保留未读霓虹描边"
    );
    app.update(cx, |app, _| app.runtime.kill_all());
}
