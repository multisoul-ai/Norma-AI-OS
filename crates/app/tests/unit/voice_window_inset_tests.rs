use super::*;
use std::time::Duration;

/// VWI-1：实际播放开始后，语音驱动窗口内缘光应在 120ms 内平滑进入安静底光。
///
/// 数据构造（含关键数值推导）：
///   协调器初始没有待播放音频，故视觉状态必须为空。
///   产品定义淡入时长为 120ms；在 60ms 中点白光应已可见但尚未达到最终底光，
///   在第二个 60ms 后恰好累计 120ms，必须达到 12% 不透明度和 10px 内扩。
///
/// 执行过程：
///   1. 创建初始协调器并确认未显示白光。
///   2. 通知一段 AI TTS 已实际开始播放。
///   3. 推进两个 60ms 动画帧，并读取公开的可视状态。
///
/// 预期结果：
///   - 正断言：中点出现白光但弱于最终底光。
///   - 正断言：120ms 后精确达到产品定义的安静底光。
///   - 负断言：未播放时不应凭空显示白光。
#[test]
fn actual_playback_fades_window_inset_to_quiet_baseline() {
    let mut inset = VoiceWindowInset::default();

    assert!(
        inset.visual().is_none(),
        "没有实际播放时不应凭空显示语音驱动窗口内缘光"
    );

    inset.begin_playback();
    inset.tick(Duration::from_millis(60));
    let halfway = inset
        .visual()
        .expect("实际播放淡入中点应出现可见的窗口内缘光");
    assert!(
        halfway.opacity > 0.0 && halfway.opacity < 0.12,
        "60ms 中点的透明度应位于 0 与 12% 底光之间，实际为 {}",
        halfway.opacity
    );
    assert!(
        halfway.inset_px > 0.0 && halfway.inset_px < 10.0,
        "60ms 中点的内扩应位于 0 与 10px 底光之间，实际为 {}",
        halfway.inset_px
    );

    inset.tick(Duration::from_millis(60));
    let settled = inset.visual().expect("累计 120ms 后安静底光应保持可见");
    assert_eq!(
        settled.opacity, 0.12,
        "累计 120ms 后透明度应精确达到 12% 安静底光"
    );
    assert_eq!(
        settled.inset_px, 10.0,
        "累计 120ms 后内扩应精确达到 10px 安静底光"
    );
}

/// VWI-2：用户打断播放后，迟到的完成回调不得把 80ms 快速熄灭改写成正常 220ms 收束。
///
/// 数据构造（含关键数值推导）：
///   先推进 120ms，使一段实际播放达到 12% / 10px 安静底光。
///   打断后产品定义的收束为 80ms；再模拟底层音频后端发出的迟到完成回调。
///
/// 执行过程：
///   1. 开始播放并推进至安静底光。
///   2. 发送打断事件，随后发送迟到的完成事件。
///   3. 推进 80ms，并读取公开可视状态。
///
/// 预期结果：
///   - 正断言：80ms 后白光完全消失。
///   - 负断言：迟到完成事件不应让白光重新进入或延长为正常结束动画。
#[test]
fn interrupted_playback_ignores_late_completion_and_finishes_within_80ms() {
    let mut inset = VoiceWindowInset::default();
    inset.begin_playback();
    inset.tick(Duration::from_millis(120));

    inset.interrupt_playback();
    inset.finish_playback();
    inset.tick(Duration::from_millis(80));

    assert!(
        inset.visual().is_none(),
        "打断后的迟到完成回调不得延长白光；80ms 后必须完全消失"
    );
}

/// VWI-3：整段回复的最后一个播放块完成后，窗口内缘光应使用 220ms 的正常收束。
///
/// 数据构造：一段播放已到达 12% / 10px 安静底光；产品定义正常结束收束为 220ms。
/// 执行过程：通知播放完成，先推进 110ms，再推进第二个 110ms。
/// 预期结果：中点仍保留可见白光；累计 220ms 后白光消失，且不会继续请求可视状态。
#[test]
fn completed_playback_settles_window_inset_over_220ms() {
    let mut inset = VoiceWindowInset::default();
    inset.begin_playback();
    inset.tick(Duration::from_millis(120));

    inset.finish_playback();
    inset.tick(Duration::from_millis(110));
    assert!(
        inset.visual().is_some(),
        "正常结束收束中点仍应保留可见白光，不能提前熄灭"
    );

    inset.tick(Duration::from_millis(110));
    assert!(
        inset.visual().is_none(),
        "正常结束累计 220ms 后窗口内缘光必须完全消失"
    );
}
