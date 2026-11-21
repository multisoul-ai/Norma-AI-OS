//! Browser 卡片单次导航的加载耗时时间线：创建 / 提交 / 完成。

use std::time::{Duration, Instant};

/// 超过该时长仍未收到 Finished（例如加载失败）就不再显示进度线。
const LOAD_INDICATOR_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserLoadPhase {
    Started,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BrowserLoadReport {
    pub(crate) create: Option<Duration>,
    pub(crate) committed: Option<Duration>,
    pub(crate) finished: Duration,
    pub(crate) resizes: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserRefreshStatus {
    Loading,
    Updated,
    Failed,
}

#[derive(Default)]
pub(crate) struct LoadTimeline {
    active: Option<ActiveLoad>,
    refresh: Option<BrowserRefreshStatus>,
    timed_out: bool,
}

struct ActiveLoad {
    started_at: Instant,
    create: Option<Duration>,
    committed: Option<Duration>,
    resizes: u32,
}

impl LoadTimeline {
    pub(crate) fn begin(&mut self, now: Instant, create: Option<Duration>) {
        // Native creation consumes a pending refresh; an ordinary navigation supersedes it.
        self.timed_out = false;
        if create.is_none() {
            self.refresh = None;
        }
        self.active = Some(ActiveLoad {
            started_at: now,
            create,
            committed: None,
            resizes: 0,
        });
    }

    pub(super) fn request_refresh(&mut self) {
        self.refresh = Some(BrowserRefreshStatus::Loading);
    }

    pub(super) fn begin_refresh(&mut self, now: Instant) {
        self.begin(now, None);
        self.request_refresh();
    }

    pub(super) fn refresh_status(&self) -> Option<BrowserRefreshStatus> {
        self.refresh
    }

    pub(super) fn fail_refresh(&mut self) -> bool {
        if self.refresh != Some(BrowserRefreshStatus::Loading) {
            return false;
        }
        self.active = None;
        self.refresh = Some(BrowserRefreshStatus::Failed);
        true
    }

    pub(super) fn expire(&mut self, now: Instant) -> bool {
        if self.active.is_none() || self.is_loading(now) {
            return false;
        }
        self.fail_refresh();
        self.active = None;
        self.timed_out = true;
        true
    }

    pub(super) fn can_reload(&self, now: Instant, native_loading: bool) -> bool {
        !self.is_loading(now) && (!native_loading || self.timed_out)
    }

    pub(super) fn polled_phase(&self, loading: bool) -> Option<BrowserLoadPhase> {
        match (self.active.as_ref()?.committed.is_some(), loading) {
            (false, true) => Some(BrowserLoadPhase::Started),
            (true, false) => Some(BrowserLoadPhase::Finished),
            _ => None,
        }
    }

    pub(super) fn accepts_event_at(&self, at: Instant) -> bool {
        self.active
            .as_ref()
            .is_none_or(|load| load.started_at <= at)
    }

    pub(crate) fn is_loading(&self, now: Instant) -> bool {
        self.is_loading_within(now, LOAD_INDICATOR_TIMEOUT)
    }

    /// Still waiting for the first commit; past `limit` a failed load stops hiding the view.
    pub(crate) fn is_loading_within(&self, now: Instant, limit: Duration) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| now.saturating_duration_since(active.started_at) < limit)
    }

    pub(crate) fn cancel(&mut self) {
        self.active = None;
        self.refresh = None;
    }

    pub(super) fn clear_refresh(&mut self) {
        self.refresh = None;
    }

    pub(crate) fn note_resize(&mut self) {
        if let Some(active) = self.active.as_mut() {
            active.resizes += 1;
        }
    }

    pub(crate) fn record(
        &mut self,
        phase: BrowserLoadPhase,
        now: Instant,
    ) -> Option<BrowserLoadReport> {
        if !self.accepts_event_at(now) || self.expire(now) {
            return None;
        }
        let active = self.active.as_mut()?;
        let elapsed = now.duration_since(active.started_at);
        match phase {
            BrowserLoadPhase::Started => {
                active.committed.get_or_insert(elapsed);
                None
            }
            BrowserLoadPhase::Finished => {
                if self.refresh == Some(BrowserRefreshStatus::Loading) {
                    active.committed?;
                    self.refresh = Some(BrowserRefreshStatus::Updated);
                }
                let report = BrowserLoadReport {
                    create: active.create,
                    committed: active.committed,
                    finished: elapsed,
                    resizes: active.resizes,
                };
                self.active = None;
                Some(report)
            }
        }
    }
}

/// 只在圆角半径真正变化时才需要重新裁剪原生视图，避免拖拽缩放时每帧改写 CALayer。
#[derive(Default)]
pub(crate) struct CornerClipState {
    applied_radius_px: Option<f64>,
}

impl CornerClipState {
    pub(crate) fn needs_apply(&mut self, radius_px: f64) -> bool {
        if self.applied_radius_px == Some(radius_px) {
            return false;
        }
        self.applied_radius_px = Some(radius_px);
        true
    }
}
