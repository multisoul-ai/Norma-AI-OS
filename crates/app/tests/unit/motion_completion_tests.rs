use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> CanvasRect {
    CanvasRect::new(x, y, w, h)
}

/// MC-1: 运行中的冷色呼吸进入 Done 时触发一次性“呼气落定”。
///
/// 数据构造（含关键数值推导）：
///   node_id = 41，初始为空闲节点。
///   Cool 呼吸周期 = 2500ms，tick(625ms) 后 phase = 625 / 2500 = 0.25。
///   completion_settle 目标时长 = 520ms，tick(260ms) 后仍应有一半左右的落定余韵。
///
/// 执行过程：
///   1. ensure_node 创建节点
///   2. set_breathing_intent(Cool) 并 tick(625ms)，模拟运行中的专注呼吸
///   3. set_breathing_intent(Done)，模拟 agent 完成
///   4. 分别检查完成瞬间、260ms、520ms 后的视觉状态
///
/// 预期结果：
///   - 正断言：完成瞬间和 260ms 时 stroke 不应立即掉回静态色
///   - 正断言：520ms 后 stroke 回到静态色，status dot 保持 done green
///   - 负断言：完成后不应继续保留 Cool breathing_state
#[test]
fn test_done_after_cool_breathing_plays_settle_exhale_then_rests_green() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 41;
    let node_rect = rect(0.0, 0.0, 100.0, 80.0);
    store.ensure_node(node_id, node_rect);

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    store.tick(Duration::from_millis(625));
    store.set_breathing_intent(node_id, BreathingIntent::Done);

    let immediate = store.resolve_node_visual(node_id, node_rect, theme);
    assert_ne!(
        immediate.stroke_color, theme.node_stroke,
        "Done 进入瞬间应保留呼吸收拢的完成光，不应立即回到静态描边"
    );
    assert_eq!(
        store.nodes[&node_id].breathing_state,
        BreathingState::Inactive,
        "Done 后循环呼吸应停止，避免完成态继续无限呼吸"
    );

    store.tick(Duration::from_millis(260));
    let halfway = store.resolve_node_visual(node_id, node_rect, theme);
    assert_ne!(
        halfway.stroke_color, theme.node_stroke,
        "520ms 落定中点仍应有可见完成余韵"
    );

    store.tick(Duration::from_millis(260));
    let final_visual = store.resolve_node_visual(node_id, node_rect, theme);
    assert_eq!(
        final_visual.stroke_color, theme.node_stroke,
        "520ms 呼气落定结束后 stroke 应回到静态描边"
    );
    assert_eq!(
        final_visual.status_color,
        mix_color(theme.text_tertiary, theme.accent_green, 0.86),
        "Done 落定结束后状态点应保持安静绿色完成信号"
    );
}

/// MC-2: 没有先运行呼吸时进入 Done，不应凭空播放完成落定动效。
///
/// 数据构造：
///   node_id = 43，节点从 Inactive 直接收到 Done intent。
///   这种情况常见于恢复历史状态或首次渲染已完成任务。
///
/// 执行过程：
///   1. ensure_node 创建空闲节点
///   2. set_breathing_intent(Done)
///   3. 立即解析视觉状态
///
/// 预期结果：
///   - 正断言：stroke 保持静态色，避免无来源的完成闪动
///   - 正断言：status dot 仍表达 done green
///   - 负断言：completion_settle 不应被激活
#[test]
fn test_done_without_prior_active_breathing_rests_green_without_settle_pulse() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 43;
    let node_rect = rect(0.0, 0.0, 100.0, 80.0);
    store.ensure_node(node_id, node_rect);

    store.set_breathing_intent(node_id, BreathingIntent::Done);
    let visual = store.resolve_node_visual(node_id, node_rect, theme);

    assert_eq!(
        visual.stroke_color, theme.node_stroke,
        "直接进入 Done 时 stroke 应保持静态色，不应播放无来源完成光"
    );
    assert_eq!(
        visual.status_color,
        mix_color(theme.text_tertiary, theme.accent_green, 0.86),
        "直接进入 Done 仍应使用绿色状态点表达完成态"
    );
    assert_eq!(
        store.nodes[&node_id].completion_settle.value, 0.0,
        "没有先运行呼吸时 completion_settle 不应被激活"
    );
}
