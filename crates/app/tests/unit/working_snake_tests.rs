use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> CanvasRect {
    CanvasRect::new(x, y, w, h)
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.0001
}

/// MWS-1: Agent 工作态应输出一段沿圆角边框移动的贪吃蛇 segment，而不是整圈渐变 ring。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 41（任意稳定 id）
///   参考视觉：只有一段高亮沿边框跑，剩余边框保持卡片正常描边
///   本主题内等价色：accent_cyan = 0x27d7eaff，accent_violet = 0x9b55ffff
///   snake 可见 alpha 固定为 0xe6，因此 head=0x27d7eae6、tail=0x9b55ffe6
///   为了保持连续流动，完整绕行周期 = 1.2s：
///     tick(300ms) 后 head_phase = 300 / 1200 = 0.25
///   snake_tail_fraction = 0.24，表示蛇身占整条圆角边框路径的 24%
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ensure_node 创建空闲节点
///   2. set_chase_intent(true) 模拟 Agent Thinking/RunningTool 工作态
///   3. tick(300ms) 让 fade 完成并把蛇头推进到 25% 路径处
///   4. resolve_node_visual 读取 working_snake 数据
///
/// 预期结果：
///   - 正断言：working_snake 存在，canvas 才能画移动 segment
///   - 正断言：head_phase 精确推进到 0.25
///   - 正断言：tail_fraction 精确等于 0.24，避免退化为整圈 ring
///   - 正断言：head_color / tail_color 使用青紫双色
///   - 负断言：tail_fraction 不应大于 0.5，否则视觉上接近整圈发光
///   - 负断言：priority 不应伪装成 Focused，工作态不是输入焦点
#[test]
fn test_agent_working_snake_uses_moving_segment_not_full_ring() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 41;
    let node_rect = rect(0.0, 0.0, 190.0, 254.0);
    store.ensure_node(node_id, node_rect);

    store.set_chase_intent(node_id, true);
    assert!(
        store.tick(Duration::from_millis(300)),
        "工作态 snake 淡入和 1.2s 路径推进期间应报告视觉变化"
    );
    let visual = store.resolve_node_visual(node_id, node_rect, theme);
    let snake = visual
        .working_snake
        .expect("Agent 工作态淡入后必须提供 working_snake 数据");

    assert!(
        approx(snake.head_phase, 0.25),
        "1.2s 绕行 tick(300ms) 后 head_phase 应为 0.25，实际 {}",
        snake.head_phase
    );
    assert!(
        approx(snake.tail_fraction, 0.24),
        "snake_tail_fraction 应为 0.24，实际 {}",
        snake.tail_fraction
    );
    assert_eq!(
        snake.head_color, 0x27d7eae6,
        "working_snake.head_color 应为 theme.accent_cyan 且 alpha=0xe6"
    );
    assert_eq!(
        snake.tail_color, 0x9b55ffe6,
        "working_snake.tail_color 应为 theme.accent_violet 且 alpha=0xe6"
    );
    assert!(
        snake.tail_fraction < 0.5,
        "snake_tail_fraction 必须小于半圈，否则会退化成整圈发光"
    );
    assert_ne!(
        visual.priority,
        VisualPriority::Focused,
        "Agent 工作态不应伪装成 Focused；真实输入焦点仍应由 selection/focus 控制"
    );
}

/// MWS-2: Agent 工作态 snake 稳定运行后应推进真实相位，但不超过 15fps repaint。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 42（任意稳定 id，与 MWS-1 区分）
///   完整绕行周期 = 1200ms
///   chase_fade token = 150ms，tick(150ms) 后 fade 完成且 phase = 150 / 1200 = 0.125
///   running snake repaint 预算 = ceil(1000ms / 15fps) = 67ms：
///     前 5 个稳定 tick = 12ms × 5 = 60ms < 67ms → 均不应 repaint
///     第 6 个稳定 tick：累计 72ms >= 67ms → 应 repaint
///   节流期间 phase = (150 + 12 × 5) / 1200 = 0.175
///   最终 phase = (150 + 12 × 6) / 1200 = 0.185
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ensure_node 创建空闲节点
///   2. set_chase_intent(true) 模拟 Agent Thinking/RunningTool 工作态
///   3. tick(150ms) 完成 snake 淡入
///   4. 连续执行五个 12ms tick，验证稳定期不 repaint 但 phase 持续推进。
///   5. 第六个 12ms tick 达到预算后放行 repaint，并读取最终 working_snake 相位。
///
/// 预期结果：
///   - 正断言：淡入 tick 返回 true，确保动画首次出现
///   - 负断言：第 1 个 12ms 稳定 tick 不应 repaint，避免整张 Canvas 以约 83fps 重绘
///   - 负断言：第 2～5 个 12ms tick 累计最多 60ms < 67ms，仍不应 repaint。
///   - 正断言：节流期间 head_phase 推进到 0.175，动画逻辑时间没有冻结。
///   - 正断言：第 6 个 12ms tick 应返回 true，以不超过 15fps 更新画面。
///   - 正断言：head_phase 推进到 0.185，证明节流没有冻结真实动画时间。
///   - 负断言：head_phase 不应停在淡入完成时的 0.125
#[test]
fn test_stable_working_snake_throttles_repaint_without_freezing_phase() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 42;
    let node_rect = rect(0.0, 0.0, 190.0, 254.0);
    store.ensure_node(node_id, node_rect);

    store.set_chase_intent(node_id, true);
    let fade_in_changed = store.tick(Duration::from_millis(150));
    let first_short_tick_changed = store.tick(Duration::from_millis(12));
    let second_short_tick_changed = store.tick(Duration::from_millis(12));
    let third_short_tick_changed = store.tick(Duration::from_millis(12));
    let fourth_short_tick_changed = store.tick(Duration::from_millis(12));
    let fifth_short_tick_changed = store.tick(Duration::from_millis(12));
    let suppressed_visual = store.resolve_node_visual(node_id, node_rect, theme);
    let suppressed_snake = suppressed_visual
        .working_snake
        .expect("节流期间工作态仍必须保留上一帧 working_snake 数据");
    let sixth_short_tick_changed = store.tick(Duration::from_millis(12));
    let visual = store.resolve_node_visual(node_id, node_rect, theme);
    let snake = visual
        .working_snake
        .expect("节流后工作态仍必须提供 working_snake 数据");

    assert!(
        fade_in_changed,
        "tick(150ms) 应完成 snake 淡入并触发首次 repaint"
    );
    assert!(
        !first_short_tick_changed,
        "淡入完成后的第 1 个 12ms tick 不应触发 repaint，否则整张 Canvas 仍是约 83fps"
    );
    assert!(
        !second_short_tick_changed,
        "淡入完成后的第 2 个 12ms tick 累计 24ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        !third_short_tick_changed,
        "淡入完成后的第 3 个 12ms tick 累计 36ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        !fourth_short_tick_changed,
        "淡入完成后的第 4 个 12ms tick 累计 48ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        !fifth_short_tick_changed,
        "淡入完成后的第 5 个 12ms tick 累计 60ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        approx(suppressed_snake.head_phase, 0.175),
        "节流期间可渲染 head_phase 应推进到 0.175，实际 {}",
        suppressed_snake.head_phase
    );
    assert!(
        sixth_short_tick_changed,
        "淡入完成后的第 6 个 12ms tick 累计 72ms >= 67ms，应触发不超过 15fps 的下一帧 repaint"
    );
    assert!(
        approx(snake.head_phase, 0.185),
        "节流不应冻结动画时间；最终 head_phase 应为 0.185，实际 {}",
        snake.head_phase
    );
    assert!(
        !approx(snake.head_phase, 0.125),
        "head_phase 不应停在淡入完成时的 0.125，否则动画会卡住"
    );
}

/// MWS-2B: 呼吸动画完全淡入后的稳定期应节流 repaint，但累计时间仍要推进 phase。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 52（任意稳定 id，与 snake 用例区分）。
///   Cool 呼吸周期 = 2500ms。
///   breathing_fade token = 150ms，tick(150ms) 后 fade 完成且 phase = 150 / 2500 = 0.06。
///   repaint 预算 = ceil(1000ms / 15fps) = 67ms，避免 12ms tick 触发约 83fps 整画布 repaint。
///   前 5 个稳定 tick = 12ms × 5 = 60ms < 67ms → 均不应 repaint，但 phase 继续推进。
///   第 6 个稳定 tick：累计 72ms >= 67ms → 应 repaint。
///   节流期间 phase = (150 + 12 × 5) / 2500 = 0.084。
///   最终 phase = (150 + 12 × 6) / 2500 = 0.0888。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ensure_node 创建空闲节点。
///   2. set_breathing_intent(Cool) 模拟 Agent 打开后的呼吸边框。
///   3. tick(150ms) 完成淡入并记录 phase。
///   4. 连续执行五个 12ms tick，验证稳定期不触发 repaint 但 phase 持续推进。
///   5. 第六个 12ms tick 放行 repaint，并提交下一帧 notify。
///
/// 预期结果：
///   - 正断言：淡入 tick 返回 true，确保呼吸动画首次出现。
///   - 负断言：第 1 个 12ms 稳定 tick 不应返回 true，避免每 12ms repaint。
///   - 负断言：第 2～5 个 12ms 稳定 tick 不应返回 true，累计最多 60ms。
///   - 正断言：节流期间 breathing_phase 推进到 0.084，外部 repaint 不会看到旧相位。
///   - 正断言：第 6 个 12ms 稳定 tick 返回 true，动画不超过 15fps 更新。
///   - 正断言：breathing_phase 推进到 0.0888，证明节流没有冻结真实动画时间。
///   - 负断言：breathing_phase 不应停在淡入完成时的 0.06。
#[test]
fn test_stable_breathing_throttles_repaint_without_freezing_phase() {
    let mut store = MotionStore::new(false);
    let node_id = 52;
    let node_rect = rect(0.0, 0.0, 190.0, 254.0);
    store.ensure_node(node_id, node_rect);

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    let fade_in_changed = store.tick(Duration::from_millis(150));
    let phase_after_fade = store
        .nodes
        .get(&node_id)
        .expect("node_id 应在 ensure_node 后存在")
        .breathing_phase;
    let first_short_tick_changed = store.tick(Duration::from_millis(12));
    let second_short_tick_changed = store.tick(Duration::from_millis(12));
    let third_short_tick_changed = store.tick(Duration::from_millis(12));
    let fourth_short_tick_changed = store.tick(Duration::from_millis(12));
    let fifth_short_tick_changed = store.tick(Duration::from_millis(12));
    let suppressed_phase = store
        .nodes
        .get(&node_id)
        .expect("node_id 应在五个短 tick 后仍存在")
        .breathing_phase;
    let sixth_short_tick_changed = store.tick(Duration::from_millis(12));
    let final_phase = store
        .nodes
        .get(&node_id)
        .expect("node_id 应在第六个短 tick 后仍存在")
        .breathing_phase;

    assert!(
        fade_in_changed,
        "tick(150ms) 应完成 breathing 淡入并触发首次 repaint"
    );
    assert!(
        approx(phase_after_fade, 0.06),
        "Cool 周期 2500ms 下 tick(150ms) 后 phase 应为 0.06，实际 {}",
        phase_after_fade
    );
    assert!(
        !first_short_tick_changed,
        "淡入完成后的第 1 个 12ms tick 不应触发 repaint，否则仍是约 83fps"
    );
    assert!(
        !second_short_tick_changed,
        "淡入完成后的第 2 个 12ms tick 累计 24ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        !third_short_tick_changed,
        "淡入完成后的第 3 个 12ms tick 累计 36ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        !fourth_short_tick_changed,
        "淡入完成后的第 4 个 12ms tick 累计 48ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        !fifth_short_tick_changed,
        "淡入完成后的第 5 个 12ms tick 累计 60ms < 67ms，仍不应触发 repaint"
    );
    assert!(
        approx(suppressed_phase, 0.084),
        "节流期间 breathing_phase 应推进到 0.084，实际 {}",
        suppressed_phase
    );
    assert!(
        sixth_short_tick_changed,
        "淡入完成后的第 6 个 12ms tick 累计 72ms >= 67ms，应触发下一帧 repaint"
    );
    assert!(
        approx(final_phase, 0.0888),
        "节流不应冻结动画时间；最终 breathing_phase 应为 0.0888，实际 {}",
        final_phase
    );
    assert!(
        !approx(final_phase, 0.06),
        "breathing_phase 不应停在淡入完成时的 0.06，否则呼吸动画会卡住"
    );
}

/// MWS-2C: motion diagnostics 应区分 breathing 已可见与正在节流等待的节点数。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 53（任意稳定 id，与 MWS-2B 区分）。
///   Cool 呼吸周期 = 2500ms，breathing_fade token = 150ms。
///   tick(150ms) 后 fade.value = 1.0，breathing_visible_nodes 应为 1。
///   repaint 预算 = 67ms；随后 tick(12ms)：
///     breathing_repaint_elapsed = 12ms < 67ms，节点处于 waiting 状态。
///   因为 12ms 没达到 repaint 预算，tick 返回 false，但 diagnostics 应能暴露等待状态。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ensure_node 创建空闲节点。
///   2. set_breathing_intent(Cool) 并 tick(150ms)，让 breathing 完全可见。
///   3. 再 tick(12ms)，制造一个被节流的 waiting 样本。
///   4. 调用 diagnostics 读取 breathing_active/visible/waiting 计数。
///
/// 预期结果：
///   - 正断言：breathing_active_nodes=1，说明节点仍处于呼吸动画状态。
///   - 正断言：breathing_visible_nodes=1，说明 fade 已完成且用户能看到呼吸边框。
///   - 正断言：breathing_waiting_nodes=1，说明下一帧尚未到 67ms repaint 预算。
///   - 负断言：chase_active_nodes=0，避免把 breathing 掉帧误判成 snake chase。
#[test]
fn test_motion_diagnostics_counts_breathing_visible_and_waiting_nodes() {
    let mut store = MotionStore::new(false);
    let node_id = 53;
    let node_rect = rect(0.0, 0.0, 190.0, 254.0);
    store.ensure_node(node_id, node_rect);

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    assert!(
        store.tick(Duration::from_millis(150)),
        "tick(150ms) 应完成 breathing 淡入并触发首次 repaint"
    );
    assert!(
        !store.tick(Duration::from_millis(12)),
        "淡入完成后的 12ms tick 应被 67ms repaint 预算节流"
    );
    let diagnostics = store.diagnostics();

    assert_eq!(
        diagnostics.breathing_active_nodes, 1,
        "breathing intent 仍为 Cool，active breathing 节点数应为 1"
    );
    assert_eq!(
        diagnostics.breathing_visible_nodes, 1,
        "fade 完成后 breathing_visible_nodes 应为 1，说明动画已可见"
    );
    assert_eq!(
        diagnostics.breathing_waiting_nodes, 1,
        "12ms < 67ms 时 breathing_waiting_nodes 应为 1，便于定位节流等待状态"
    );
    assert_eq!(
        diagnostics.chase_active_nodes, 0,
        "本用例未开启 chase，不能把 breathing repaint 误归因到 working snake"
    );
}

/// MWS-3: Agent 工作态应只绘制 snake overlay，不改变卡片自身颜色、阴影或边框强调。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 33，初始 Idle，theme.node_stroke = 0xffffff24。
///   chase_fade token = 150ms，set_chase_intent(true) 后 tick(150ms)：
///     chase_fade.value = 1.0，chase_phase = 150ms / 1200ms = 0.125。
///   工作态 snake 只应影响 working_snake overlay：
///     stroke_color       = theme.node_stroke
///     edge_highlight     = theme.edge_highlight with hover/selection emphasis only（本用例为 Idle）
///     shadow_color alpha = theme.shadow_color alpha，不叠加 running alpha
///     shadow_blur        = theme.shadow_blur
///     border_emphasized  = false（没有选中/聚焦/完成态）
///
/// 执行过程：
///   1. ensure_node 创建空闲节点
///   2. set_chase_intent(true) 模拟 Agent Thinking/RunningTool
///   3. tick(150ms) 让工作态淡入完成
///   4. resolve_node_visual 读取卡片 chrome 与 snake overlay 数据
///
/// 预期结果：
///   - 正断言：working_snake 存在，保留贪吃蛇逻辑和动画
///   - 正断言：stroke_color 等于静态 node_stroke，不再让整张卡片变色
///   - 正断言：edge_highlight_color 等于静态 edge highlight
///   - 正断言：shadow_color / shadow_y / shadow_blur / shadow_spread 等于主题基线
///   - 负断言：border_is_emphasized=false，运行态不应把卡片边框变粗
///   - 负断言：priority 不应伪装成 Focused，工作态不是输入焦点
#[test]
fn test_working_state_keeps_card_chrome_static_while_snake_animates() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 33;
    let node_rect = rect(0.0, 0.0, 100.0, 80.0);
    store.ensure_node(node_id, node_rect);

    store.set_chase_intent(node_id, true);
    store.tick(Duration::from_millis(150));
    let visual = store.resolve_node_visual(node_id, node_rect, theme);

    assert!(
        visual.working_snake.is_some(),
        "Agent 工作态应保留 working_snake overlay，不能为了性能直接撤销动画"
    );
    assert_eq!(
        visual.stroke_color, theme.node_stroke,
        "Agent 工作态不应改变卡片自身 stroke_color，否则整张卡片会持续变色"
    );
    assert_eq!(
        visual.edge_highlight_color, theme.edge_highlight,
        "Agent 工作态不应改变卡片自身 edge_highlight_color"
    );
    assert_eq!(
        visual.shadow_color, theme.shadow_color,
        "Agent 工作态不应叠加卡片自身 shadow alpha"
    );
    assert_eq!(
        visual.shadow_y, theme.shadow_y,
        "Agent 工作态不应改变卡片自身 shadow_y"
    );
    assert_eq!(
        visual.shadow_blur, theme.shadow_blur,
        "Agent 工作态不应改变卡片自身 shadow_blur"
    );
    assert_eq!(
        visual.shadow_spread, theme.shadow_spread,
        "Agent 工作态不应改变卡片自身 shadow_spread"
    );
    assert!(
        !visual.border_is_emphasized,
        "Agent 工作态不应把卡片自身 border 加粗；snake overlay 已表达运行态"
    );
    assert_ne!(
        visual.priority,
        VisualPriority::Focused,
        "Agent 工作态不应伪装成 Focused；真实输入焦点仍应由 selection/focus 控制"
    );
}

/// MWS-4: motion 诊断应能数出正在运行的 working snake 节点。
///
/// 数据构造（含关键数值的推导过程）：
///   节点 A bounds=(0,0,320,220)，set_chase_intent(true) 模拟 Agent Thinking。
///   节点 B bounds=(360,0,320,220)，保持 chase inactive，作为负样本。
///   tick(150ms) = chase_fade token 150ms → A 的 working snake 完成淡入并可见。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 MotionStore 并 ensure A/B 两个节点。
///   2. 只对 A 调用 set_chase_intent(true)。
///   3. tick(150ms) 推进到 snake 可见状态。
///   4. 调用 diagnostics 读取节点数与 snake 计数。
///
/// 预期结果：
///   - 正断言：diagnostics.nodes=2，确认两个节点都被纳入采样。
///   - 正断言：chase_active_nodes=1，只统计 A。
///   - 正断言：chase_visible_nodes=1，A 淡入后应被日志标记为可见 snake。
///   - 负断言：chase_active_nodes 不应为 2，否则普通节点会污染掉帧分析。
#[test]
fn test_motion_diagnostics_counts_active_working_snake_nodes() {
    let theme = Theme::dark();
    let mut store = MotionStore::new(false);
    let active_id: NodeId = 42;
    let idle_id: NodeId = 43;
    let active_rect = CanvasRect::new(0.0, 0.0, 320.0, 220.0);
    let idle_rect = CanvasRect::new(360.0, 0.0, 320.0, 220.0);

    store.ensure_node(active_id, active_rect);
    store.ensure_node(idle_id, idle_rect);
    store.set_chase_intent(active_id, true);
    store.tick(Duration::from_millis(150));
    let active_visual = store.resolve_node_visual(active_id, active_rect, theme);
    let diagnostics = store.diagnostics();

    assert!(
        active_visual.working_snake.is_some(),
        "active_id tick(150ms) 后应已生成 working_snake，测试样本才有效"
    );
    assert_eq!(
        diagnostics.nodes, 2,
        "diagnostics 应采样两个 motion 节点，不能漏掉 idle 负样本"
    );
    assert_eq!(
        diagnostics.chase_active_nodes, 1,
        "只有 active_id 调用了 set_chase_intent(true)，active snake 数应为 1"
    );
    assert_eq!(
        diagnostics.chase_visible_nodes, 1,
        "tick(150ms) 后 active_id snake 已淡入，应计为 visible snake"
    );
    assert_ne!(
        diagnostics.chase_active_nodes, 2,
        "idle_id 未开启 chase，不应被计入 active snake，否则日志会误判负载来源"
    );
}
