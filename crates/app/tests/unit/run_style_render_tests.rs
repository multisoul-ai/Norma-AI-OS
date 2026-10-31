//! 各风格的图元：一道光、灵光、磁流体与减少动态效果的回落。辅助函数来自上层测试模块。

use super::*;

/// 一道光：思考光段 2.4s 往返；工具光段滑到工具名下方；完成铺满单调；减少动态效果为整条线。
///
/// 数据：线段 x0=57, x1=675（长 618）；思考 t=0 时位置比例 0、t=1200 时 1。
/// 预期：思考 1200ms 时光段中心在右端附近；工具 1000ms 后光段中心 ≈ x0+618*0.88；
///       完成 0/300/600ms 的线长递增，600ms 达到满长；reduce 下思考是一条整线（1 个图元）。
#[test]
fn underline_positions_and_reduce_motion() {
    let p = palette();
    let f = frame();
    let center = |prims: &[RunPrim]| match prims {
        [RunPrim::Bar { x1, .. }, ..] => *x1,
        other => panic!("光段应由两段渐变条组成: {other:?}"),
    };
    let mut motion = RunMotion::new(RunStyle::Underline, input(RunState::Thinking), &p);
    run(&mut motion, RunStyle::Underline, 1200.0);
    let prims = build(RunStyle::Underline, &motion, &f, false);
    assert!(
        (center(&prims) - 675.0).abs() < 3.0,
        "1200ms（半周期）光段在右端: {}",
        center(&prims)
    );
    assert_eq!(
        build(RunStyle::Underline, &motion, &f, true).len(),
        1,
        "减少动态效果：整条线一个图元"
    );
    motion.set_input(RunInput {
        state: RunState::Tool,
        viewed: false,
        tool_key: 7,
    });
    run(&mut motion, RunStyle::Underline, 1000.0);
    let prims = build(RunStyle::Underline, &motion, &f, false);
    assert!(
        (center(&prims) - (57.0 + 618.0 * 0.88)).abs() < 3.0,
        "工具光段停在工具名下方: {}",
        center(&prims)
    );
    motion.set_input(input(RunState::Done));
    let mut lengths = Vec::new();
    for _ in 0..3 {
        match build(RunStyle::Underline, &motion, &f, false).as_slice() {
            [RunPrim::Bar { x0, x1, .. }] => lengths.push(x1 - x0),
            other => panic!("完成应是一条线: {other:?}"),
        }
        run(&mut motion, RunStyle::Underline, 300.0);
    }
    assert!(
        lengths[0] < lengths[1] && lengths[1] < lengths[2],
        "铺满进度单调递增: {lengths:?}"
    );
    assert!((lengths[2] - 618.0).abs() < 0.5, "600ms 铺满整条线");
}

/// 一道光的线宽随缩放变化，并夹在 1..2.5px。
///
/// 数据：zoom=0.5 → 1.6*0.5=0.8 夹到 1.0；zoom=1.25 → 2.0。
#[test]
fn underline_thickness_scales_with_zoom() {
    let p = palette();
    let mut motion = RunMotion::new(RunStyle::Underline, input(RunState::Failed), &p);
    run(&mut motion, RunStyle::Underline, 500.0);
    for (zoom, expected) in [(0.5, 1.0), (1.25, 2.0)] {
        let f = CardFrame { zoom, ..frame() };
        match build(RunStyle::Underline, &motion, &f, false).as_slice() {
            [RunPrim::Bar { height, .. }] => assert!(
                (height - expected).abs() < 1e-4,
                "zoom={zoom} 线宽应为 {expected}"
            ),
            other => panic!("失败应是一条线: {other:?}"),
        }
    }
}

/// 灵光：图元数上限；工具亮点沿周长前进；失败闪烁序列确定。
///
/// 数据：24 段 × 4 色带 = 96 ≤ 160；工具 ping 后 100ms 与 300ms 的最亮段。
/// 预期：最亮段位置前进（u 增大，按 1/650 每毫秒）；同一状态序列两次构建结果完全相同。
#[test]
fn aura_budget_tool_light_and_determinism() {
    let p = palette();
    let f = frame();
    let mut motion = RunMotion::new(RunStyle::Aura, input(RunState::Thinking), &p);
    run(&mut motion, RunStyle::Aura, 500.0);
    motion.set_input(RunInput {
        state: RunState::Tool,
        viewed: false,
        tool_key: 9,
    });
    let brightest = |motion: &RunMotion| {
        build(RunStyle::Aura, motion, &f, false)
            .iter()
            .enumerate()
            .filter_map(|(i, prim)| match prim {
                RunPrim::Fill {
                    fill: RunFill::Solid(color),
                    ..
                } if i % 4 == 0 => Some((i / 4, color & 0xff)),
                _ => None,
            })
            .max_by_key(|(_, alpha)| *alpha)
            .map(|(segment, _)| segment)
            .unwrap()
    };
    run(&mut motion, RunStyle::Aura, 100.0);
    let early = brightest(&motion);
    run(&mut motion, RunStyle::Aura, 200.0);
    let late = brightest(&motion);
    assert!(late > early, "工具亮点沿周长前进：{early} → {late}");
    let prims = build(RunStyle::Aura, &motion, &f, false);
    assert!(
        prims.len() <= MAX_PRIMS && prims.len() <= aura::SEGMENTS * 4,
        "图元数 {} 超预算",
        prims.len()
    );

    let failed = |ms: f32| {
        let mut m = RunMotion::new(RunStyle::Aura, input(RunState::Thinking), &p);
        run(&mut m, RunStyle::Aura, 400.0);
        m.set_input(input(RunState::Failed));
        run(&mut m, RunStyle::Aura, ms);
        build(RunStyle::Aura, &m, &f, false)
    };
    assert_eq!(
        failed(330.0),
        failed(330.0),
        "失败闪烁由时间片 hash 决定，可复现"
    );
}

/// 磁流体：膜偏移非负且不超留白；等待审批的液滴在底边中点；滴落有上限；结果确定。
///
/// 数据：240 个采样点。Approval 推进 1000ms；Failed 推进 5000ms（el*0.035 远超 16）。
/// 预期：所有偏移 ∈ [0, 52]，52 < margin 56；Approval 时最大偏移的采样点离底边中点
///       环形距离 < 0.03；Failed 底边最大偏移 ≤ 16 + 起伏(≤ 2.4+1.6*1.5) + 3。
#[test]
fn ferrofluid_offsets_drop_and_drip_limits() {
    let p = palette();
    let f = frame();
    let base: Vec<_> = (0..ferrofluid::SAMPLES)
        .map(|i| f.edge(i as f32 / ferrofluid::SAMPLES as f32))
        .collect();
    assert!(
        ferrofluid::MAX_REACH < RunStyle::Ferrofluid.margin(),
        "膜的最远距离必须在留白之内"
    );
    let mut motion = RunMotion::new(RunStyle::Ferrofluid, input(RunState::Tool), &p);
    run(&mut motion, RunStyle::Ferrofluid, 800.0);
    let offsets = ferrofluid::offsets(&motion, &f, &base);
    assert!(
        offsets
            .iter()
            .all(|d| (0.0..=ferrofluid::MAX_REACH).contains(d)),
        "工具调用时偏移在 [0, 52]"
    );

    motion.set_input(input(RunState::Approval));
    run(&mut motion, RunStyle::Ferrofluid, 1000.0);
    let offsets = ferrofluid::offsets(&motion, &f, &base);
    let (peak, _) = offsets
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap();
    let u = peak as f32 / ferrofluid::SAMPLES as f32;
    assert!(
        ring_distance(u, f.bottom_center_u()) < 0.03,
        "液滴应在底边中点，实际 u={u}"
    );

    motion.set_input(input(RunState::Failed));
    run(&mut motion, RunStyle::Ferrofluid, 5000.0);
    let offsets = ferrofluid::offsets(&motion, &f, &base);
    let max = offsets.iter().cloned().fold(0.0, f32::max);
    // 滴落上限 16 + 基础起伏 2.4 + 1.6*(0.6+0.4+0.5) + 下垂肩部 3 = 23.8
    assert!(max <= 23.8 + 0.01, "滴落长度有上限，实际最大偏移 {max}");
    assert_eq!(
        offsets,
        ferrofluid::offsets(&motion, &f, &base),
        "同一状态输出一致"
    );
    assert!(
        build(RunStyle::Ferrofluid, &motion, &f, false).len() <= MAX_PRIMS,
        "图元数在预算内"
    );
}

/// 磁流体：各状态的膜（含尖刺、工具切换脉冲与液滴）连同外描边半宽都落在 24px 布局间距内，
/// 不会钻到相邻卡片下方被挡住。
///
/// 数据：zoom=1 的卡片；思考、工具（从切换瞬间起）、等待、完成各逐帧推进 3000ms。
/// 预期：每帧最大偏移 + 外描边半宽 3 ≤ 24。
#[test]
fn ferrofluid_reach_stays_inside_layout_gap() {
    let p = palette();
    let f = frame();
    let base: Vec<_> = (0..ferrofluid::SAMPLES)
        .map(|i| f.edge(i as f32 / ferrofluid::SAMPLES as f32))
        .collect();
    for state in [
        RunState::Thinking,
        RunState::Tool,
        RunState::Approval,
        RunState::Done,
    ] {
        let mut motion = RunMotion::new(RunStyle::Ferrofluid, input(state), &p);
        for _ in 0..(3000 / 16) {
            run(&mut motion, RunStyle::Ferrofluid, 16.0);
            let max = ferrofluid::offsets(&motion, &f, &base)
                .into_iter()
                .fold(0.0, f32::max);
            assert!(
                max + 3.0 <= 24.0,
                "{state:?} 膜外扩 {max} 加描边超出 24px 布局间距"
            );
        }
    }
}

/// 减少动态效果：灵光、磁流体回落为卡片底边外的 3px 状态色细条；准备状态不画。
///
/// 数据：frame() 卡片底边 y = 56 + 420 = 476。
/// 执行过程：思考状态 tick 一次（减少动态效果直接落到目标值）后构建图元。
/// 预期：思考时 1 个 Bar，高 3px，顶边在 476 + 2 = 478（卡片外、布局间距内，
///       否则会被画在其上的卡片正文挡住）；准备时为空。
#[test]
fn reduce_motion_falls_back_to_bottom_bar() {
    let p = palette();
    let f = frame();
    for style in [RunStyle::Aura, RunStyle::Ferrofluid] {
        let mut motion = RunMotion::new(style, input(RunState::Thinking), &p);
        motion.tick(16.0, style, &p, true);
        match build(style, &motion, &f, true).as_slice() {
            [RunPrim::Bar { y, height, .. }] => {
                assert_eq!(*height, 3.0, "{style:?} 细条高 3px");
                assert!(
                    (y - (f.y + f.h + 2.0)).abs() < 1e-4,
                    "{style:?} 细条在卡片底边外 2px，实际 y={y}"
                );
                assert!(
                    y + height <= f.y + f.h + 24.0,
                    "{style:?} 细条不能超出 24px 布局间距"
                );
            }
            other => panic!("{style:?} 减少动态效果应只有一条细条: {other:?}"),
        }
        let ready = RunMotion::new(style, input(RunState::Ready), &p);
        assert!(
            build(style, &ready, &f, true).is_empty(),
            "{style:?} 准备状态不画细条"
        );
    }
}
