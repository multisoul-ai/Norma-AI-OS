//! Small opt-in records through the application's existing logger; no trace or pixel buffers.

use crate::{frame_timing, Scene};
use block::ConcreteBlock;
use metal::{CommandBufferRef, DrawableRef, MetalDrawableRef};
use objc::{msg_send, sel, sel_impl};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    fn CACurrentMediaTime() -> f64;
}

pub(super) struct FrameProbe {
    id: u64,
    pid: u32,
    started: Instant,
    started_at: f64,
}

impl FrameProbe {
    pub(super) fn start() -> Option<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let started = frame_timing::start()?;
        // 300 seconds × 120Hz = 36,000 samples: also bound pathological repaint loops.
        let id = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
                (id <= 36_000).then_some(id + 1)
            })
            .ok()?;
        Some(Self {
            id,
            pid: std::process::id(),
            started,
            started_at: unsafe { CACurrentMediaTime() },
        })
    }

    pub(super) fn attach(&self, command: &CommandBufferRef, drawable: &MetalDrawableRef) {
        let id = self.id;
        let pid = self.pid;
        command.add_completed_handler(&ConcreteBlock::new(move |buffer: &CommandBufferRef| {
            // Metal guarantees these timestamps only after command-buffer completion.
            let start: f64 = unsafe { msg_send![buffer, GPUStartTime] };
            let end: f64 = unsafe { msg_send![buffer, GPUEndTime] };
            log::info!(target: "gpui_frame", "gpu pid={} id={} start_s={:.9} end_s={:.9} elapsed_ms={:?} status={:?}",
                pid, id, start, end, frame_timing::elapsed_ms(start, end), buffer.status());
        }).copy());
        drawable.add_presented_handler(
            &ConcreteBlock::new(move |drawable: &DrawableRef| {
                // This is the actual drawable presentation timestamp, not callback arrival time.
                log::info!(target: "gpui_frame", "present pid={} id={} drawable_id={} presented_s={:.9}",
                pid, id, drawable.drawable_id(), drawable.presented_time());
            })
            .copy(),
        );
    }

    pub(super) fn submitted(self, scene: &Scene, width: i32, height: i32, scale: f64) {
        let submit_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        // The same Scene may be presented repeatedly without another content build.
        let build_ms = scene
            .cpu_build_duration
            .take()
            .map(|d| d.as_secs_f64() * 1000.0);
        log::info!(target: "gpui_frame", "submit pid={} id={} start_s={:.9} build_ms={:?} submit_ms={:.6} cpu_total_ms={:.6} width_px={} height_px={} scale={:.3}",
            self.pid, self.id, self.started_at, build_ms, submit_ms, build_ms.unwrap_or(0.0) + submit_ms,
            width, height, scale);
    }
}
