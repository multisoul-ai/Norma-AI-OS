//! Direct sibling navigation: one camera, stable project anchors, retained live content.
use super::camera::{
    desktop_scene_rect, DesktopCamera, DesktopCanvasSubject, DesktopCanvasTransition,
};
use crate::app::MultisoulApp;
use canvas::{CanvasRect, Vec2};
use gpui::{Context, Window};
use std::time::Duration;

// 23's inactive/active project widths. Opacity and local size have separate durations.
const INACTIVE_SCALE: f32 = 700.0 / 870.0;

fn ease_out(t: f32) -> f32 {
    if t <= 0.0 || t >= 1.0 {
        return t.clamp(0.0, 1.0);
    }
    let (mut low, mut high) = (0.0, 1.0);
    // 12 bisections bound the CSS curve parameter error to 1/4096.
    for _ in 0..12 {
        let u = (low + high) * 0.5;
        if 1.74 * (1.0 - u) * u * u + u * u * u < t {
            low = u;
        } else {
            high = u;
        }
    }
    let u = (low + high) * 0.5;
    3.0 * (1.0 - u) * u * u + u * u * u
}

#[derive(Clone, Debug)]
pub(super) struct ProjectLayer {
    pub id: String,
    pub anchor: Vec2,
    pub opacity: f32,
    pub scale: f32,
    start: [f32; 2],
    target: [f32; 2],
    elapsed: f32,
}

impl ProjectLayer {
    fn retarget(&mut self, selected: bool) {
        self.start = [self.opacity, self.scale];
        self.target = if selected {
            [1.0, 1.0]
        } else {
            [0.0, INACTIVE_SCALE]
        };
        self.elapsed = 0.0;
    }

    fn tick(&mut self, dt: f32) {
        self.elapsed += dt;
        let progress = |duration: f32| {
            let t = (self.elapsed / duration).min(1.0);
            // CSS ease-out = cubic-bezier(0, 0, .58, 1).
            ease_out(t)
        };
        self.opacity = self.start[0] + (self.target[0] - self.start[0]) * progress(0.18);
        self.scale = self.start[1] + (self.target[1] - self.start[1]) * progress(0.35);
    }
}

#[derive(Clone, Debug)]
pub(super) struct ProjectSwitch {
    pub layers: Vec<ProjectLayer>,
    pub logical_size: Vec2,
}

impl ProjectSwitch {
    pub fn rect(&self, id: &str, camera: CanvasRect) -> CanvasRect {
        let anchor = self
            .layers
            .iter()
            .find(|layer| layer.id == id)
            .map(|layer| layer.anchor)
            .unwrap_or(Vec2::ZERO);
        CanvasRect::new(
            camera.origin.x + anchor.x * camera.size.x,
            camera.origin.y + anchor.y * camera.size.x,
            camera.size.x,
            camera.size.y,
        )
    }

    pub fn camera_target(&self, id: &str, frame: CanvasRect) -> CanvasRect {
        let at = self.rect(id, CanvasRect::new(0.0, 0.0, frame.size.x, frame.size.y));
        CanvasRect::new(
            frame.origin.x - at.origin.x,
            frame.origin.y - at.origin.y,
            frame.size.x,
            frame.size.y,
        )
    }

    pub fn select(&mut self, id: &str) {
        for layer in &mut self.layers {
            layer.retarget(layer.id == id);
        }
    }

    pub fn show_overview(&mut self) {
        for layer in &mut self.layers {
            layer.retarget(true);
        }
    }

    pub fn tick(&mut self, delta: Duration) {
        for layer in &mut self.layers {
            layer.tick(delta.as_secs_f32().min(0.1));
        }
    }

    pub fn is_active(&self) -> bool {
        self.layers
            .iter()
            .any(|layer| layer.elapsed < 0.35 && layer.start != layer.target)
    }
}

impl MultisoulApp {
    pub(super) fn begin_desktop_project_switch(
        &mut self,
        workspace_id: &str,
        previous: &str,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.desktop_workspace.pending_navigation = None;
        let size = window.viewport_size();
        let full = desktop_scene_rect(f32::from(size.width), f32::from(size.height));
        let subject = DesktopCanvasSubject::Project {
            workspace_id: workspace_id.to_owned(),
            area_id: target.to_owned(),
        };
        self.desktop_workspace.start_motion_log(
            "project_switch",
            &subject,
            full,
            full,
            cx.background_executor().now(),
        );
        if self.motion.reduce_motion {
            self.desktop_workspace
                .finish_motion_log("reduced_motion", cx.background_executor().now());
            self.desktop_workspace.transition = None;
            return;
        }
        #[cfg(feature = "e2e-test-support")]
        if !self.desktop_workspace.animate_camera_for_e2e {
            self.desktop_workspace
                .finish_motion_log("headless_disabled", cx.background_executor().now());
            self.desktop_workspace.transition = None;
            return;
        }
        if let Some(transition) = self
            .desktop_workspace
            .transition
            .as_mut()
            .filter(|t| t.projects.is_some())
        {
            transition.retarget(subject, full);
        } else {
            let frame = self
                .desktop_workspace
                .transition
                .as_ref()
                .map(|t| t.rect())
                .unwrap_or(full);
            let layout = super::layout::project_area_layout(
                f32::from(size.width),
                f32::from(size.height),
                self.desktop_workspace.areas.len(),
            );
            let index = self
                .desktop_workspace
                .areas
                .iter()
                .position(|area| area.id == previous)
                .unwrap_or(0);
            let content_rect =
                |rect| super::preview_canvas::project_canvas_rect(rect, full.size, 1.0);
            let source = content_rect(layout[index]);
            let layers = self
                .desktop_workspace
                .areas
                .iter()
                .zip(layout)
                .map(|(area, rect)| {
                    let rect = content_rect(rect);
                    let selected = area.id == previous;
                    ProjectLayer {
                        id: area.id.clone(),
                        anchor: Vec2::new(
                            (rect.origin.x - source.origin.x) / source.size.x.max(1.0),
                            (rect.origin.y - source.origin.y) / source.size.x.max(1.0),
                        ),
                        opacity: if selected { 1.0 } else { 0.0 },
                        scale: if selected { 1.0 } else { INACTIVE_SCALE },
                        start: [0.0; 2],
                        target: [0.0; 2],
                        elapsed: 0.0,
                    }
                })
                .collect();
            let mut projects = ProjectSwitch {
                layers,
                logical_size: full.size,
            };
            projects.select(target);
            // Reuse velocity as well as geometry when a sibling click interrupts a zoom.
            let previous_transition = self.desktop_workspace.transition.take();
            let directory_content = previous_transition
                .as_ref()
                .and_then(|t| t.directory_content);
            let mut camera = previous_transition
                .map(|t| t.camera)
                .unwrap_or_else(|| DesktopCamera::new(frame));
            camera.retarget(projects.camera_target(target, full), false);
            self.desktop_workspace.transition = Some(DesktopCanvasTransition {
                subject,
                camera,
                projects: Some(projects),
                directory_content,
            });
        }
        self.desktop_workspace.transition_last_tick = cx.background_executor().now();
        self.schedule_desktop_zoom_frame(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_project_switch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_project_return_tests.rs"]
mod return_tests;
