use super::*;
use std::path::{Path, PathBuf};

fn failed_slot(path: &str) -> VideoWallpaperState {
    VideoWallpaperState::Failed {
        path: PathBuf::from(path),
    }
}

/// 导览与视频合成层协调：模态可见时所有页面必须释放视频路径，关闭后恢复原路径。
///
/// 数据构造（含关键数值的推导过程）：
///   requested video paths = 1（/tmp/canvas.mp4）
///   guide visibility states = hidden(1) + visible(1) = 2
///   allowed paths while hidden = 1
///   allowed paths while visible = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在导览不可见时传入 Canvas 视频路径。
///   2. 在导览可见时传入相同视频路径。
///   3. 比较共享视频槽在两种模态状态下的解析结果。
///
/// 预期结果：
///   - 正断言：导览不可见时保留原视频路径，关闭导览可恢复播放。
///   - 负断言：导览可见时返回 None，避免原生 Surface 旧帧盖住 GPUI 模态。
///   - 负断言：隐藏与可见状态不能返回相同结果，否则生命周期规则未生效。
#[test]
fn test_feature_guide_releases_shared_video_wallpaper_path() {
    let video_path = Path::new("/tmp/canvas.mp4");
    let without_guide = video_wallpaper_path_for_feature_guide(Some(video_path), false);
    let with_guide = video_wallpaper_path_for_feature_guide(Some(video_path), true);

    assert_eq!(
        without_guide,
        Some(video_path),
        "导览不可见时必须保留 Canvas 原视频路径，以便正常播放或恢复"
    );
    assert!(
        with_guide.is_none(),
        "导览可见时必须释放共享视频路径并清空原生 Surface 槽"
    );
    assert_ne!(
        without_guide, with_guide,
        "导览隐藏与可见状态必须产生不同视频槽结果，否则模态仍可能被旧帧遮盖"
    );
}

/// Failed slot with the same path should not restart decode.
///
/// Data setup:
///   slot.path = "/tmp/a.mov"; resolved.path = "/tmp/a.mov"; is_video = true
///
/// Execution:
///   1. decide_video_sync sees a video wallpaper request.
///   2. It compares the failed slot path with the resolved path.
///
/// Expected:
///   - Positive: action is Keep because retrying the same failed asset can spin.
///   - Negative: action is not Restart for the same path.
#[test]
fn test_failed_slot_same_path_does_not_restart() {
    let slot = failed_slot("/tmp/a.mov");
    let action = decide_video_sync(Some(&slot), true, Some(Path::new("/tmp/a.mov")));

    assert_eq!(
        action,
        VideoSyncAction::Keep,
        "same failed video path should stay failed instead of restarting a decode loop"
    );
    assert!(
        !matches!(action, VideoSyncAction::Restart { .. }),
        "same failed video path must not produce Restart"
    );
}

/// Failed slot with a new path should restart decode.
///
/// Data setup:
///   slot.path = "/tmp/a.mov"; resolved.path = "/tmp/b.mov"; is_video = true
///
/// Execution:
///   1. decide_video_sync sees a video wallpaper request.
///   2. It compares the failed slot path with the newly resolved path.
///
/// Expected:
///   - Positive: action is Restart with "/tmp/b.mov".
///   - Negative: action is not Keep because the requested asset changed.
#[test]
fn test_failed_slot_different_path_restarts() {
    let slot = failed_slot("/tmp/a.mov");
    let action = decide_video_sync(Some(&slot), true, Some(Path::new("/tmp/b.mov")));

    assert_eq!(
        action,
        VideoSyncAction::Restart {
            path: PathBuf::from("/tmp/b.mov")
        },
        "different video path should restart decoding for the new asset"
    );
    assert!(
        action != VideoSyncAction::Keep,
        "different video path must not keep the old failed slot"
    );
}

/// Existing video slot should clear when wallpaper kind is not video.
///
/// Data setup:
///   slot.path = "/tmp/a.mov"; is_video = false; resolved.path = none
///
/// Execution:
///   1. decide_video_sync sees a non-video wallpaper kind.
///   2. It returns the cleanup action for the existing slot.
///
/// Expected:
///   - Positive: action is Clear.
///   - Negative: action is not Keep because the video worker must be cancelled.
#[test]
fn test_failed_slot_cleared_when_kind_not_video() {
    let slot = failed_slot("/tmp/a.mov");
    let action = decide_video_sync(Some(&slot), false, None);

    assert_eq!(
        action,
        VideoSyncAction::Clear,
        "non-video wallpaper kind should clear the existing video slot"
    );
    assert!(
        action != VideoSyncAction::Keep,
        "non-video wallpaper kind must not keep a stale video worker"
    );
}

/// Empty slot with a resolved video path should start decode.
///
/// Data setup:
///   slot = none; resolved.path = "/tmp/a.mov"; is_video = true
///
/// Execution:
///   1. decide_video_sync sees there is no current slot.
///   2. It builds a Restart action from the resolved path.
///
/// Expected:
///   - Positive: action is Restart with "/tmp/a.mov".
///   - Negative: action is not Keep because no frame worker exists yet.
#[test]
fn test_none_slot_video_kind_restarts() {
    let action = decide_video_sync(None, true, Some(Path::new("/tmp/a.mov")));

    assert_eq!(
        action,
        VideoSyncAction::Restart {
            path: PathBuf::from("/tmp/a.mov")
        },
        "video wallpaper with no active slot should start a decoder worker"
    );
    assert!(
        action != VideoSyncAction::Keep,
        "video wallpaper with no active slot must not be treated as already synced"
    );
}

/// Empty slot with an unresolved video path should keep state unchanged.
///
/// Data setup:
///   slot = none; resolved.path = none; is_video = true
///
/// Execution:
///   1. decide_video_sync sees the wallpaper kind is video.
///   2. It cannot resolve a path, so it avoids starting a worker.
///
/// Expected:
///   - Positive: action is Keep.
///   - Negative: action is not Restart because there is no concrete file.
#[test]
fn test_none_slot_unresolvable_path_keeps() {
    let action = decide_video_sync(None, true, None);

    assert_eq!(
        action,
        VideoSyncAction::Keep,
        "unresolved video wallpaper path should leave state unchanged"
    );
    assert!(
        !matches!(action, VideoSyncAction::Restart { .. }),
        "unresolved video wallpaper path must not start a decoder worker"
    );
}

/// Empty slot with non-video wallpaper should keep state unchanged.
///
/// Data setup:
///   slot = none; resolved.path = none; is_video = false
///
/// Execution:
///   1. decide_video_sync sees a non-video wallpaper kind.
///   2. It sees there is no current video slot to clear.
///
/// Expected:
///   - Positive: action is Keep.
///   - Negative: action is not Clear because no video worker exists.
#[test]
fn test_none_slot_non_video_keeps() {
    let action = decide_video_sync(None, false, None);

    assert_eq!(
        action,
        VideoSyncAction::Keep,
        "non-video wallpaper with no active slot should leave state unchanged"
    );
    assert!(
        action != VideoSyncAction::Clear,
        "non-video wallpaper with no active slot must not request cleanup"
    );
}

/// Identical slot and worker paths should match.
///
/// Data setup:
///   current.path = "/tmp/a.mov"; spawned.path = "/tmp/a.mov"
///
/// Execution:
///   1. slot_path_matches compares both paths by value.
///
/// Expected:
///   - Positive: identical paths return true.
///   - Negative: this case must not return false, or poll loops will stop early.
#[test]
fn test_slot_path_matches_same_path() {
    let matches = slot_path_matches(Path::new("/tmp/a.mov"), Path::new("/tmp/a.mov"));

    assert!(
        matches,
        "identical video wallpaper paths should match the active poll loop"
    );
    let stopped_poll_loop = !matches;
    assert!(
        !stopped_poll_loop,
        "identical video wallpaper paths must not stop the poll loop"
    );
}

/// Different slot and worker paths should not match.
///
/// Data setup:
///   current.path = "/tmp/a.mov"; spawned.path = "/tmp/b.mov"
///
/// Execution:
///   1. slot_path_matches compares both paths by value.
///
/// Expected:
///   - Positive: different paths return false.
///   - Negative: this case must not return true, or stale workers can update UI.
#[test]
fn test_slot_path_matches_different_path() {
    let matches = slot_path_matches(Path::new("/tmp/a.mov"), Path::new("/tmp/b.mov"));

    assert!(
        !matches,
        "different video wallpaper paths should reject stale poll loop updates"
    );
    let stale_worker_accepted = matches;
    assert!(
        !stale_worker_accepted,
        "different video wallpaper paths must not be treated as the active worker"
    );
}

/// VW-LIFECYCLE-1：同一路径视频快速重启后，旧 poll 必须失去新槽所有权。
///
/// 数据构造（含关键数值的推导过程）：
///   video paths = 1（`/tmp/a.mov`，Clear 前后路径相同）
///   mailbox tokens = old(1) + restarted(1) = 2
///   expected active poll owners = 1（仅 restarted token）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 old mailbox 模拟 Clear 前的解码槽与 poll。
///   2. 用 restarted mailbox 模拟导览关闭后立即恢复的同路径新槽。
///   3. 分别校验新 poll 与旧 poll 对 restarted 槽的所有权。
///
/// 预期结果：
///   - 正断言：restarted token 与新槽匹配，新的 40ms poll 可以继续。
///   - 负断言：old token 即使路径相同也不得匹配，旧 poll 必须退出。
///   - 负断言：新旧 token 的判断不得相同，否则快速开关会累积 25fps 定时器。
#[test]
fn test_same_path_restart_rejects_stale_video_poll_owner() {
    let video_path = Path::new("/tmp/a.mov");
    let old_mailbox = Arc::new(VideoFrameMailbox::default());
    let restarted_mailbox = Arc::new(VideoFrameMailbox::default());

    let restarted_poll_matches = slot_poll_owner_matches(
        video_path,
        video_path,
        &restarted_mailbox,
        &restarted_mailbox,
    );
    let stale_poll_matches =
        slot_poll_owner_matches(video_path, video_path, &restarted_mailbox, &old_mailbox);

    assert!(
        restarted_poll_matches,
        "同一路径重启后，新 mailbox token 必须拥有新视频槽的 poll 权限"
    );
    assert!(
        !stale_poll_matches,
        "同一路径重启后，旧 mailbox token 必须失效并让旧 poll 退出"
    );
    assert_ne!(
        restarted_poll_matches, stale_poll_matches,
        "新旧 mailbox token 必须产生不同所有权结果，否则会累积前台定时器"
    );
}

/// VW-SETTINGS-1：Settings 遮住 Canvas 时，视频新帧不得请求整窗重绘。
///
/// 数据构造（含关键数值的推导过程）：
///   mailbox state_changed = true（后台解码器交付一帧新视频）
///   settings states = 2（closed 作为正对照，open 验证遮罩降载）
///   foreground cadence = 40ms（每秒约 1,000 / 40 = 25 次潜在 repaint）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 模拟视频 mailbox 已将新帧交换到 App state。
///   2. 在 Settings 关闭时解析 repaint 决策。
///   3. 保持新帧状态不变，仅打开 Settings，再次解析 repaint 决策。
///
/// 预期结果：
///   - 正断言：Settings 关闭时新视频帧必须刷新可见壁纸。
///   - 负断言：Settings 打开时新视频帧不得以约 25fps 重绘被遮住的 Canvas。
///   - 负断言：打开与关闭状态不得产生相同结果，否则遮罩状态未参与视频策略。
#[test]
fn test_video_wallpaper_repaint_pauses_while_settings_occludes_canvas() {
    let settings_closed = video_wallpaper_should_notify(true, false);
    let settings_open = video_wallpaper_should_notify(true, true);

    assert!(settings_closed, "Settings 关闭时新视频帧必须刷新可见壁纸");
    assert!(
        !settings_open,
        "Settings 打开时视频帧不得刷新被遮住的 Canvas"
    );
    assert_ne!(
        settings_closed, settings_open,
        "Settings 遮罩状态必须改变视频壁纸 repaint 决策"
    );
}

/// V-PTS-1：视频 worker 必须按相邻帧 PTS 等待，并扣除本帧已经消耗的解码时间。
///
/// 数据构造（含关键数值的推导过程）：
///   previous_pts = 2,000,000µs
///   current_pts  = 2,033,333µs（约 30fps，帧间隔 33,333µs）
///   decode_time  = 5,000µs
///   expected     = 33,333 - 5,000 = 28,333µs
///   first_frame  = previous None + current 0ms → 0ms（首帧立即显示）
///   fallback     = 40ms - 10ms = 30ms（缺少 PTS 时保持旧约 25fps 行为）
///   max_gap      = 1,000ms，安全上限 = 250ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 计算正常 30fps 相邻帧的剩余等待时间
///   2. 模拟第一帧只有 current PTS，检查立即显示
///   3. 模拟视频循环后 PTS 从 14.9s 回到 0，检查立即重建时钟
///   4. 模拟无 PTS 输入，检查回退到旧 40ms cadence
///   5. 模拟 1 秒异常间隙，检查等待被限制到 250ms
///
/// 预期结果：
///   - 正断言：30fps 帧等待精确为 28,333µs，不再固定 sleep 40ms
///   - 正断言：第一帧等待为零
///   - 正断言：循环 PTS 回退时等待为零
///   - 正断言：无 PTS 时回退等待为 30ms
///   - 负断言：异常 1 秒 PTS 间隙不得阻塞 worker 超过 250ms
#[test]
fn test_video_frame_delay_tracks_pts_without_adding_decode_time() {
    let normal = video_frame_delay(
        Some(Duration::from_micros(2_000_000)),
        Some(Duration::from_micros(2_033_333)),
        Duration::from_micros(5_000),
    );
    let first_frame = video_frame_delay(None, Some(Duration::ZERO), Duration::ZERO);
    let loop_reset = video_frame_delay(
        Some(Duration::from_millis(14_900)),
        Some(Duration::ZERO),
        Duration::from_millis(5),
    );
    let fallback = video_frame_delay(None, None, Duration::from_millis(10));
    let capped = video_frame_delay(
        Some(Duration::from_secs(1)),
        Some(Duration::from_secs(2)),
        Duration::ZERO,
    );

    assert_eq!(
        normal,
        Duration::from_micros(28_333),
        "30fps PTS 等待必须扣除 5ms 解码时间，避免每帧额外累积处理耗时"
    );
    assert_eq!(
        first_frame,
        Duration::ZERO,
        "携带有效 PTS 的首帧必须立即进入 mailbox，不能额外等待旧 40ms cadence"
    );
    assert_eq!(
        loop_reset,
        Duration::ZERO,
        "视频循环导致 PTS 回退时必须立即展示新首帧并重建时钟"
    );
    assert_eq!(
        fallback,
        Duration::from_millis(30),
        "缺少 PTS 时必须从旧 40ms cadence 扣除已经过去的 10ms"
    );
    assert_eq!(
        capped,
        Duration::from_millis(250),
        "异常 PTS 间隙必须限制为 250ms，不能让后台 worker 长时间假死"
    );
    assert_ne!(
        normal,
        Duration::from_millis(VIDEO_PTS_FALLBACK_INTERVAL_MS),
        "有效 30fps PTS 不得继续无条件使用固定 40ms sleep"
    );
}

/// V-PTS-3：后台视频循环必须消费解码帧 PTS，不得继续在每帧后固定 sleep 40ms。
///
/// 数据构造（含关键数值的推导过程）：
///   production source = video_wallpaper.rs
///   正向锚点 = frame.presentation_time() + video_frame_delay
///   worker 函数体边界 = `fn decode_video_wallpaper_frames` 到 `#[cfg(test)]`
///   旧错误锚点 = std::thread::sleep(Duration::from_millis(VIDEO_PTS_FALLBACK_INTERVAL_MS))
///   30fps 旧时长倍率 = 40ms / 33.33ms ≈ 1.20（约慢放 20%）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取真实后台 worker 源码
///   2. 截取 `decode_video_wallpaper_frames` 函数体，排除同文件函数定义造成的假阳性
///   3. 确认 worker 从每个 DecodedVideoFrame 读取 PTS
///   4. 确认 worker 调用已验证的剩余延迟计算
///   5. 排除旧固定 40ms thread sleep
///
/// 预期结果：
///   - 正断言：worker 同时包含 PTS 读取和 `video_frame_delay` 调用
///   - 负断言：worker 不得保留固定 40ms sleep，否则 29.93fps 4K 源会慢放
#[test]
fn test_video_wallpaper_worker_uses_frame_pts_instead_of_fixed_sleep() {
    let source = include_str!("../../src/app/video_wallpaper.rs");
    let (_, worker_and_tail) = source
        .split_once("fn decode_video_wallpaper_frames(")
        .expect("生产源码必须包含视频后台 worker 函数");
    let (worker, _) = worker_and_tail
        .split_once("\n#[cfg(test)]")
        .expect("视频后台 worker 后必须保留测试模块边界");

    assert!(
        worker.contains("frame.presentation_time()"),
        "视频 worker 必须读取真实解码帧 PTS，不能忽略媒体时间轴"
    );
    assert!(
        worker
            .contains("video_frame_delay(previous_pts, current_pts, elapsed_since_previous_frame)"),
        "视频 worker 必须通过已验证的延迟函数扣除解码时间并处理循环回退"
    );
    assert!(
        !worker
            .contains("std::thread::sleep(Duration::from_millis(VIDEO_PTS_FALLBACK_INTERVAL_MS))"),
        "视频 worker 不得继续固定 sleep 40ms，否则 29.93fps 素材会累计约 20% 慢放"
    );
}
