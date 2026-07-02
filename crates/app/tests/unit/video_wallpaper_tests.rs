use super::*;
use std::path::{Path, PathBuf};

fn failed_slot(path: &str) -> VideoWallpaperState {
    VideoWallpaperState::Failed {
        path: PathBuf::from(path),
    }
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
