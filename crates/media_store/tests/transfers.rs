use media_store::{MediaAvailability, MediaTransfers};
use std::path::PathBuf;

// 场景：多个窗口请求同一媒体，用户取消后立即重试。
// 构造 3 字节 abc 任务；旧任务进度/成功迟到。预期去重，迟到结果无效，新任务可就绪。
#[test]
fn cancelled_and_retried_transfer_rejects_late_results_and_duplicate_requests() {
    let mut transfers = MediaTransfers::default();
    let first = transfers
        .begin("wallpapers/a.mp4", 3, false)
        .expect("first request must start");
    assert!(
        transfers.begin("wallpapers/a.mp4", 3, false).is_none(),
        "duplicate window must share active download"
    );
    assert!(
        transfers.progress("wallpapers/a.mp4", first, 2),
        "current transfer must accept progress"
    );
    assert!(
        transfers.cancel("wallpapers/a.mp4"),
        "active request must be cancellable"
    );
    assert_eq!(
        transfers.state("wallpapers/a.mp4"),
        MediaAvailability::Cancelled,
        "cancelled request must remain cancelled"
    );
    assert!(
        transfers.begin("wallpapers/a.mp4", 3, false).is_none(),
        "automatic sync must not restart a user cancellation"
    );
    let retry = transfers
        .begin("wallpapers/a.mp4", 3, true)
        .expect("explicit retry must start");
    assert!(
        !transfers.progress("wallpapers/a.mp4", first, 3),
        "old progress cannot update retry"
    );
    assert!(
        !transfers.finish("wallpapers/a.mp4", first, Ok(PathBuf::from("old"))),
        "late success cannot complete a newer transfer"
    );
    assert!(
        transfers.finish("wallpapers/a.mp4", retry, Ok(PathBuf::from("verified"))),
        "verified current transfer must publish"
    );
    assert_eq!(
        transfers.state("wallpapers/a.mp4"),
        MediaAvailability::Ready(PathBuf::from("verified")),
        "only current verified bytes become ready"
    );
}

// 场景：离线失败后 UI 多次同步，直到用户明确重试。预期不自动产生请求风暴。
#[test]
fn failed_transfer_waits_for_explicit_retry() {
    let mut transfers = MediaTransfers::default();
    let first = transfers
        .begin("feature-guide/a.mp4", 3, false)
        .expect("request must start");
    assert!(
        transfers.finish("feature-guide/a.mp4", first, Err("offline".into())),
        "current failure must be recorded"
    );
    assert!(
        matches!(
            transfers.state("feature-guide/a.mp4"),
            MediaAvailability::Failed(_)
        ),
        "offline media must not become ready"
    );
    assert!(
        transfers.begin("feature-guide/a.mp4", 3, false).is_none(),
        "render/sync cannot retry a failed network request"
    );
    assert!(
        transfers.begin("feature-guide/a.mp4", 3, true).is_some(),
        "explicit retry must be available"
    );
}

// 场景：已验证媒体在运行期间被清缓存，用户明确重新选择/重试。
// seed Ready 后显式重试；预期后台可重新校验或修复，自动同步仍不重复启动。
#[test]
fn explicit_retry_can_revalidate_previously_ready_media() {
    let mut transfers = MediaTransfers::default();
    transfers.seed_verified("wallpapers/a.mp4", PathBuf::from("old-cache"));
    assert!(
        transfers.begin("wallpapers/a.mp4", 3, false).is_none(),
        "automatic sync cannot repeatedly revalidate ready media"
    );
    assert!(
        transfers.begin("wallpapers/a.mp4", 3, true).is_some(),
        "explicit user retry must permit background cache validation and repair"
    );
}
