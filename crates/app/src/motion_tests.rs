use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> CanvasRect {
    CanvasRect::new(x, y, w, h)
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.0001
}

/// M-1: hover 进入和离开分别使用 120ms / 180ms 的 motion token
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 7（任意稳定 id，用于定位目标卡片）
///   初始 hover.value = 0.0
///   hover_in  = 120ms，tick(60ms) 后进度 = 60 / 120 = 0.5
///   hover_out = 180ms，tick(90ms) 后进度 = 1.0 - 90 / 180 = 0.5
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ensure_node(7) 创建 idle 节点
///   2. set_hover(true) 后 tick(60ms) → hover 进入到 0.5
///   3. 再 tick(60ms) → hover 到达 1.0
///   4. set_hover(false) 后 tick(90ms) → hover 离开到 0.5
///   5. 再 tick(90ms) → hover 回到 0.0
///
/// 预期结果：
///   - 正断言：60ms 时 hover 是 0.5，120ms 时 hover 是 1.0
///   - 正断言：离开 90ms 时 hover 是 0.5，180ms 时 hover 是 0.0
///   - 负断言：完全离开后 priority 不应仍是 Hover
#[test]
fn test_hover_uses_distinct_enter_and_exit_durations() {
    let mut store = MotionStore::new(false);
    let node_id = 7;
    store.ensure_node(node_id, rect(0.0, 0.0, 100.0, 80.0));

    store.set_hover(node_id, true);
    assert!(
        store.tick(Duration::from_millis(60)),
        "hover 进入中应报告视觉状态变化"
    );
    assert!(
        approx(store.nodes[&node_id].hover.value, 0.5),
        "hover 进入 60ms 后应为 0.5，实际 {}",
        store.nodes[&node_id].hover.value
    );

    assert!(
        store.tick(Duration::from_millis(60)),
        "hover 进入到终点前应继续报告变化"
    );
    assert!(
        approx(store.nodes[&node_id].hover.value, 1.0),
        "hover 进入 120ms 后应到达 1.0，实际 {}",
        store.nodes[&node_id].hover.value
    );

    store.set_hover(node_id, false);
    assert!(
        store.tick(Duration::from_millis(90)),
        "hover 离开中应报告视觉状态变化"
    );
    assert!(
        approx(store.nodes[&node_id].hover.value, 0.5),
        "hover 离开 90ms 后应回落到 0.5，实际 {}",
        store.nodes[&node_id].hover.value
    );

    assert!(
        store.tick(Duration::from_millis(90)),
        "hover 离开到终点前应继续报告变化"
    );
    let visual =
        store.resolve_node_visual(node_id, rect(0.0, 0.0, 100.0, 80.0), Theme::glass_dark());
    assert!(
        approx(store.nodes[&node_id].hover.value, 0.0),
        "hover 离开 180ms 后应回到 0.0，实际 {}",
        store.nodes[&node_id].hover.value
    );
    assert_ne!(
        visual.priority,
        VisualPriority::Hover,
        "hover 完全离开后视觉优先级不应仍停留在 Hover"
    );
}

/// M-2: focused 状态优先级覆盖 selected 和 hover
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 9
///   selected/focused/hover 都设为 true
///   focus_selected = 180ms，因此 tick(180ms) 后 selected=1.0 且 focused=1.0
///   hover_in = 120ms，因此 tick(180ms) 足够让 hover=1.0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建节点并同时开启 hover、selected、focused
///   2. tick(180ms) 推进所有状态到终点
///   3. resolve_node_visual 解析最终视觉状态
///
/// 预期结果：
///   - 正断言：priority 为 Focused
///   - 正断言：stroke_color 使用 focused_stroke
///   - 负断言：priority 不应退化为 Selected 或 Hover
#[test]
fn test_focused_visual_priority_overrides_selected_and_hover() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 9;
    store.ensure_node(node_id, rect(0.0, 0.0, 120.0, 90.0));
    store.set_hover(node_id, true);
    store.set_selection_focus(node_id, true, true);

    assert!(
        store.tick(Duration::from_millis(180)),
        "focused/selected/hover 状态推进到终点前应报告变化"
    );
    let visual = store.resolve_node_visual(node_id, rect(0.0, 0.0, 120.0, 90.0), theme);

    assert_eq!(
        visual.priority,
        VisualPriority::Focused,
        "focused 应覆盖 selected 和 hover，成为最高视觉优先级"
    );
    assert_eq!(
        visual.stroke_color, theme.focused_stroke,
        "focused 到达终点后 stroke_color 应精确使用 focused_stroke"
    );
    assert_ne!(
        visual.priority,
        VisualPriority::Selected,
        "focused 已激活时 priority 不应退化为 Selected"
    );
    assert_ne!(
        visual.priority,
        VisualPriority::Hover,
        "focused 已激活时 priority 不应退化为 Hover"
    );
}

/// M-3: layout rect 从旧位置插值到最终位置，并在 420ms 后精确落点
///
/// 数据构造（含关键数值的推导过程）：
///   old rect = (0,0,100,80)
///   new rect = (200,100,160,120)
///   layout = 420ms
///   tick(210ms) → raw progress = 210 / 420 = 0.5
///   ease_out_cubic(0.5) = 1 - (1 - 0.5)^3 = 0.875
///   x = 0 + (200 - 0) * 0.875 = 175
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ensure_node 创建旧 rect
///   2. set_layout_target 设置新 rect
///   3. tick(210ms) 检查中间插值
///   4. 再 tick(210ms) 检查最终 rect
///
/// 预期结果：
///   - 正断言：中途 rect 位于旧/新之间，x≈175
///   - 正断言：420ms 后 rect 精确等于 new rect
///   - 负断言：中途 rect 不应提前等于最终 rect
#[test]
fn test_layout_rect_interpolates_and_reaches_target_after_420ms() {
    let mut store = MotionStore::new(false);
    let node_id = 11;
    let old_rect = rect(0.0, 0.0, 100.0, 80.0);
    let new_rect = rect(200.0, 100.0, 160.0, 120.0);
    store.ensure_node(node_id, old_rect);
    store.set_layout_target(node_id, new_rect);

    assert!(
        store.tick(Duration::from_millis(210)),
        "layout 210ms 时仍在插值，应报告视觉状态变化"
    );
    let halfway = store.nodes[&node_id].current_rect;
    assert!(
        approx(halfway.origin.x, 175.0),
        "layout 210ms 后 x 应为 175，实际 {}",
        halfway.origin.x
    );
    assert_ne!(
        halfway, new_rect,
        "layout 210ms 只是中途状态，不应提前等于最终 rect"
    );

    assert!(
        store.tick(Duration::from_millis(210)),
        "layout 到达 420ms 终点时应报告最后一次视觉状态变化"
    );
    let final_rect = store.nodes[&node_id].current_rect;
    assert_eq!(
        final_rect, new_rect,
        "layout 420ms 后 current_rect 应精确等于最终 rect"
    );
}

/// M-4: Reduce Motion 开启时 rect 立即到最终值，但 opacity/color 仍允许淡入
///
/// 数据构造（含关键数值的推导过程）：
///   reduce_motion = true
///   old rect = (0,0,100,80)，new rect = (300,120,180,130)
///   spawn_node opacity 初始直接为 1.0，避免空间动效替代方案造成不可见节点
///   hover_in = 120ms，Reduce Motion 不关闭颜色/透明度类状态反馈
///
/// 执行过程（逐步说明系统如何处理）：
///   1. spawn_node 创建节点
///   2. set_layout_target 设置新 rect
///   3. 不 tick 直接检查 rect 已到最终值
///   4. 开启 hover 并 tick(60ms)，检查 hover 颜色进度仍推进
///
/// 预期结果：
///   - 正断言：Reduce Motion 下 current_rect 立即等于 new rect
///   - 正断言：hover 60ms 后仍推进到 0.5
///   - 负断言：Reduce Motion 不应让节点 opacity 变为 0
#[test]
fn test_reduce_motion_snaps_rect_but_keeps_subtle_state_fades() {
    let mut store = MotionStore::new(true);
    let node_id = 13;
    let old_rect = rect(0.0, 0.0, 100.0, 80.0);
    let new_rect = rect(300.0, 120.0, 180.0, 130.0);
    store.spawn_node(node_id, old_rect);
    store.set_layout_target(node_id, new_rect);

    assert_eq!(
        store.nodes[&node_id].current_rect, new_rect,
        "Reduce Motion 开启时 rect 应立即跳到最终值，避免空间位移动画"
    );
    assert!(
        store.nodes[&node_id].opacity.value > 0.0,
        "Reduce Motion 不应把节点 opacity 置为 0，节点必须保持可见"
    );

    store.set_hover(node_id, true);
    assert!(
        store.tick(Duration::from_millis(60)),
        "Reduce Motion 下 hover 颜色/透明度反馈仍应推进"
    );
    assert!(
        approx(store.nodes[&node_id].hover.value, 0.5),
        "Reduce Motion 下 hover 60ms 后仍应为 0.5，实际 {}",
        store.nodes[&node_id].hover.value
    );
}

/// M-5: 环境变量只在明确开启值时启用 Reduce Motion
///
/// 数据构造（含关键数值的推导过程）：
///   "1" / "true" / "on" / "yes" 都表示开启
///   "0" / "false" / None 都表示关闭
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别传入开启和关闭字符串
///   2. 调用 reduce_motion_from_env_value 解析
///
/// 预期结果：
///   - 正断言：明确开启值返回 true
///   - 负断言：空值和关闭值返回 false
#[test]
fn test_reduce_motion_env_value_parsing() {
    assert!(
        reduce_motion_from_env_value(Some("1")),
        "MULTISOUL_REDUCE_MOTION=1 应开启 Reduce Motion"
    );
    assert!(
        reduce_motion_from_env_value(Some("true")),
        "MULTISOUL_REDUCE_MOTION=true 应开启 Reduce Motion"
    );
    assert!(
        reduce_motion_from_env_value(Some("on")),
        "MULTISOUL_REDUCE_MOTION=on 应开启 Reduce Motion"
    );
    assert!(
        !reduce_motion_from_env_value(Some("0")),
        "MULTISOUL_REDUCE_MOTION=0 不应开启 Reduce Motion"
    );
    let missing_value: Option<&str> = None;
    assert!(
        !reduce_motion_from_env_value(missing_value),
        "未设置 MULTISOUL_REDUCE_MOTION 时不应开启 Reduce Motion"
    );
}
