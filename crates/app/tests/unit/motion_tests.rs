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

/// M-6: 呼吸完全淡入后 stroke_color 精确等于冷色组在该相位的呼吸色。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 21，空闲（无 hover/selected/focus），accent_mix = 0 → base_stroke = node_stroke
///   breathing_fade token = 150ms，tick(1250ms) 时 step = 1250/150 ≈ 8.3 远超 1，立刻 clamp 到 target=1.0
///   Cool 周期 = 2500ms，tick(1250ms) 后 phase = 1250 / 2500 = 0.5（与呼吸纯函数 BC-3 用例一致）
///   theme.glass_dark() 下 phase=0.5 Cool 的呼吸色已在 motion_breathing_tests 验证为精确值 0x7363ffb3
///
/// 执行过程：
///   1. ensure_node 创建空闲节点
///   2. set_breathing_intent(Cool)
///   3. tick(1250ms) 一次性让 fade 到 1.0、phase 到 0.5
///   4. resolve_node_visual 读取最终 stroke_color
///
/// 预期结果：
///   - 正断言：stroke_color 精确等于 0x7363ffb3
///   - 负断言：stroke_color 不应仍是未呼吸的静态 node_stroke
#[test]
fn test_breathing_fully_faded_in_uses_exact_cool_phase_color() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 21;
    store.ensure_node(node_id, rect(0.0, 0.0, 100.0, 80.0));

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    store.tick(Duration::from_millis(1250));
    let visual = store.resolve_node_visual(node_id, rect(0.0, 0.0, 100.0, 80.0), theme);

    assert_eq!(
        visual.stroke_color, 0x7363ffb3,
        "完全淡入后 stroke_color 应精确等于 Cool 色组 phase=0.5 的呼吸色，实际 {:#010x}",
        visual.stroke_color
    );
    assert_ne!(
        visual.stroke_color, theme.node_stroke,
        "呼吸激活且完全淡入时不应仍停留在静态 node_stroke"
    );
}

/// M-7: 色组切换是硬切——相位立即回到新色组起点，呼吸淡入状态不被打断。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 23，先 set_breathing_intent(Cool) 并 tick(625ms)：
///     phase = 625/2500 = 0.25，fade 立刻 clamp 到 1.0（625ms >> 150ms token）
///   随后 set_breathing_intent(Warm)：色组从 Cool 变为 Warm → phase 重置为 0.0；
///     Warm 仍是激活态，fade.target 仍是 1.0，且 fade.value 已是 1.0 → 不产生跨色组渐变缺口
///   不再 tick，直接 resolve：phase=0.0 时 Warm 呼吸色已在 BC-5 验证为精确值 0xf2c26bb3
///
/// 执行过程：
///   1. 激活 Cool 并 tick(625ms) 进入呼吸中段
///   2. 切到 Warm（不 tick）
///   3. resolve_node_visual 读取 stroke_color
///
/// 预期结果：
///   - 正断言：stroke_color 精确等于 0xf2c26bb3（Warm 起点色，硬切无残留）
///   - 负断言：fade 不应因切色组而跌落到 0（即颜色不应退回静态 node_stroke）
#[test]
fn test_breathing_state_switch_hard_cuts_phase_without_fade_dip() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 23;
    store.ensure_node(node_id, rect(0.0, 0.0, 100.0, 80.0));

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    store.tick(Duration::from_millis(625));

    store.set_breathing_intent(node_id, BreathingIntent::Warm);
    let visual = store.resolve_node_visual(node_id, rect(0.0, 0.0, 100.0, 80.0), theme);

    assert_eq!(
        visual.stroke_color, 0xf2c26bb3,
        "切到 Warm 后应硬切到其相位起点呼吸色，实际 {:#010x}",
        visual.stroke_color
    );
    assert_ne!(
        visual.stroke_color, theme.node_stroke,
        "色组切换瞬间不应因重置而跌回静态 node_stroke"
    );
}

/// M-8: 退出激活态后边框在 150ms 内淡出回静态色，相位在淡出期间冻结。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 29，先激活 Cool 并 tick(625ms) → phase=0.25，fade=1.0
///   set_breathing_intent(Inactive) 后 fade.target=0，但 breathing_state 变为 Inactive
///     → tick 中相位推进条件 `breathing_state != Inactive` 不成立，phase 冻结在 0.25
///   tick(75ms)（150ms 淡出 token 的一半）→ fade.value = 1.0 - 75/150 = 0.5（半淡出，颜色应为静态与呼吸色之间）
///   再 tick(75ms) 共 150ms → fade.value = 0.0 → 完全回到静态 base_stroke
///
/// 执行过程：
///   1. 激活 Cool 并 tick(625ms)
///   2. set_breathing_intent(Inactive) 模拟进入 Ready/Ended
///   3. tick(75ms) 检查半淡出的中间态
///   4. 再 tick(75ms) 检查淡出完成后的静态终态
///
/// 预期结果：
///   - 正断言：半淡出时 stroke_color 既不等于纯静态色也不等于完全呼吸色（真正处于混合中）
///   - 正断言：150ms 淡出完成后 stroke_color 精确等于静态 node_stroke
///   - 负断言：淡出完成后不应仍带有呼吸色痕迹
#[test]
fn test_breathing_fades_out_to_static_color_within_150ms_after_deactivation() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 29;
    store.ensure_node(node_id, rect(0.0, 0.0, 100.0, 80.0));

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    store.tick(Duration::from_millis(625));
    store.set_breathing_intent(node_id, BreathingIntent::Inactive);

    store.tick(Duration::from_millis(75));
    let halfway_visual = store.resolve_node_visual(node_id, rect(0.0, 0.0, 100.0, 80.0), theme);
    let full_breathing = breathing_color_for_phase(0.25, BreathingState::Cool, theme);
    assert_ne!(
        halfway_visual.stroke_color, theme.node_stroke,
        "淡出过半时不应已经回到纯静态色"
    );
    assert_ne!(
        halfway_visual.stroke_color, full_breathing,
        "淡出过半时不应仍是完全饱和的呼吸色"
    );

    store.tick(Duration::from_millis(75));
    let final_visual = store.resolve_node_visual(node_id, rect(0.0, 0.0, 100.0, 80.0), theme);
    assert_eq!(
        final_visual.stroke_color, theme.node_stroke,
        "150ms 淡出完成后应精确回到静态 node_stroke，实际 {:#010x}",
        final_visual.stroke_color
    );
}

/// M-9: Reduce Motion 开启时呼吸完全不推进相位、不淡入，边框保持静态色。
///
/// 数据构造（含关键数值的推导过程）：
///   reduce_motion = true，node_id = 31
///   set_breathing_intent(Cool) 在 reduce_motion 下直接把 fade.value/target 强制为 0
///   tick(2500ms)（整整一个 Cool 呼吸周期）：tick 中 `!reduce_motion && ...` 为 false，相位不推进
///
/// 执行过程：
///   1. 用 reduce_motion=true 创建 store
///   2. set_breathing_intent(Cool)
///   3. tick(2500ms)，刻意覆盖一整个呼吸周期
///   4. resolve_node_visual 读取 stroke_color
///
/// 预期结果：
///   - 正断言：stroke_color 精确等于静态 node_stroke（完全不呼吸）
///   - 负断言：stroke_color 不应等于 Cool 色组任意相位的呼吸色
#[test]
fn test_reduce_motion_disables_breathing_phase_and_fade_entirely() {
    let mut store = MotionStore::new(true);
    let theme = Theme::glass_dark();
    let node_id = 31;
    store.ensure_node(node_id, rect(0.0, 0.0, 100.0, 80.0));

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    store.tick(Duration::from_millis(2500));
    let visual = store.resolve_node_visual(node_id, rect(0.0, 0.0, 100.0, 80.0), theme);

    assert_eq!(
        visual.stroke_color, theme.node_stroke,
        "Reduce Motion 开启时 stroke_color 应精确保持静态 node_stroke，实际 {:#010x}",
        visual.stroke_color
    );
    assert_ne!(
        visual.stroke_color,
        breathing_color_for_phase(0.0, BreathingState::Cool, theme),
        "Reduce Motion 开启时不应渗入任何呼吸色"
    );
}

/// M-10: Agent 工作态应通过卡片自身描边/阴影呼吸，而不是依赖 chase 直线 overlay。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 33，初始 Idle，theme.node_stroke = 0xffffff24。
///   chase_fade token = 150ms，set_chase_intent(true) 后 tick(150ms)：
///     chase_fade.value = 1.0，chase_phase = 150ms / 1200ms = 0.125。
///   工作态视觉应复用 node_div 自身 rounded border/shadow，不再产生单独 overlay 数据。
///
/// 执行过程：
///   1. ensure_node 创建空闲节点
///   2. set_chase_intent(true) 模拟 Agent Thinking/RunningTool
///   3. tick(150ms) 让工作态淡入完成
///   4. resolve_node_visual 读取描边、阴影和强调边框状态
///
/// 预期结果：
///   - 正断言：stroke_color 不再等于静态 node_stroke，说明工作态有自身边框反馈
///   - 正断言：border_is_emphasized=true，复用卡片真实 rounded border 路径
///   - 正断言：shadow_blur 大于主题默认值，说明工作态通过阴影呼吸表达活跃
///   - 负断言：priority 不应伪装成 Focused，工作态不是输入焦点
#[test]
fn test_working_state_uses_card_chrome_glow_without_chase_overlay() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 33;
    let node_rect = rect(0.0, 0.0, 100.0, 80.0);
    store.ensure_node(node_id, node_rect);

    store.set_chase_intent(node_id, true);
    store.tick(Duration::from_millis(150));
    let visual = store.resolve_node_visual(node_id, node_rect, theme);

    assert_ne!(
        visual.stroke_color, theme.node_stroke,
        "Agent 工作态淡入完成后 stroke_color 不应仍是静态 node_stroke"
    );
    assert!(
        visual.border_is_emphasized,
        "Agent 工作态应复用卡片自身 emphasized border，而不是额外直线 overlay"
    );
    assert!(
        visual.shadow_blur > theme.shadow_blur,
        "Agent 工作态应增加卡片自身 shadow_blur，实际 {} <= 默认 {}",
        visual.shadow_blur,
        theme.shadow_blur
    );
    assert_ne!(
        visual.priority,
        VisualPriority::Focused,
        "Agent 工作态不应伪装成 Focused；真实输入焦点仍应由 selection/focus 控制"
    );
}
