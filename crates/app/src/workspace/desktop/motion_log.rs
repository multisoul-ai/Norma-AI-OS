//! Navigation-level diagnostics; callback intervals are not GPU/presentation timings.
use super::{camera::DesktopCanvasSubject, DesktopWorkspaceUiState};
use canvas::CanvasRect;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

pub(super) struct DesktopMotionLog {
    id: u64,
    kind: &'static str,
    started: Instant,
    first_callback: Option<Duration>,
    callbacks: u64,
    max_gap: Duration,
    long_gaps: u64,
    clamped_gaps: u64,
}

impl DesktopMotionLog {
    pub(super) fn record_callback(&mut self, now: Instant, gap: Duration) {
        self.first_callback
            .get_or_insert(now.saturating_duration_since(self.started));
        self.callbacks += 1;
        self.max_gap = self.max_gap.max(gap);
        // Two nominal 60Hz intervals = 2 / 60 seconds. This is not a dropped-frame count.
        self.long_gaps += u64::from(gap > Duration::from_nanos(33_333_333));
        // DesktopCamera / ProjectLayer cap integration at 0.1s; retain the raw delay here.
        self.clamped_gaps += u64::from(gap > Duration::from_millis(100));
    }
}

impl DesktopWorkspaceUiState {
    pub(super) fn start_motion_log(
        &mut self,
        kind: &'static str,
        subject: &DesktopCanvasSubject,
        source: CanvasRect,
        target: CanvasRect,
        now: Instant,
    ) {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let previous_transition_id = self.motion_log.as_ref().map(|log| log.id);
        self.finish_motion_log("retargeted", now);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let source = self.transition.as_ref().map(|t| t.rect()).unwrap_or(source);
        tracing::info!(
            target: "desktop_motion",
            event = "start", pid = std::process::id(), transition_id = id,
            ?previous_transition_id, kind, ?subject, ?source, requested_target = ?target,
            from_workspace = ?self.active_id, from_project = ?self.expanded_area_id,
            project_areas = self.areas.len(), loaded_sessions = self.sessions.len(),
            "desktop transition started"
        );
        self.motion_log = Some(DesktopMotionLog {
            id,
            kind,
            started: now,
            first_callback: None,
            callbacks: 0,
            max_gap: Duration::ZERO,
            long_gaps: 0,
            clamped_gaps: 0,
        });
    }

    pub(super) fn finish_motion_log(&mut self, outcome: &'static str, now: Instant) {
        let Some(log) = self.motion_log.take() else {
            return;
        };
        tracing::info!(
            target: "desktop_motion",
            event = "end", pid = std::process::id(), transition_id = log.id,
            kind = log.kind, outcome,
            elapsed_ms = now.saturating_duration_since(log.started).as_secs_f64() * 1000.0,
            callbacks = log.callbacks,
            first_callback_ms = ?log.first_callback.map(|gap| gap.as_secs_f64() * 1000.0),
            max_callback_gap_ms = log.max_gap.as_secs_f64() * 1000.0,
            long_callback_gaps = log.long_gaps, clamped_callback_gaps = log.clamped_gaps,
            "desktop transition ended"
        );
    }
}
