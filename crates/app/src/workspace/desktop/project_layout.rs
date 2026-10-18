//! Presentation geometry only. Never mutates a project's document, viewport or PTY.
use crate::{app::MultisoulApp, motion::util::lerp_rect};
use canvas::{CanvasRect, Vec2};
use gpui::{Context, Window};
use std::{collections::HashMap, time::Duration};

pub(super) struct ProjectLayout {
    workspace: String,
    viewport: Vec2,
    ids: Vec<String>,
    from: Vec<CanvasRect>,
    current: Vec<CanvasRect>,
    target: Vec<CanvasRect>,
    elapsed: Duration,
}

impl Default for ProjectLayout {
    fn default() -> Self {
        Self {
            workspace: String::new(),
            viewport: Vec2::ZERO,
            ids: Vec::new(),
            from: Vec::new(),
            current: Vec::new(),
            target: Vec::new(),
            elapsed: Duration::ZERO,
        }
    }
}

// Prototype 23's geometry transition: .33s cubic-bezier(.2,.8,.2,1).
const REFLOW_DURATION: Duration = Duration::from_millis(330);

fn geometry_progress(t: f32) -> f32 {
    if t <= 0.0 || t >= 1.0 {
        return t.clamp(0.0, 1.0);
    }
    let (mut low, mut high) = (0.0, 1.0);
    // 12 bisections bound the curve parameter error to 1/4096.
    for _ in 0..12 {
        let u = (low + high) * 0.5;
        if 0.6 * (1.0 - u) * u + u * u * u < t {
            low = u;
        } else {
            high = u;
        }
    }
    let u = (low + high) * 0.5;
    2.4 * (1.0 - u).powi(2) * u + 3.0 * (1.0 - u) * u * u + u * u * u
}

impl ProjectLayout {
    fn sync(&mut self, workspace: &str, viewport: Vec2, ids: &[String], animate: bool) -> bool {
        if self.workspace == workspace && self.viewport == viewport && self.ids == ids {
            if !animate && self.is_active() {
                self.settle();
            }
            return false;
        }
        let same_workspace = self.workspace == workspace;
        let reason = if !same_workspace {
            "workspace"
        } else if self.viewport != viewport {
            "viewport"
        } else {
            "projects"
        };
        let target = super::layout::project_area_layout(viewport.x, viewport.y, ids.len());
        let previous: HashMap<_, _> = self.ids.iter().zip(&self.current).collect();
        let from = ids
            .iter()
            .zip(&target)
            .map(|(id, target)| {
                if animate && same_workspace {
                    previous.get(id).map_or(*target, |rect| **rect)
                } else {
                    *target
                }
            })
            .collect::<Vec<_>>();
        if self.is_active() {
            tracing::info!(target: "desktop_motion", event="project_layout_end", workspace=%self.workspace, outcome="retargeted");
        }
        self.workspace = workspace.to_owned();
        self.viewport = viewport;
        self.ids = ids.to_vec();
        self.current = from.clone();
        self.from = from;
        self.target = target;
        self.elapsed = Duration::ZERO;
        tracing::info!(target: "desktop_motion", event="project_layout", workspace,
            reason, projects=ids.len(), rows=2, columns=ids.len().div_ceil(2).max(2),
            ?viewport, animated=self.is_active(), "project regions updated");
        true
    }

    pub(super) fn is_active(&self) -> bool {
        self.current != self.target
    }

    pub(super) fn settle(&mut self) {
        self.current.clone_from(&self.target);
        tracing::info!(target: "desktop_motion", event="project_layout_end", workspace=%self.workspace,
            elapsed_ms=self.elapsed.as_millis(), outcome="completed");
    }

    pub(super) fn tick(&mut self, delta: Duration) {
        self.elapsed += delta;
        if self.elapsed >= REFLOW_DURATION {
            self.settle();
        } else {
            let progress =
                geometry_progress(self.elapsed.as_secs_f32() / REFLOW_DURATION.as_secs_f32());
            for ((current, from), target) in
                self.current.iter_mut().zip(&self.from).zip(&self.target)
            {
                *current = lerp_rect(*from, *target, progress);
            }
        }
    }
}

impl MultisoulApp {
    pub(super) fn sync_desktop_project_layout(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.desktop_workspace.active_id.as_deref() else {
            return;
        };
        let size = window.viewport_size();
        let viewport = Vec2::new(
            self.desktop_assistant_layout(window).canvas_width,
            f32::from(size.height),
        );
        let ids: Vec<_> = self
            .desktop_workspace
            .areas
            .iter()
            .map(|area| area.id.clone())
            .collect();
        // Reflows pause during navigation; the camera uses the last presented rectangles.
        // Changed window geometry while expanded snaps to the new overview destination.
        let same_geometry = self.desktop_workspace.project_layout.viewport == viewport
            && self.desktop_workspace.project_layout.ids == ids;
        let animate = !self.motion.reduce_motion
            && (same_geometry
                || (self.desktop_workspace.transition.is_none()
                    && self.desktop_workspace.expanded_area_id.is_none()));
        self.desktop_workspace
            .project_layout
            .sync(workspace, viewport, &ids, animate);
        if self.desktop_project_reflow_active() {
            if !self.desktop_workspace.transition_frame_pending {
                self.desktop_workspace.transition_last_tick = cx.background_executor().now();
            }
            self.schedule_desktop_zoom_frame(window, cx);
        }
    }

    pub(super) fn desktop_project_reflow_active(&self) -> bool {
        self.desktop_workspace.project_layout.is_active()
            && self.desktop_workspace.transition.is_none()
            && self.desktop_workspace.expanded_area_id.is_none()
    }

    pub(super) fn desktop_project_rects<'a>(
        &self,
        workspace: &str,
        viewport: Vec2,
        ids: impl ExactSizeIterator<Item = &'a str>,
    ) -> Vec<CanvasRect> {
        let layout = &self.desktop_workspace.project_layout;
        let count = ids.len();
        if layout.workspace == workspace
            && layout.viewport == viewport
            && layout.ids.iter().map(String::as_str).eq(ids)
        {
            layout.current.clone()
        } else {
            super::layout::project_area_layout(viewport.x, viewport.y, count)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_project_reflow_tests.rs"]
mod tests;
