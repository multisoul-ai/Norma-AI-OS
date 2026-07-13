use super::*;
use audio::{PcmAudioChunk, PcmFormat};
use std::time::Duration;

fn pcm16_mono_chunk(amplitude: i16, duration: Duration) -> PcmAudioChunk {
    let frame_count = (duration.as_secs_f64() * 16_000.0) as usize;
    let bytes = std::iter::repeat_n(amplitude.to_le_bytes(), frame_count)
        .flatten()
        .collect();
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, bytes)
        .expect("测试 PCM 必须满足 16kHz mono PCM16 格式约束")
}

/// VWI-1：实际播放开始后，语音驱动窗口内缘光应在 120ms 内平滑进入安静底光。
///
/// 数据构造（含关键数值推导）：
///   协调器初始没有待播放音频，故视觉状态必须为空。
///   产品定义淡入时长为 120ms；在 60ms 中点白光应已可见但尚未达到最终底光，
///   在第二个 60ms 后恰好累计 120ms，必须达到 12% 不透明度和 30px 内扩。
///   中点强度为 60ms / 120ms = 50%，故内扩应位于 0 与 30px 之间。
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
        halfway.inset_px > 0.0 && halfway.inset_px < 30.0,
        "60ms 中点的内扩应位于 0 与 30px 底光之间，实际为 {}",
        halfway.inset_px
    );

    inset.tick(Duration::from_millis(60));
    let settled = inset.visual().expect("累计 120ms 后安静底光应保持可见");
    assert_eq!(
        settled.opacity, 0.12,
        "累计 120ms 后透明度应精确达到 12% 安静底光"
    );
    assert_eq!(
        settled.inset_px, 30.0,
        "累计 120ms 后内扩应精确达到 30px 安静底光"
    );
}

/// VWI-1b：静音 PCM 保持白色底光，响亮的实际播放 PCM 必须把连续内缘光提升到峰值范围。
///
/// 数据构造（含关键数值推导）：
///   采样率 = 16_000Hz；能量窗口 = 16ms，因此每块帧数 = 16_000 * 16 / 1_000 = 256 frames。
///   静音块 amplitude = 0、持续 16ms；响亮块 amplitude = 32_767（PCM16 正最大值）、持续 128ms。
///   响亮块含 128ms / 16ms = 8 个能量窗口；消费其中 7 个窗口 = 112ms，
///   已远超 35ms 起振时间，足以接近峰值，但仍保留一个窗口使目标能量不回落。
///   产品底光 = 12% / 30px；自然律动峰值上限 = 55% / 38px；淡入先推进 120ms 至稳定底光。
///
/// 执行过程：
///   1. 开始实际播放并推进 120ms，使窗口内缘光完成淡入。
///   2. 排入 16ms 静音 PCM 并推进一个 16ms 动画帧。
///   3. 排入 128ms 响亮 PCM，连续推进 7 个 16ms 动画帧后读取两次可视状态。
///
/// 预期结果：
///   - 正断言：静音 PCM 仍精确保持 12% / 30px 底光。
///   - 正断言：响亮 PCM 的透明度和内扩均高于静音底光。
///   - 负断言：响亮 PCM 不得超过 55% / 38px 产品峰值，避免过曝或遮挡内容。
#[test]
fn pcm_energy_lifts_window_inset_above_the_quiet_baseline_without_exceeding_peak() {
    let mut inset = VoiceWindowInset::default();
    inset.begin_playback();
    inset.tick(Duration::from_millis(120));

    inset.queue_pcm_energy(&pcm16_mono_chunk(0, Duration::from_millis(16)));
    inset.tick(Duration::from_millis(16));
    let quiet = inset
        .visual()
        .expect("静音 PCM 仍处于实际播放期间，必须保留窗口内缘光底光");
    assert_eq!(quiet.opacity, 0.12, "静音 PCM 不得把底光透明度抬高或熄灭");
    assert_eq!(quiet.inset_px, 30.0, "静音 PCM 不得改变 30px 白色底光宽度");

    inset.queue_pcm_energy(&pcm16_mono_chunk(i16::MAX, Duration::from_millis(128)));
    // 持续高能量 = 7 * 16ms = 112ms，超过 attack(35ms)，但小于 8 个窗口总长 128ms。
    for _ in 0..7 {
        inset.tick(Duration::from_millis(16));
    }
    let loud = inset
        .visual()
        .expect("响亮 PCM 的实际播放期间必须持续显示窗口内缘光");
    assert!(
        loud.opacity > quiet.opacity,
        "响亮 PCM 应提升透明度；quiet={}, loud={}",
        quiet.opacity,
        loud.opacity
    );
    assert!(
        loud.inset_px > quiet.inset_px,
        "响亮 PCM 应增加内扩；quiet={}, loud={}",
        quiet.inset_px,
        loud.inset_px
    );
    assert!(
        loud.opacity <= 0.55,
        "响亮 PCM 不得超过 55% 峰值透明度，实际为 {}",
        loud.opacity
    );
    assert!(
        loud.inset_px <= 38.0,
        "响亮 PCM 不得超过 38px 峰值内扩，实际为 {}",
        loud.inset_px
    );
}

/// VWI-1c：统一内缘光的四角必须由互斥渐变分区组成，不能再让两条边在同一像素叠加。
///
/// 数据构造（含关键数值推导）：
///   内容区宽 = 200px，高 = 100px；白色底光向内延展 = 40px。
///   内部无光中心宽 = 200 - 2 * 40 = 120px，高 = 100 - 2 * 40 = 20px。
///   取左上角上侧 (25, 10)、左侧 (10, 25)、顶部中央 (100, 10)、左边中央 (10, 50)
///   与中心 (100, 50)，分别覆盖角部两个三角分区、两条直边和无光中心。
///
/// 执行过程：
///   1. 按 200px × 100px 内容区与 40px 内扩构造统一内缘光分区。
///   2. 查询四个边缘采样点和一个中心采样点被多少个渐变分区拥有。
///
/// 预期结果：
///   - 正断言：四个边缘采样点都恰好属于一个分区，不会产生角部双重 Alpha。
///   - 正断言：中心采样点不属于任何分区，内容区中央不应被白光覆盖。
///   - 负断言：任何受测边缘点都不得同时属于两个分区。
#[test]
fn unified_inset_partition_assigns_each_edge_pixel_to_exactly_one_gradient() {
    let top_left_top = inset_partition_coverage(200.0, 100.0, 40.0, 25.0, 10.0);
    let top_left_left = inset_partition_coverage(200.0, 100.0, 40.0, 10.0, 25.0);
    let top_center = inset_partition_coverage(200.0, 100.0, 40.0, 100.0, 10.0);
    let left_center = inset_partition_coverage(200.0, 100.0, 40.0, 10.0, 50.0);
    let center = inset_partition_coverage(200.0, 100.0, 40.0, 100.0, 50.0);

    assert_eq!(
        top_left_top, 1,
        "左上角上侧采样点必须只由顶部渐变拥有，避免与左边渐变重叠"
    );
    assert_eq!(
        top_left_left, 1,
        "左上角左侧采样点必须只由左侧渐变拥有，避免与顶部渐变重叠"
    );
    assert_eq!(top_center, 1, "顶部中央采样点必须保留恰好一层内缘光");
    assert_eq!(left_center, 1, "左边中央采样点必须保留恰好一层内缘光");
    assert_eq!(center, 0, "内容区中心不得被任何内缘光渐变覆盖");
    assert_ne!(top_left_top, 2, "左上角上侧不得重叠两层渐变");
    assert_ne!(top_left_left, 2, "左上角左侧不得重叠两层渐变");
}

/// VWI-1d：峰值动态内缘光的角部共享斜边必须补足抗锯齿合成损失，视觉上与完整边闭合。
///
/// 数据构造（含关键数值推导）：
///   峰值边缘透明度 A = 55% = 0.55；共享斜边上两个互斥三角各覆盖半个像素，
///   因此旧实现每段源透明度 = A / 2 = 0.275。
///   两段 premultiplied source-over 合成后 = 0.275 + 0.275 * (1 - 0.275) = 0.474375，
///   比完整边 0.55 少 0.075625，故动态放大时会呈现暗缝。
///
/// 执行过程：
///   1. 以 0.55 构造共享斜边两段各 50% 覆盖的旧合成结果。
///   2. 读取渲染器为该透明度计算的斜边补偿量。
///   3. 将补偿描边覆盖到旧合成结果上，并比较其与完整边。
///
/// 预期结果：
///   - 正断言：补偿后角部透明度应恢复为完整边的 0.55。
///   - 正断言：补偿量必须为正，才能填补抗锯齿造成的暗缝。
///   - 负断言：未补偿的角部不得仍被当作完整边；补偿量也不得大于完整边而形成亮线。
#[test]
fn corner_seam_guard_restores_peak_alpha_to_the_continuous_edge() {
    let edge_alpha = 0.55;
    let half_coverage_alpha = edge_alpha * 0.5;
    let split_corner_alpha = source_over_alpha(half_coverage_alpha, half_coverage_alpha);
    let guard_alpha = corner_seam_compensation_alpha(edge_alpha);
    let repaired_corner_alpha = source_over_alpha(guard_alpha, split_corner_alpha);

    assert!(
        split_corner_alpha < edge_alpha,
        "没有补偿时共享斜边必须复现暗缝；corner={}, edge={}",
        split_corner_alpha,
        edge_alpha
    );
    assert!(
        (repaired_corner_alpha - edge_alpha).abs() <= 0.0001,
        "补偿后的角部透明度必须恢复为完整边；corner={}, edge={}",
        repaired_corner_alpha,
        edge_alpha
    );
    assert!(
        guard_alpha > 0.0,
        "补偿描边透明度必须为正，才能填补共享斜边的抗锯齿暗缝"
    );
    assert!(
        guard_alpha < edge_alpha,
        "补偿描边不得比完整边更亮，否则角落会形成额外亮线；guard={}, edge={}",
        guard_alpha,
        edge_alpha
    );
}

fn source_over_alpha(source: f32, destination: f32) -> f32 {
    source + destination * (1.0 - source)
}

/// VWI-2b：打断播放必须丢弃尚未消费的 PCM 能量，下一段播放不得复用上一段的高亮状态。
///
/// 数据构造（含关键数值推导）：
///   先推进 120ms 到 12% / 30px 底光；响亮 PCM = 16kHz * 128ms = 2_048 frames，
///   其中第一个 16ms 窗口 = 256 frames、amplitude = 32_767，足以把可视状态提升到底光以上。
///   打断收束时长 = 80ms；新一段播放再推进 120ms，应只回到 12% / 30px，不能残留旧峰值。
///
/// 执行过程：
///   1. 开始播放、注入响亮 PCM 并推进一个 16ms 能量窗口。
///   2. 在其余 PCM 尚未消费时发送打断和迟到完成，再推进 80ms。
///   3. 开始一段没有 PCM 的新播放并推进完整 120ms 淡入。
///
/// 预期结果：
///   - 正断言：打断前响亮 PCM 已提升透明度。
///   - 正断言：80ms 后内缘光完全消失。
///   - 负断言：下一段无 PCM 播放不得继承上一段的高亮，只能停在 12% / 30px 底光。
#[test]
fn interrupted_playback_discards_pending_pcm_energy_before_the_next_reply() {
    let mut inset = VoiceWindowInset::default();
    inset.begin_playback();
    inset.tick(Duration::from_millis(120));
    inset.queue_pcm_energy(&pcm16_mono_chunk(i16::MAX, Duration::from_millis(128)));
    inset.tick(Duration::from_millis(16));
    let loud = inset.visual().expect("响亮 PCM 播放中必须显示窗口内缘光");
    assert!(
        loud.opacity > 0.12,
        "打断前的响亮 PCM 必须已把透明度提升到底光以上，实际为 {}",
        loud.opacity
    );

    inset.interrupt_playback();
    inset.finish_playback();
    inset.tick(Duration::from_millis(80));
    assert!(
        inset.visual().is_none(),
        "打断和迟到完成后 80ms 必须完全熄灭，不能保留旧 PCM 能量"
    );

    inset.begin_playback();
    inset.tick(Duration::from_millis(120));
    let restarted = inset
        .visual()
        .expect("新一段实际播放淡入完成后必须重新显示底光");
    assert_eq!(
        restarted.opacity, 0.12,
        "新一段没有 PCM 时不得复用上一段高亮，应回到 12% 底光"
    );
    assert_eq!(
        restarted.inset_px, 30.0,
        "新一段没有 PCM 时不得复用上一段宽度，应回到 30px 底光"
    );
}

/// VWI-2：用户打断播放后，迟到的完成回调不得把 80ms 快速熄灭改写成正常 220ms 收束。
///
/// 数据构造（含关键数值推导）：
///   先推进 120ms，使一段实际播放达到 12% / 30px 安静底光。
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
/// 数据构造：一段播放已到达 12% / 30px 安静底光；产品定义正常结束收束为 220ms。
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

/// VWI-4：已排入但未开始或已正常结束的 PCM 能量不得污染下一段 TTS 动画。
///
/// 数据构造（含关键数值推导）：
///   queued PCM = 128ms 满幅单声道音频
///   first playback visible time = 120ms 淡入 + 16ms 能量消费
///   second playback = 无 PCM，仅推进 120ms 到安静底光
///
/// 执行过程：
///   1. 排入第一段 PCM 后在 Started 前发送 DiscardQueued，再开始无 PCM 播放。
///   2. 验证底光没有继承已撤回 PCM，随后正常完成并推进 220ms。
///   3. 再开始一段无 PCM 播放，验证正常完成也清除了剩余能量。
///
/// 预期结果：
///   - 正断言：两次无 PCM 播放都只显示 12% 安静底光。
///   - 负断言：撤回或完成后的旧满幅 PCM 不得把后续透明度提升到 12% 以上。
#[test]
fn discarded_or_finished_pcm_energy_does_not_leak_into_next_playback() {
    let mut inset = VoiceWindowInset::default();
    inset.apply_playback_event(
        TtsPlaybackSource::Preview,
        TtsPlaybackEvent::PcmQueued(PcmEnergyTimeline::from_pcm(&pcm16_mono_chunk(
            i16::MAX,
            Duration::from_millis(128),
        ))),
    );
    inset.apply_playback_event(TtsPlaybackSource::Preview, TtsPlaybackEvent::DiscardQueued);
    inset.begin_playback();
    inset.tick(Duration::from_millis(120));
    let after_discard = inset
        .visual()
        .expect("撤回 queued PCM 后的新播放仍应显示安静底光");

    assert_eq!(
        after_discard.opacity, 0.12,
        "DiscardQueued 必须清除已 drain 的 PCM，后续播放只能显示 12% 底光"
    );
    assert!(
        after_discard.opacity <= 0.12,
        "已撤回的满幅 PCM 不得把后续播放透明度提升到底光以上"
    );

    inset.queue_pcm_energy(&pcm16_mono_chunk(i16::MAX, Duration::from_millis(128)));
    inset.tick(Duration::from_millis(16));
    inset.finish_playback();
    inset.tick(Duration::from_millis(220));
    inset.begin_playback();
    inset.tick(Duration::from_millis(120));
    let after_finish = inset
        .visual()
        .expect("正常完成后的下一段播放仍应显示安静底光");

    assert_eq!(
        after_finish.opacity, 0.12,
        "Finished 必须清除上一段剩余 PCM，下一段应回到 12% 底光"
    );
    assert!(
        after_finish.opacity <= 0.12,
        "正常完成后的旧满幅 PCM 不得污染下一段播放动画"
    );
}

/// VWI-5：旧 Preview 的迟到取消事件不得清除已开始的助手回复内缘光。
///
/// 数据构造（含关键数值推导）：
///   assistant PCM = 128ms 满幅单声道音频
///   fade-in = 120ms，故推进 120ms 后必须已有可见内缘光
///   stale preview events = DiscardQueued + Interrupted，各 1 次
///
/// 执行过程：
///   1. 助手先排入 PCM 并开始播放，建立 Assistant purpose 时间线。
///   2. 随后注入上一轮 Preview 的迟到撤回和中断事件。
///   3. 推进完整 120ms 淡入并读取视觉状态。
///
/// 预期结果：
///   - 正断言：助手内缘光仍然可见。
///   - 负断言：旧 Preview 事件不能把助手播放计数和 PCM 时间线清零。
#[test]
fn stale_preview_cancellation_does_not_clear_new_assistant_timeline() {
    let mut inset = VoiceWindowInset::default();
    inset.apply_playback_event(
        TtsPlaybackSource::Assistant,
        TtsPlaybackEvent::PcmQueued(PcmEnergyTimeline::from_pcm(&pcm16_mono_chunk(
            i16::MAX,
            Duration::from_millis(128),
        ))),
    );
    inset.apply_playback_event(TtsPlaybackSource::Assistant, TtsPlaybackEvent::Started);
    inset.apply_playback_event(TtsPlaybackSource::Preview, TtsPlaybackEvent::DiscardQueued);
    inset.apply_playback_event(TtsPlaybackSource::Preview, TtsPlaybackEvent::Interrupted);
    inset.tick(Duration::from_millis(120));

    let visual = inset
        .visual()
        .expect("旧 Preview 取消到达后，新的助手回复内缘光仍应可见");
    assert!(
        visual.opacity >= 0.12,
        "助手播放必须保留至少 12% 底光，旧 Preview 事件不得清空它，实际为 {}",
        visual.opacity
    );
    assert!(
        visual.inset_px > 0.0,
        "旧 Preview 事件不得把助手内缘光内扩重置为零"
    );
}
