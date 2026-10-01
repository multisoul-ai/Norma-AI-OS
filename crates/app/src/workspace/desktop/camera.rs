use crate::app::MultisoulApp;
use canvas::{CanvasNode, CanvasRect, Vec2, Viewport};
use gpui::{Context, Window};
use std::time::Duration;

pub(super) const DESKTOP_SCENE_SIDE: f32 = 14.0;
pub(super) const DESKTOP_SCENE_TOP: f32 = 36.0;
pub(super) const DESKTOP_SCENE_BOTTOM: f32 = 91.0;

pub(crate) fn desktop_scene_rect(width: f32, height: f32) -> CanvasRect {
    CanvasRect::new(
        DESKTOP_SCENE_SIDE,
        DESKTOP_SCENE_TOP,
        (width - 2.0 * DESKTOP_SCENE_SIDE).max(1.0),
        (height - DESKTOP_SCENE_TOP - DESKTOP_SCENE_BOTTOM).max(1.0),
    )
}

pub(super) fn canvas_rect_from_bounds(bounds: gpui::Bounds<gpui::Pixels>) -> CanvasRect {
    CanvasRect::new(
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    )
}

fn embedded_viewport(viewport: Viewport, full_w: f32, full_h: f32, frame: CanvasRect) -> Viewport {
    let scale = (frame.size.x / full_w.max(1.0))
        .min(frame.size.y / full_h.max(1.0))
        .clamp(0.05, 1.0);
    Viewport {
        pan: Vec2::new(viewport.pan.x * scale, viewport.pan.y * scale),
        zoom: viewport.zoom * scale,
    }
}

fn project_preview_viewport(nodes: &[CanvasNode], card: CanvasRect) -> Option<Viewport> {
    let stage = super::preview_canvas::project_stage_rect(card.size, 1.0);
    let mut viewport = super::preview_canvas::fitted_viewport(
        nodes.iter().map(|node| node.bounds),
        stage.size.x,
        stage.size.y,
        1.0,
    )?;
    viewport.pan = viewport.pan + stage.origin;
    Some(viewport)
}

fn project_transition_viewport(
    viewport: Viewport,
    card: CanvasRect,
    frame: CanvasRect,
    full_width: f32,
    nodes: &[CanvasNode],
) -> Option<Viewport> {
    let preview = project_preview_viewport(nodes, card)?;
    let target_width = (full_width - DESKTOP_SCENE_SIDE * 2.0).max(1.0);
    let progress =
        ((frame.size.x - card.size.x) / (target_width - card.size.x).max(1.0)).clamp(0.0, 1.0);
    if progress == 1.0 {
        return Some(viewport);
    }
    Some(Viewport {
        pan: Vec2::new(
            preview.pan.x + (viewport.pan.x - preview.pan.x) * progress,
            preview.pan.y + (viewport.pan.y - preview.pan.y) * progress,
        ),
        zoom: preview.zoom + (viewport.zoom - preview.zoom) * progress,
    })
}

#[derive(Clone, Debug)]
pub(super) struct DesktopCamera {
    values: [f32; 4],
    velocity: [f32; 4],
    target: [f32; 4],
    active: bool,
}

impl DesktopCamera {
    pub(super) fn new(rect: CanvasRect) -> Self {
        let values = [rect.origin.x, rect.origin.y, rect.size.x, rect.size.y];
        Self {
            values,
            velocity: [0.0; 4],
            target: values,
            active: false,
        }
    }

    pub(super) fn rect(&self) -> CanvasRect {
        CanvasRect::new(
            self.values[0],
            self.values[1],
            self.values[2],
            self.values[3],
        )
    }

    pub(super) fn retarget(&mut self, rect: CanvasRect, reduce_motion: bool) {
        self.target = [rect.origin.x, rect.origin.y, rect.size.x, rect.size.y];
        if reduce_motion {
            self.values = self.target;
            self.velocity = [0.0; 4];
            self.active = false;
        } else {
            self.active = self.values != self.target || self.velocity != [0.0; 4];
        }
    }

    pub(super) fn tick(&mut self, delta: Duration) -> bool {
        if !self.active {
            return false;
        }
        let mut remaining = delta.as_secs_f32().min(0.1);
        while remaining > 0.0 {
            let dt = remaining.min(0.016);
            remaining -= dt;
            for axis in 0..4 {
                let acceleration =
                    225.0 * (self.target[axis] - self.values[axis]) - 30.0 * self.velocity[axis];
                self.velocity[axis] += acceleration * dt;
                self.values[axis] += self.velocity[axis] * dt;
            }
        }
        if (0..4).all(|axis| {
            (self.values[axis] - self.target[axis]).abs() < 0.2 && self.velocity[axis].abs() < 1.0
        }) {
            self.values = self.target;
            self.velocity = [0.0; 4];
            self.active = false;
        }
        true
    }

    pub(super) fn is_active(&self) -> bool {
        self.active
    }
}

#[derive(Clone, Debug)]
pub(super) enum DesktopCanvasSubject {
    Workspace(String),
    Project {
        workspace_id: String,
        area_id: String,
    },
}

#[derive(Clone, Debug)]
pub(super) struct DesktopCanvasTransition {
    pub(super) subject: DesktopCanvasSubject,
    camera: DesktopCamera,
}

impl DesktopCanvasTransition {
    pub(super) fn new(
        subject: DesktopCanvasSubject,
        source: CanvasRect,
        target: CanvasRect,
    ) -> Self {
        let mut camera = DesktopCamera::new(source);
        camera.retarget(target, false);
        Self { subject, camera }
    }

    pub(super) fn retarget(&mut self, subject: DesktopCanvasSubject, target: CanvasRect) {
        self.subject = subject;
        self.camera.retarget(target, false);
    }

    pub(super) fn rect(&self) -> CanvasRect {
        self.camera.rect()
    }

    pub(super) fn tick(&mut self, delta: Duration) -> bool {
        self.camera.tick(delta)
    }

    pub(super) fn is_active(&self) -> bool {
        self.camera.is_active()
    }
}

impl MultisoulApp {
    pub(crate) fn desktop_canvas_visual_geometry(
        &self,
        viewport: Viewport,
        full_w: f32,
        full_h: f32,
        nodes: &[CanvasNode],
    ) -> (Viewport, f32, f32) {
        let size = desktop_scene_rect(full_w, full_h).size;
        let Some(transition) = self.desktop_workspace.transition.as_ref() else {
            return (viewport, size.x, size.y);
        };
        let DesktopCanvasSubject::Project {
            workspace_id,
            area_id,
        } = &transition.subject
        else {
            return (viewport, size.x, size.y);
        };
        let frame = transition.rect();
        let card = self
            .desktop_workspace
            .area_rects
            .lock()
            .expect("project bounds mutex poisoned")
            .get(&(workspace_id.clone(), area_id.clone()))
            .copied();
        let visual_viewport = card
            .and_then(|card| project_transition_viewport(viewport, card, frame, full_w, nodes))
            .unwrap_or_else(|| embedded_viewport(viewport, size.x, size.y, frame));
        (visual_viewport, frame.size.x, frame.size.y)
    }

    pub(crate) fn desktop_workspace_zoom_active(&self) -> bool {
        matches!(
            self.desktop_workspace
                .transition
                .as_ref()
                .map(|transition| &transition.subject),
            Some(DesktopCanvasSubject::Workspace(_))
        )
    }

    pub(crate) fn desktop_project_zoom_active(&self) -> bool {
        matches!(
            self.desktop_workspace
                .transition
                .as_ref()
                .map(|transition| &transition.subject),
            Some(DesktopCanvasSubject::Project { .. })
        )
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn desktop_forget_project_bounds_for_e2e(&mut self) {
        self.desktop_workspace
            .area_rects
            .lock()
            .expect("project bounds mutex poisoned")
            .clear();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn desktop_advance_camera_for_e2e(&mut self, delta: Duration, cx: &mut Context<Self>) {
        if let Some(transition) = self.desktop_workspace.transition.as_mut() {
            transition.tick(delta);
        }
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn desktop_finish_camera_for_e2e(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_desktop_zoom(window, cx);
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn desktop_enable_camera_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.animate_camera_for_e2e = true;
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn desktop_disable_camera_for_headless_e2e(&mut self, cx: &mut Context<Self>) {
        self.desktop_workspace.animate_camera_for_e2e = false;
        cx.notify();
    }

    pub(super) fn begin_desktop_workspace_zoom(
        &mut self,
        workspace_id: &str,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let full = desktop_focus_rect(window);
        if !self.desktop_workspace_zoom_active() {
            self.desktop_workspace.workspace_motion_rects = self
                .desktop_workspace
                .workspace_rects
                .lock()
                .expect("workspace bounds mutex poisoned")
                .clone();
        }
        let card = self
            .desktop_workspace
            .workspace_motion_rects
            .get(workspace_id)
            .copied()
            .unwrap_or_else(|| centered_source_rect(full));
        self.desktop_workspace
            .workspace_motion_rects
            .insert(workspace_id.to_owned(), card);
        let (source, target) = if reverse { (full, card) } else { (card, full) };
        self.begin_desktop_zoom(
            DesktopCanvasSubject::Workspace(workspace_id.to_owned()),
            source,
            target,
            window,
            cx,
        );
    }

    pub(super) fn begin_desktop_project_zoom(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let size = window.viewport_size();
        let full = desktop_scene_rect(f32::from(size.width), f32::from(size.height));
        let card = self
            .desktop_workspace
            .area_rects
            .lock()
            .expect("project bounds mutex poisoned")
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
            .copied()
            .unwrap_or_else(|| centered_source_rect(full));
        let (source, target) = if reverse { (full, card) } else { (card, full) };
        self.begin_desktop_zoom(
            DesktopCanvasSubject::Project {
                workspace_id: workspace_id.to_owned(),
                area_id: area_id.to_owned(),
            },
            source,
            target,
            window,
            cx,
        );
    }

    fn begin_desktop_zoom(
        &mut self,
        subject: DesktopCanvasSubject,
        source: CanvasRect,
        target: CanvasRect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The headless harness opts out; a real .app built with e2e-test-support still animates.
        #[cfg(feature = "e2e-test-support")]
        if !self.desktop_workspace.animate_camera_for_e2e {
            self.desktop_workspace.transition = None;
            cx.notify();
            return;
        }
        if self.motion.reduce_motion {
            self.desktop_workspace.transition = None;
            cx.notify();
            return;
        }
        if let Some(transition) = self.desktop_workspace.transition.as_mut() {
            transition.retarget(subject, target);
        } else {
            self.desktop_workspace.transition =
                Some(DesktopCanvasTransition::new(subject, source, target));
        }
        self.desktop_workspace.transition_last_tick = cx.background_executor().now();
        self.schedule_desktop_zoom_frame(window, cx);
        cx.notify();
    }

    fn schedule_desktop_zoom_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.desktop_workspace.transition_frame_pending {
            return;
        }
        self.desktop_workspace.transition_frame_pending = true;
        cx.on_next_frame(window, |app, window, cx| {
            app.desktop_workspace.transition_frame_pending = false;
            let now = cx.background_executor().now();
            let delta = now.saturating_duration_since(app.desktop_workspace.transition_last_tick);
            app.desktop_workspace.transition_last_tick = now;
            if let Some(transition) = app.desktop_workspace.transition.as_mut() {
                transition.tick(delta);
                if transition.is_active() {
                    app.schedule_desktop_zoom_frame(window, cx);
                } else {
                    app.finish_desktop_zoom(window, cx);
                }
                if let Some(DesktopCanvasTransition {
                    subject:
                        DesktopCanvasSubject::Project {
                            workspace_id,
                            area_id,
                        },
                    ..
                }) = app.desktop_workspace.transition.as_ref()
                {
                    if let Some(session) = app
                        .desktop_workspace
                        .sessions
                        .get(&(workspace_id.clone(), area_id.clone()))
                    {
                        session.update(cx, |_, cx| cx.notify());
                    }
                }
                cx.notify();
            }
        });
    }

    fn finish_desktop_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(finished) = self.desktop_workspace.transition.take() else {
            return;
        };
        match finished.subject {
            DesktopCanvasSubject::Workspace(workspace_id) => {
                if self.desktop_workspace.pending_navigation.is_none()
                    && matches!(self.screen, crate::app::AppScreen::DesktopWorkspace)
                    && self.desktop_workspace.active_id.as_deref() == Some(&workspace_id)
                {
                    if let Some(area_id) = self.desktop_workspace.expanded_area_id.clone() {
                        self.expand_desktop_project_area(area_id, window, cx);
                    }
                }
            }
            DesktopCanvasSubject::Project {
                workspace_id,
                area_id,
            } => {
                if let Some(session) = self
                    .desktop_workspace
                    .sessions
                    .get(&(workspace_id, area_id.clone()))
                {
                    if self.desktop_workspace.expanded_area_id.as_deref() != Some(&area_id) {
                        session.update(cx, |session, cx| session.detach_project_window(cx));
                    } else {
                        session.update(cx, |_, cx| cx.notify());
                    }
                }
            }
        }
        self.finish_desktop_navigation(window, cx);
    }
}

fn desktop_focus_rect(window: &Window) -> CanvasRect {
    let size = window.viewport_size();
    CanvasRect::new(0.0, 0.0, f32::from(size.width), f32::from(size.height))
}

fn centered_source_rect(full: CanvasRect) -> CanvasRect {
    let width = full.size.x * 0.32;
    let height = full.size.y * 0.42;
    CanvasRect::new(
        full.origin.x + (full.size.x - width) * 0.5,
        full.origin.y + (full.size.y - height) * 0.5,
        width,
        height,
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_camera_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_workspace_entry_tests.rs"]
mod workspace_entry_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_switcher_motion_tests.rs"]
mod switcher_motion_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_project_dock_tests.rs"]
mod project_dock_tests;
