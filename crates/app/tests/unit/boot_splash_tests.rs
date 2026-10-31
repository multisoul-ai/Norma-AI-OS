use super::*;
use std::time::Duration;

fn s(seconds: f32) -> Duration {
    Duration::from_secs_f32(seconds)
}

/// 正常冷启动且未处于 e2e 控制下时播放开机动画。
///
/// 数据构造：reduce_motion=false，e2e_controlled=false，e2e_opt_in=false。
/// 执行过程：调用 should_play_boot_splash。
/// 预期结果：
///   - 正断言：返回 true。
#[test]
fn plays_on_normal_launch() {
    assert!(
        should_play_boot_splash(false, false, false),
        "a normal launch without reduce motion must play the boot animation"
    );
}

/// 系统开启「减少动态效果」时不播放开机动画，直接进入 App。
///
/// 数据构造：reduce_motion=true，其余为正常启动。
/// 执行过程：调用 should_play_boot_splash。
/// 预期结果：
///   - 负断言：返回 false（即使 e2e 显式请求也不播放）。
#[test]
fn reduce_motion_skips_animation_even_when_e2e_opts_in() {
    assert!(
        !should_play_boot_splash(true, false, false),
        "reduce motion must skip the boot animation"
    );
    assert!(
        !should_play_boot_splash(true, true, true),
        "reduce motion must win over an e2e opt-in"
    );
}

/// 桌面 e2e 控制的进程默认不播放，避免挡住其他场景；显式 opt-in 时才播放。
///
/// 数据构造：e2e_controlled=true，分别取 e2e_opt_in=false/true。
/// 执行过程：两次调用 should_play_boot_splash。
/// 预期结果：
///   - 负断言：未 opt-in 返回 false。
///   - 正断言：opt-in 返回 true。
#[test]
fn e2e_control_needs_explicit_opt_in() {
    assert!(
        !should_play_boot_splash(false, true, false),
        "e2e-controlled processes must not be blocked by the boot animation by default"
    );
    assert!(
        should_play_boot_splash(false, true, true),
        "NORMA_E2E_BOOT_ANIMATION=1 must let the boot animation scenario play it"
    );
}

/// 第一帧到达前保持全黑遮罩；完整播放后在片尾淡出并结束。
///
/// 数据构造：片长 MAIN_DURATION=6.0s，淡出 FADE_OUT=0.45s；首帧在 t=0.2s 到达。
///   淡出开始 = 0.2 + 6.0 - 0.45 = 5.75s；结束 = 0.2 + 6.0 = 6.2s。
/// 执行过程：new(0) → advance(0.1) → on_first_frame(0.2) → advance 到 3.0 / 5.9 / 6.25。
/// 预期结果：
///   - 正断言：首帧前和播放中 opacity == 1.0；5.9s 处于淡出中（0 < opacity < 1）；6.25s is_done。
///   - 负断言：3.0s 时尚未结束。
#[test]
fn full_playback_fades_out_at_the_end() {
    let mut timeline = BootTimeline::new(s(0.0));
    timeline.advance(s(0.1));
    assert_eq!(
        timeline.opacity(s(0.1)),
        1.0,
        "before the first frame the black cover stays opaque"
    );
    timeline.on_first_frame(s(0.2));
    timeline.advance(s(3.0));
    assert!(
        !timeline.is_done(),
        "mid-playback must not dismiss the animation"
    );
    assert_eq!(timeline.opacity(s(3.0)), 1.0, "playback is fully opaque");
    timeline.advance(s(5.9));
    let fading = timeline.opacity(s(5.9));
    assert!(
        fading > 0.0 && fading < 1.0,
        "5.9s lies inside the 5.75s..6.2s fade, got {fading}"
    );
    timeline.advance(s(6.25));
    assert!(
        timeline.is_done(),
        "after first frame + 6.0s the animation must be done"
    );
    assert_eq!(
        timeline.opacity(s(6.25)),
        0.0,
        "a finished animation is invisible"
    );
}

/// 任意点击或按键跳过：从当前透明度开始短淡出后结束。
///
/// 数据构造：首帧 t=0.1s，在 t=2.0s 跳过；SKIP_FADE=0.25s，结束于 2.25s。
/// 执行过程：on_first_frame(0.1) → skip(2.0) → advance(2.1) → advance(2.3)。
/// 预期结果：
///   - 正断言：2.1s 正在淡出（0 < opacity < 1）；2.3s is_done。
///   - 负断言：重复 skip 不会把已开始的淡出重置回不透明。
#[test]
fn skip_fades_out_quickly() {
    let mut timeline = BootTimeline::new(s(0.0));
    timeline.on_first_frame(s(0.1));
    timeline.skip(s(2.0));
    timeline.advance(s(2.1));
    let mid = timeline.opacity(s(2.1));
    assert!(mid > 0.0 && mid < 1.0, "skip must fade out, got {mid}");
    timeline.skip(s(2.1));
    assert!(
        timeline.opacity(s(2.1)) <= mid,
        "a second skip must not restart the fade from fully opaque"
    );
    timeline.advance(s(2.3));
    assert!(timeline.is_done(), "skip fade lasts 0.25s, so 2.3s is done");
}

/// 视频迟迟没有第一帧（资源缺失/解码慢）时不阻塞 App：超时后直接结束。
///
/// 数据构造：FIRST_FRAME_TIMEOUT=1.5s，没有任何 on_first_frame。
/// 执行过程：advance(1.0) → advance(1.6)。
/// 预期结果：
///   - 负断言：1.0s 尚未结束。
///   - 正断言：1.6s is_done。
#[test]
fn missing_first_frame_times_out() {
    let mut timeline = BootTimeline::new(s(0.0));
    timeline.advance(s(1.0));
    assert!(
        !timeline.is_done(),
        "1.0s is still within the 1.5s first-frame grace"
    );
    timeline.advance(s(1.6));
    assert!(
        timeline.is_done(),
        "no first frame after 1.5s must dismiss the animation"
    );
}

/// 解码失败立即结束，不留下黑屏遮罩。
///
/// 数据构造：播放中 t=1.0s 调用 fail。
/// 执行过程：on_first_frame(0.1) → fail()。
/// 预期结果：
///   - 正断言：is_done 且 opacity == 0。
#[test]
fn decode_failure_dismisses_immediately() {
    let mut timeline = BootTimeline::new(s(0.0));
    timeline.on_first_frame(s(0.1));
    timeline.fail();
    assert!(
        timeline.is_done(),
        "a decode failure must dismiss the animation"
    );
    assert_eq!(
        timeline.opacity(s(1.0)),
        0.0,
        "a failed animation must not leave a black cover"
    );
}

/// 启动瞬间的残留输入（如用 Spotlight 回车启动后的按键事件）不能把动画跳过。
///
/// 数据构造：SKIP_GRACE=0.8s；首帧 t=0.1s；t=0.4s 收到一次 skip，t=1.0s 再收到一次。
/// 执行过程：on_first_frame(0.1) → skip(0.4) → advance(0.5) → skip(1.0) → advance(1.3)。
/// 预期结果：
///   - 负断言：0.4s 的 skip 被忽略，0.5s 仍完全不透明、未结束。
///   - 正断言：1.0s 的 skip 生效，淡出 0.25s 后（1.25s）结束。
#[test]
fn input_right_after_launch_does_not_skip() {
    let mut timeline = BootTimeline::new(s(0.0));
    timeline.on_first_frame(s(0.1));
    timeline.skip(s(0.4));
    timeline.advance(s(0.5));
    assert!(
        !timeline.is_done(),
        "input within the launch grace must not dismiss the animation"
    );
    assert_eq!(
        timeline.opacity(s(0.5)),
        1.0,
        "input within the launch grace must not start a fade"
    );
    timeline.skip(s(1.0));
    timeline.advance(s(1.3));
    assert!(
        timeline.is_done(),
        "a deliberate key after the grace must skip the animation"
    );
}

/// 每次冷启动在两个版本间随机选择：种子奇偶决定版本，两个版本都会被选到。
///
/// 数据构造：BOOT_VARIANTS = [Sync, Awaken]；种子 0、1、2、7。
/// 执行过程：boot_variant_for(seed)。
/// 预期结果：
///   - 正断言：0、2 → boot/boot-sync.mp4；1、7 → boot/boot-awaken.mp4。
///   - 负断言：两个结果互不相同（不会永远只播一个版本）。
#[test]
fn random_seed_picks_both_variants() {
    assert_eq!(
        boot_variant_for(0),
        "boot/boot-sync.mp4",
        "even seed plays Sync"
    );
    assert_eq!(
        boot_variant_for(2),
        "boot/boot-sync.mp4",
        "even seed plays Sync"
    );
    assert_eq!(
        boot_variant_for(1),
        "boot/boot-awaken.mp4",
        "odd seed plays Awaken"
    );
    assert_eq!(
        boot_variant_for(7),
        "boot/boot-awaken.mp4",
        "odd seed plays Awaken"
    );
    assert_ne!(
        boot_variant_for(0),
        boot_variant_for(1),
        "both variants must be reachable"
    );
}

/// e2e 用 NORMA_E2E_BOOT_ANIMATION 指定版本；"1" 或其他值表示随机，未设置表示不开启。
///
/// 数据构造：Some("sync")、Some("awaken")、Some("1")、Some(""), None。
/// 执行过程：forced_boot_variant(value)。
/// 预期结果：
///   - 正断言：sync/awaken 映射到各自资源。
///   - 负断言："1"、空串、None 不强制任何版本。
#[test]
fn e2e_can_force_a_variant() {
    assert_eq!(
        forced_boot_variant(Some("sync")),
        Some("boot/boot-sync.mp4"),
        "sync forces Sync"
    );
    assert_eq!(
        forced_boot_variant(Some("awaken")),
        Some("boot/boot-awaken.mp4"),
        "awaken forces Awaken"
    );
    assert_eq!(
        forced_boot_variant(Some("1")),
        None,
        "1 opts in with a random variant"
    );
    assert_eq!(
        forced_boot_variant(Some("")),
        None,
        "an empty value forces nothing"
    );
    assert_eq!(forced_boot_variant(None), None, "unset forces nothing");
}

/// 每个版本按自己的片长淡出：Awaken 9.0s，Sync 6.0s。
///
/// 数据构造：with_duration(0, 9.0s)；首帧 t=0；淡出开始 = 9.0 - 0.45 = 8.55s。
/// 执行过程：advance(6.5) → advance(8.7) → advance(9.05)。
/// 预期结果：
///   - 负断言：6.5s（超过 Sync 的 6s 片长）仍在播放、完全不透明。
///   - 正断言：8.7s 处于淡出中；9.05s 结束；variant_duration 给出各版本片长。
#[test]
fn each_variant_uses_its_own_length() {
    assert_eq!(variant_duration("boot/boot-sync.mp4"), s(6.0), "Sync is 6s");
    assert_eq!(
        variant_duration("boot/boot-awaken.mp4"),
        s(9.0),
        "Awaken is 9s"
    );
    let mut timeline = BootTimeline::with_duration(s(0.0), s(9.0));
    timeline.on_first_frame(s(0.0));
    timeline.advance(s(6.5));
    assert_eq!(
        timeline.opacity(s(6.5)),
        1.0,
        "a 9s variant must not fade at the 6s mark"
    );
    timeline.advance(s(8.7));
    let fading = timeline.opacity(s(8.7));
    assert!(
        fading > 0.0 && fading < 1.0,
        "8.7s lies inside the 8.55s..9.0s fade, got {fading}"
    );
    timeline.advance(s(9.05));
    assert!(timeline.is_done(), "a 9s variant ends at 9s");
}
