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

#[derive(Default)]
pub(crate) struct LoadTimeline {
    active: Option<ActiveLoad>,
}

struct ActiveLoad {
    started_at: Instant,
    create: Option<Duration>,
    committed: Option<Duration>,
    resizes: u32,
}

impl LoadTimeline {
    pub(crate) fn begin(&mut self, now: Instant, create: Option<Duration>) {
        self.active = Some(ActiveLoad {
            started_at: now,
            create,
            committed: None,
            resizes: 0,
        });
    }

    pub(crate) fn is_loading(&self, now: Instant) -> bool {
        self.active.as_ref().is_some_and(|active| {
            now.saturating_duration_since(active.started_at) < LOAD_INDICATOR_TIMEOUT
        })
    }

    pub(crate) fn cancel(&mut self) {
        self.active = None;
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
        let active = self.active.as_mut()?;
        let elapsed = now.saturating_duration_since(active.started_at);
        match phase {
            BrowserLoadPhase::Started => {
                active.committed.get_or_insert(elapsed);
                None
            }
            BrowserLoadPhase::Finished => {
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
