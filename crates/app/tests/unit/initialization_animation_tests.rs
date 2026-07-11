use super::*;

fn approximately(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.0001
}

/// IA-1：自然播放严格遵循 32 帧 × 31ms，并在最后 6 帧交叉淡入真实内容。
///
/// 数据构造（含关键数值推导）：
///   frame_interval = 31ms；frame_count = 32；总时长 = 32 × 31 = 992ms。
///   交叉淡入从第 26 帧开始：26 × 31 = 806ms；剩余 6 × 31 = 186ms。
///
/// 执行过程：
///   1. 创建 Terminal 初始化动画并读取 0ms 的首帧。
///   2. 推进到 806ms，读取交叉淡入起点。
///   3. 再推进 93ms（186ms 的一半），读取两个图层的半透明状态。
///   4. 推进剩余 93ms，确认动画结束。
///
/// 预期结果：
///   - 正断言：0ms 为第 0 帧，动画全不透明，真实内容全透明。
///   - 正断言：806ms 为第 26 帧，交叉淡入尚未开始。
///   - 正断言：899ms 时两个图层均为 0.5。
///   - 负断言：992ms 后不应继续暴露初始化画面。
#[test]
fn test_initialization_animation_uses_native_32_frame_timeline_and_crossfade() {
    let mut animation = InitializationAnimation::new(InitializationAnimationKind::Terminal);

    let start = animation
        .presentation()
        .expect("刚创建的初始化动画应暴露第 0 帧");
    assert_eq!(
        start.frame_index, 0,
        "0ms 应显示原始序列的第 0 帧，而不是跳到中间帧"
    );
    assert!(
        approximately(start.overlay_opacity, 1.0),
        "0ms 时初始化画面应完全不透明，实际为 {}",
        start.overlay_opacity
    );
    assert!(
        approximately(start.content_opacity, 0.0),
        "0ms 时真实终端内容应完全被遮住，实际为 {}",
        start.content_opacity
    );

    assert!(
        animation.tick(INITIALIZATION_CROSSFADE_START),
        "推进到 806ms 仍在初始化期间，应报告视觉变化"
    );
    let crossfade_start = animation
        .presentation()
        .expect("806ms 时动画应仍有可绘制画面");
    assert_eq!(
        crossfade_start.frame_index, 26,
        "806ms = 26 × 31ms，应从第 26 帧开始交叉淡入"
    );
    assert!(
        approximately(crossfade_start.overlay_opacity, 1.0),
        "交叉淡入起点的初始化画面仍应全不透明"
    );
    assert!(
        approximately(crossfade_start.content_opacity, 0.0),
        "交叉淡入起点的真实内容仍应全透明"
    );

    assert!(
        animation.tick(INITIALIZATION_CROSSFADE_DURATION / 2),
        "交叉淡入中点仍应报告视觉变化"
    );
    let midpoint = animation
        .presentation()
        .expect("交叉淡入中点应保留初始化画面");
    assert!(
        approximately(midpoint.overlay_opacity, 0.5),
        "93ms 是 186ms 的一半，初始化层透明度应为 0.5，实际为 {}",
        midpoint.overlay_opacity
    );
    assert!(
        approximately(midpoint.content_opacity, 0.5),
        "93ms 是 186ms 的一半，真实内容透明度应为 0.5，实际为 {}",
        midpoint.content_opacity
    );

    assert!(
        animation.tick(INITIALIZATION_CROSSFADE_DURATION / 2),
        "推进到 992ms 的收尾帧应报告一次视觉变化"
    );
    assert!(
        animation.presentation().is_none(),
        "总时长 992ms 后不应继续暴露初始化画面"
    );
}

/// IA-2：用户输入可从任意正常播放时点在 80ms 内快速接管真实内容。
///
/// 数据构造（含关键数值推导）：
///   正常播放先推进 155ms = 5 × 31ms；快速退出持续 80ms。
///   输入前 overlay=1、content=0；40ms 为快速退出的一半，故两层均应为 0.5。
///
/// 执行过程：
///   1. 创建 Agent 初始化动画并推进到第 5 帧。
///   2. 调用 interrupt()，再分别推进 40ms 与 40ms。
///   3. 读取中点和终点的 presentation。
///
/// 预期结果：
///   - 正断言：中点同时显示初始化层和真实内容。
///   - 正断言：80ms 后初始化层结束。
///   - 负断言：被打断后不应回到自然播放的后续帧。
#[test]
fn test_initialization_animation_interrupts_into_real_content_within_80ms() {
    let mut animation = InitializationAnimation::new(InitializationAnimationKind::Agent);
    animation.tick(INITIALIZATION_FRAME_INTERVAL * 5);
    assert!(
        animation.interrupt(),
        "活动中的初始化动画应可被用户输入打断"
    );

    assert!(
        animation.tick(INITIALIZATION_INTERRUPT_DURATION / 2),
        "快速退出中点应报告视觉变化"
    );
    let midpoint = animation
        .presentation()
        .expect("快速退出中点仍应保留淡出的初始化画面");
    assert!(
        approximately(midpoint.overlay_opacity, 0.5),
        "80ms 快速退出的 40ms 中点，初始化层应为 0.5，实际为 {}",
        midpoint.overlay_opacity
    );
    assert!(
        approximately(midpoint.content_opacity, 0.5),
        "80ms 快速退出的 40ms 中点，真实内容应为 0.5，实际为 {}",
        midpoint.content_opacity
    );

    assert!(
        animation.tick(INITIALIZATION_INTERRUPT_DURATION / 2),
        "快速退出收尾应报告视觉变化"
    );
    assert!(
        animation.presentation().is_none(),
        "输入后的 80ms 已结束，初始化画面不应重新进入自然播放"
    );
}

/// IA-3：MotionStore 只为显式新建路径登记初始化动画，并在节点清理后丢弃状态。
///
/// 数据构造（含关键数值推导）：
///   node_id = 41；初始 rect = 640×360；一帧间隔 = 31ms。
///   retain_nodes([]) 代表 document 已不再包含该节点。
///
/// 执行过程：
///   1. 先 spawn_node，再显式 start_initialization(Terminal)。
///   2. tick(31ms) 验证推进到第 1 帧。
///   3. retain_nodes([]) 模拟关闭节点后的 render culling 清理。
///
/// 预期结果：
///   - 正断言：登记后可读取 Terminal 的第 0 帧。
///   - 正断言：31ms 后帧号推进到 1。
///   - 负断言：节点被清理后不得残留初始化状态，避免 restored/复用 id 时误重播。
#[test]
fn test_motion_store_tracks_explicit_initialization_and_cleans_removed_nodes() {
    let mut store = MotionStore::new(false);
    let node_id = 41;
    let restored_node_id = 42;
    store.ensure_node(restored_node_id, CanvasRect::new(0.0, 0.0, 640.0, 360.0));
    assert!(
        store
            .initialization_presentation(restored_node_id)
            .is_none(),
        "仅被 ensure_node 的恢复节点不应凭空登记初始化动画"
    );
    store.spawn_node(node_id, CanvasRect::new(0.0, 0.0, 640.0, 360.0));
    store.start_initialization(node_id, InitializationAnimationKind::Terminal);

    let start = store
        .initialization_presentation(node_id)
        .expect("显式新建 Terminal 后应登记初始化画面");
    assert_eq!(
        start.kind,
        InitializationAnimationKind::Terminal,
        "Terminal 新建路径应使用 Terminal 初始化资源"
    );
    assert_eq!(start.frame_index, 0, "刚登记时应从第 0 帧开始");

    assert!(
        store.tick(INITIALIZATION_FRAME_INTERVAL),
        "活动初始化动画推进一帧时应请求重绘"
    );
    let advanced = store
        .initialization_presentation(node_id)
        .expect("31ms 后初始化画面仍应存在");
    assert_eq!(
        advanced.frame_index, 1,
        "31ms 恰为一帧，帧号应从 0 推进到 1"
    );

    store.retain_nodes(std::iter::empty());
    assert!(
        store.initialization_presentation(node_id).is_none(),
        "已从 document 移除的节点不应残留初始化状态，避免后续恢复时误重播"
    );
}

/// IA-4：初始化动画不受既有 reduce_motion 标志影响，仍按完整 31ms 帧间隔播放。
///
/// 数据构造（含关键数值推导）：
///   MotionStore(reduce_motion=true)；Terminal 动画首帧为 0，31ms 后为 1。
///   这是本需求明确约定的例外：其他节点视觉可降级，但新卡片初始化不得跳帧。
///
/// 执行过程：
///   1. 用 reduce_motion=true 创建 MotionStore 并登记 Terminal 初始化。
///   2. 推进 31ms，读取 initialization presentation。
///
/// 预期结果：
///   - 正断言：31ms 后仍有动画且帧号为 1。
///   - 负断言：不应因 reduce_motion 把初始化状态直接清空或停留在第 0 帧。
#[test]
fn test_initialization_animation_ignores_reduce_motion_preference() {
    let mut store = MotionStore::new(true);
    let node_id = 43;
    store.spawn_node(node_id, CanvasRect::new(0.0, 0.0, 640.0, 360.0));
    store.start_initialization(node_id, InitializationAnimationKind::Terminal);

    assert!(
        store.tick(INITIALIZATION_FRAME_INTERVAL),
        "reduce_motion=true 时初始化动画推进一帧仍应请求重绘"
    );
    let presentation = store
        .initialization_presentation(node_id)
        .expect("reduce_motion=true 不应清空初始化画面");
    assert_eq!(
        presentation.frame_index, 1,
        "reduce_motion=true 时 31ms 后仍应显示第 1 帧"
    );
    assert_ne!(
        presentation.frame_index, 0,
        "reduce_motion=true 时初始化动画不应卡在第 0 帧"
    );
}

/// IA-5：生产帧循环在 12ms 常规重绘之间缩短最后一段，精确命中 31ms 帧边界。
///
/// 数据构造（含关键数值推导）：
///   常规重绘间隔为 12ms；在 24ms 后距首个 31ms 边界只剩 7ms。
///   该规则重复适用于 26 × 31 = 806ms 的交叉淡入起点和 32 × 31 = 992ms 终点。
///
/// 执行过程：
///   1. 启动 Terminal 初始化动画并先推进两次常规 12ms tick。
///   2. 查询下一次生产重绘间隔，推进到 31ms，并持续按推荐间隔推进。
///   3. 分别读取 806ms 与 992ms 的 presentation。
///
/// 预期结果：
///   - 正断言：24ms 后下一次 tick 必为 7ms，避免被量化到 36ms。
///   - 正断言：806ms 仍是第 26 帧且真实内容尚未透出。
///   - 负断言：992ms 后不得残留覆盖层。
#[test]
fn test_initialization_scheduler_hits_31ms_806ms_and_992ms_boundaries_exactly() {
    let mut store = MotionStore::new(false);
    let node_id = 44;
    let default_repaint = Duration::from_millis(12);
    let mut elapsed = Duration::ZERO;
    store.spawn_node(node_id, CanvasRect::new(0.0, 0.0, 640.0, 360.0));
    store.start_initialization(node_id, InitializationAnimationKind::Terminal);

    store.tick(default_repaint);
    store.tick(default_repaint);
    elapsed += default_repaint * 2;
    assert_eq!(
        store.next_animation_tick_interval(default_repaint),
        Duration::from_millis(7),
        "24ms 后距 31ms 首帧边界只剩 7ms，生产循环不得继续等待完整 12ms"
    );

    while elapsed < INITIALIZATION_CROSSFADE_START {
        let step = store.next_animation_tick_interval(default_repaint);
        assert!(
            step > Duration::ZERO,
            "初始化动画活动期间调度器必须返回正时长，避免生产帧循环忙等"
        );
        store.tick(step);
        elapsed += step;
    }
    assert_eq!(
        elapsed, INITIALIZATION_CROSSFADE_START,
        "调度器应精确停在 26 × 31 = 806ms 的交叉淡入起点"
    );
    let crossfade = store
        .initialization_presentation(node_id)
        .expect("806ms 时初始化动画仍应可见");
    assert_eq!(
        crossfade.frame_index, 26,
        "806ms 应显示第 26 帧，而不是被 12ms tick 推迟到下一帧"
    );
    assert!(
        approximately(crossfade.content_opacity, 0.0),
        "806ms 交叉淡入起点真实内容应仍为全透明，实际为 {}",
        crossfade.content_opacity
    );

    while elapsed < INITIALIZATION_TOTAL_DURATION {
        let step = store.next_animation_tick_interval(default_repaint);
        store.tick(step);
        elapsed += step;
    }
    assert_eq!(
        elapsed, INITIALIZATION_TOTAL_DURATION,
        "调度器应精确停在 32 × 31 = 992ms 的自然结束点"
    );
    assert!(
        store.initialization_presentation(node_id).is_none(),
        "992ms 后初始化覆盖层必须被移除，不能因 12ms 量化多保留一帧"
    );
}
