use super::*;

fn palette() -> RunPalette {
    RunPalette::from_theme(Theme::default())
}

/// 时间轴定位：自动轮播各段首尾。
///
/// 数据：HERO_CYCLE = 准备 1200 / 思考 3000 / 工具 3800 / 等你 3000 / 完成 2800 / 失败 2600。
/// 预期：t=0 → (0, 0)；t=1200 → 思考段起点 (1, 0)；t=16399（总长 16400 前 1ms）→ (5, 2599)。
#[test]
fn position_walks_the_hero_cycle() {
    assert_eq!(position(&HERO_CYCLE, 0.0), (0, 0.0), "起点在准备段");
    assert_eq!(position(&HERO_CYCLE, 1200.0), (1, 0.0), "1200ms 进入思考段");
    let (index, into) = position(&HERO_CYCLE, 16_399.0);
    assert_eq!(index, 5, "最后 1ms 仍在失败段");
    assert!((into - 2599.0).abs() < 0.01, "失败段内 2599ms，实际 {into}");
}

/// 回放到某一刻：状态正确、结果确定；第二轮从头开始。
///
/// 数据：t=5000 落在工具段（1200+3000=4200 ≤ 5000 < 8000）；t=16400+5000 是第二轮同一位置。
/// 预期：状态为 Tool；两次回放相同；第二轮与第一轮相同（每轮从准备重放）。
#[test]
fn simulate_is_deterministic_and_loops() {
    let p = palette();
    let first = simulate(RunStyle::Aura, &p, &HERO_CYCLE, 5000.0, None, false);
    assert_eq!(first.input.state, RunState::Tool, "5000ms 在工具段");
    assert_eq!(
        first,
        simulate(RunStyle::Aura, &p, &HERO_CYCLE, 5000.0, None, false),
        "同一时刻回放结果相同"
    );
    let second_round = simulate(RunStyle::Aura, &p, &HERO_CYCLE, 21_400.0, None, false);
    assert_eq!(first, second_round, "第二轮同一位置与第一轮一致");
}

/// 点击预览标记已查看：只影响本轮完成段。
///
/// 数据：完成段 [12000, 14800)；viewed_at=12500，取 t=13000。
/// 预期：本轮 viewed=true；同一 viewed_at 在下一轮（t=16400+13000）不再生效。
#[test]
fn viewed_mark_applies_to_current_round_only() {
    let p = palette();
    let now = simulate(
        RunStyle::Underline,
        &p,
        &HERO_CYCLE,
        13_000.0,
        Some(12_500.0),
        false,
    );
    assert_eq!(now.input.state, RunState::Done, "13000ms 在完成段");
    assert!(now.input.viewed, "点击之后完成提示视为已查看");
    let next = simulate(
        RunStyle::Underline,
        &p,
        &HERO_CYCLE,
        29_400.0,
        Some(12_500.0),
        false,
    );
    assert!(!next.input.viewed, "上一轮的点击不影响下一轮");
}

/// 固定在失败：收敛后定格，与画布一样不再推进。
///
/// 数据：磁流体固定 Failed；下垂 τ=450ms 约 2.8s 收敛，取 t=5000 与 t=9000。
/// 预期：两刻运动状态完全相同（时间不再推进），图元一致；思考状态则持续变化。
#[test]
fn pinned_failed_freezes_like_the_canvas() {
    let p = palette();
    let schedule = [(RunState::Failed, f32::INFINITY)];
    let a = simulate(RunStyle::Ferrofluid, &p, &schedule, 5000.0, None, false);
    let b = simulate(RunStyle::Ferrofluid, &p, &schedule, 9000.0, None, false);
    assert_eq!(a, b, "定格后运动状态不再变化");
    assert!(!a.wants_frames(false), "定格后不需要出帧");
    let thinking = [(RunState::Thinking, f32::INFINITY)];
    let c = simulate(RunStyle::Ferrofluid, &p, &thinking, 5000.0, None, false);
    let d = simulate(RunStyle::Ferrofluid, &p, &thinking, 5100.0, None, false);
    assert_ne!(c.time, d.time, "思考状态持续推进");
}

/// 工具调用每 1800ms 换一个工具，触发一次切换反馈。
///
/// 数据：固定 Tool，t=1900 → 1800ms 处切换，since_ping≈100。
#[test]
fn pinned_tool_switches_tools_periodically() {
    let p = palette();
    let schedule = [(RunState::Tool, f32::INFINITY)];
    let m = simulate(RunStyle::Underline, &p, &schedule, 1900.0, None, false);
    assert!(
        (m.since_ping - 100.0).abs() < 34.0,
        "1800ms 处换工具，100ms 后 since_ping≈100，实际 {}",
        m.since_ping
    );
}
