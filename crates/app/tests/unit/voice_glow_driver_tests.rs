use super::*;
use std::time::Duration;

fn prefs() -> VoiceGlowPrefs {
    VoiceGlowPrefs {
        style: VoiceGlowStyle::Halo,
        ..VoiceGlowPrefs::default()
    }
}

fn dock() -> Rect {
    Rect {
        x: 300.0,
        y: 626.0,
        w: 600.0,
        h: 52.0,
    }
}

fn tick(driver: &mut GlowDriver, speaking: Option<(f32, f32)>) {
    driver.tick(
        Duration::from_millis(16),
        speaking,
        prefs(),
        (1200.0, 700.0),
        dock(),
    );
}

/// 场景：思考之后进入回复，回复结束淡出时画面不能退回思考形态（交叉审查发现）。
/// 数据构造：外部相位先 Thinking，再 Idle；回复强度 1.0 → 0.5（淡出中）。
/// 执行过程：依次 tick 并读取展示相位。
/// 预期结果：思考时为 Thinking；回复开始后为 Speaking；外部相位回到 Idle、回复淡出时仍为 Speaking。
#[test]
fn reply_fade_out_keeps_speaking_shape_after_thinking() {
    let mut driver = GlowDriver::default();
    driver.set_phase(VoiceGlowPhase::Thinking);
    tick(&mut driver, None);
    assert_eq!(
        driver.shown_phase(),
        VoiceGlowPhase::Thinking,
        "思考中应展示思考形态"
    );
    tick(&mut driver, Some((1.0, 0.6)));
    assert_eq!(
        driver.shown_phase(),
        VoiceGlowPhase::Speaking,
        "回复开始后应展示回复形态"
    );
    driver.set_phase(VoiceGlowPhase::Idle);
    tick(&mut driver, Some((0.5, 0.2)));
    assert_eq!(
        driver.shown_phase(),
        VoiceGlowPhase::Speaking,
        "回复淡出期间不能跳回思考形态"
    );
}

/// 场景：外部相位回到空闲后，强度在 220ms 内淡出为 0，活动标志随之结束。
/// 数据构造：聆听 3 帧后切空闲，再 tick 20 帧（320ms）。
/// 预期结果：聆听时活动；320ms 后不再活动，避免帧驱动停不下来。
#[test]
fn glow_driver_goes_inactive_after_fade_out() {
    let mut driver = GlowDriver::default();
    driver.set_phase(VoiceGlowPhase::Listening);
    for _ in 0..3 {
        tick(&mut driver, None);
    }
    assert!(driver.is_active(), "聆听时应处于活动状态");
    driver.set_phase(VoiceGlowPhase::Idle);
    for _ in 0..20 {
        tick(&mut driver, None);
    }
    assert!(!driver.is_active(), "淡出 320ms 后应停止活动");
}

/// 场景：只有回复、没有聆听/思考时，首次 tick 也必须读取系统「减少动态效果」。
/// 数据构造：全新驱动，外部相位始终为空闲；第一帧 tick。
/// 预期结果：tick 后已读取过系统设置。
#[test]
fn first_tick_reads_system_reduce_motion() {
    let mut driver = GlowDriver::default();
    assert!(!driver.reduce_motion_read, "新驱动尚未读取");
    tick(&mut driver, Some((1.0, 0.5)));
    assert!(driver.reduce_motion_read, "首帧应读取系统减少动态效果");
}
