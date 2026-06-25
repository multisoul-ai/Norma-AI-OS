use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> CanvasRect {
    CanvasRect::new(x, y, w, h)
}

/// CG-1: Agent 卡片处于 Cool 呼吸态时，resolve_node_visual 应产出非空 corner_glow。
///
/// 数据构造（含关键数值推导）：
///   node_id = 51，Cool 呼吸周期 = 2500ms。
///   breathing_fade 淡入时长 = MotionTokens::default().breathing_fade = 150ms；
///   tick(200ms) 后 fade 已完全淡入（200ms > 150ms），fade.value 应为 1.0。
///
/// 执行过程：
///   1. ensure_node 创建空闲节点
///   2. set_breathing_intent(Cool)
///   3. tick(200ms)，让 breathing_fade 淡入完成、breathing_phase 前进
///   4. resolve_node_visual 读取 corner_glow
///
/// 预期结果：
///   - 正断言：corner_glow 为 Some，且 alpha 在 (0.0, 1.0] 区间内
///   - 正断言：blur 落在 [8.0, 14.0] 区间内（CORNER_GLOW_BLUR_MIN/MAX）
///   - 负断言：corner_glow 不应为 None（Cool 态下不应缺失光晕）
#[test]
fn test_resolve_node_visual_emits_corner_glow_when_breathing_active() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 51;
    let node_rect = rect(0.0, 0.0, 100.0, 80.0);
    store.ensure_node(node_id, node_rect);

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    store.tick(Duration::from_millis(200));

    let visual = store.resolve_node_visual(node_id, node_rect, theme);
    let glow = visual
        .corner_glow
        .expect("Cool 呼吸态下 corner_glow 不应为 None");

    assert!(
        glow.alpha > 0.0 && glow.alpha <= 1.0,
        "corner_glow.alpha 应落在 (0.0, 1.0] 区间，实际为 {}",
        glow.alpha
    );
    assert!(
        (8.0..=14.0).contains(&glow.blur),
        "corner_glow.blur 应落在 [8.0, 14.0] 区间，实际为 {}",
        glow.blur
    );
    assert!(
        visual.corner_glow.is_some(),
        "Cool 呼吸态下 corner_glow 不应缺失"
    );
}

/// CG-2: 未激活/已落定的呼吸态下，resolve_node_visual 不应产出 corner_glow。
///
/// 数据构造（含关键数值推导）：
///   node_id = 52，初始 BreathingIntent::Inactive（从未进入呼吸态）。
///
/// 执行过程：
///   1. ensure_node 创建空闲节点（不调用 set_breathing_intent，保持默认 Inactive）
///   2. resolve_node_visual 读取 corner_glow
///
/// 预期结果：
///   - 正断言：corner_glow 为 None
///   - 负断言：stroke_color 不应受影响地仍是静态 node_stroke（确认本改动未触碰既有描边路径）
#[test]
fn test_resolve_node_visual_omits_corner_glow_when_inactive() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 52;
    let node_rect = rect(0.0, 0.0, 100.0, 80.0);
    store.ensure_node(node_id, node_rect);

    let visual = store.resolve_node_visual(node_id, node_rect, theme);

    assert!(
        visual.corner_glow.is_none(),
        "Inactive 呼吸态下 corner_glow 应为 None"
    );
    assert_eq!(
        visual.stroke_color, theme.node_stroke,
        "未激活呼吸时 stroke_color 应保持静态 node_stroke，不应被本次改动影响"
    );
}
