use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::{Context, Window};
use std::time::Duration;

pub(super) const DESKTOP_SCENE_SIDE: f32 = 14.0;
// Share the fixed title bar's height, with 12px of space below it for project cards.
pub(super) const DESKTOP_SCENE_TOP: f32 = crate::app::TOP_BAR_HEIGHT + 12.0;
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
    pub(super) camera: DesktopCamera,
    pub(super) projects: Option<super::project_switch::ProjectSwitch>,
    /// The enclosing workspace's real content slot in the directory. A project camera
    /// can target this nested slot directly without an intermediate workspace animation.
    pub(super) directory_content: Option<CanvasRect>,
    /// The directory camera's (overview, expanded) ends; shells fade by camera progress
    /// between them, independent of cached live-card geometry.
    pub(super) directory_ends: Option<(CanvasRect, CanvasRect)>,
}

impl DesktopCanvasTransition {
    pub(super) fn new(
        subject: DesktopCanvasSubject,
        source: CanvasRect,
        target: CanvasRect,
    ) -> Self {
        let mut camera = DesktopCamera::new(source);
        camera.retarget(target, false);
        Self {
            subject,
            camera,
            projects: None,
            directory_content: None,
            directory_ends: None,
        }
    }

    pub(super) fn retarget(&mut self, subject: DesktopCanvasSubject, target: CanvasRect) {
        let camera_target = if let (Some(projects), DesktopCanvasSubject::Project { area_id, .. }) =
            (&mut self.projects, &subject)
        {
            projects.select(area_id);
            projects.camera_target(area_id, target)
        } else {
            target
        };
        self.subject = subject;
        self.camera.retarget(camera_target, false);
    }

    pub(super) fn rect(&self) -> CanvasRect {
        if let (Some(projects), DesktopCanvasSubject::Project { area_id, .. }) =
            (&self.projects, &self.subject)
        {
            projects.rect(area_id, self.camera.rect())
        } else {
            self.camera.rect()
        }
    }

    pub(super) fn tick(&mut self, delta: Duration) -> bool {
        if let Some(projects) = &mut self.projects {
            projects.tick(delta);
        }
        self.camera.tick(delta)
    }

    pub(super) fn is_active(&self) -> bool {
        self.camera.is_active() || self.projects.as_ref().is_some_and(|p| p.is_active())
    }
}

impl MultisoulApp {
    pub(crate) fn desktop_directory_zoom_active(&self) -> bool {
        self.desktop_workspace_zoom_active()
            || self
                .desktop_workspace
                .transition
                .as_ref()
                .is_some_and(|t| t.directory_content.is_some())
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
        let full = self.desktop_final_focus_rect(window);
        let Some(card) = self.desktop_workspace_directory_rect(workspace_id, window) else {
            return;
        };
        let (source, target) = if reverse { (full, card) } else { (card, full) };
        self.begin_desktop_zoom(
            if reverse {
                "workspace_overview"
            } else {
                "workspace_expand"
            },
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
        let full = desktop_scene_rect(
            self.desktop_assistant_layout(window).canvas_width,
            f32::from(size.height),
        );
        let Some(card) = self.desktop_project_overview_rect(workspace_id, area_id, window) else {
            return;
        };
        let (source, target) = if reverse { (full, card) } else { (card, full) };
        self.begin_desktop_zoom(
            if reverse {
                "project_overview"
            } else {
                "project_expand"
            },
            DesktopCanvasSubject::Project {
                workspace_id: workspace_id.to_owned(),
                area_id: area_id.to_owned(),
            },
            source,
            target,
            window,
            cx,
        );
        if reverse {
            if let Some(projects) = self
                .desktop_workspace
                .transition
                .as_mut()
                .and_then(|t| t.projects.as_mut())
            {
                projects.show_overview();
            }
        }
    }

    pub(super) fn desktop_project_overview_rect(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        window: &Window,
    ) -> Option<CanvasRect> {
        let size = window.viewport_size();
        let drawer = self.desktop_assistant_layout(window);
        let full = desktop_scene_rect(drawer.canvas_width, f32::from(size.height));
        let index = self
            .desktop_workspace
            .areas
            .iter()
            .position(|area| area.id == area_id)?;
        let layout = self.desktop_capture_project_rects(workspace_id, drawer, window);
        Some(super::preview_canvas::project_canvas_rect(
            layout[index],
            full.size,
            1.0,
        ))
    }

    /// Record every project card of the live workspace laid out between `drawer`'s drawers.
    pub(super) fn desktop_capture_project_rects(
        &mut self,
        workspace_id: &str,
        drawer: super::drawers::motion::DesktopDrawerLayout,
        window: &Window,
    ) -> Vec<CanvasRect> {
        let size = window.viewport_size();
        let width = drawer.canvas_width;
        let layout = self.desktop_project_rects(
            workspace_id,
            canvas::Vec2::new(width, f32::from(size.height)),
            self.desktop_workspace
                .areas
                .iter()
                .map(|area| area.id.as_str()),
        );
        let scale = window.scale_factor();
        let snap = |value: f32| (value * scale).round() / scale;
        let parent_x = snap(drawer.left_space);
        let layout = layout
            .into_iter()
            .map(|rect| {
                let left = snap(drawer.left_space + rect.origin.x);
                let right = snap(drawer.left_space + rect.origin.x + rect.size.x);
                let top = snap(rect.origin.y);
                let bottom = snap(rect.origin.y + rect.size.y);
                CanvasRect::new(left - parent_x, top, right - left, bottom - top)
            })
            .collect::<Vec<_>>();
        {
            let mut rects = self
                .desktop_workspace
                .area_rects
                .lock()
                .expect("project bounds mutex poisoned");
            for (area, rect) in self.desktop_workspace.areas.iter().zip(&layout) {
                rects.insert((workspace_id.to_owned(), area.id.clone()), *rect);
            }
        }
        layout
    }

    fn desktop_final_focus_rect(&self, window: &Window) -> CanvasRect {
        // This is called while the source is still the Launchpad. The destination
        // must use the workspace drawer geometry before `screen` changes.
        self.desktop_workspace_expanded_rect(window.viewport_size())
    }

    pub(super) fn begin_desktop_zoom(
        &mut self,
        kind: &'static str,
        subject: DesktopCanvasSubject,
        source: CanvasRect,
        target: CanvasRect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.desktop_workspace.start_motion_log(
            kind,
            &subject,
            source,
            target,
            cx.background_executor().now(),
        );
        // The headless harness opts out; a real .app built with e2e-test-support still animates.
        #[cfg(feature = "e2e-test-support")]
        if !self.desktop_workspace.animate_camera_for_e2e {
            self.desktop_workspace
                .finish_motion_log("headless_disabled", cx.background_executor().now());
            self.desktop_workspace.transition = None;
            cx.notify();
            return;
        }
        if self.motion.reduce_motion {
            self.desktop_workspace
                .finish_motion_log("reduced_motion", cx.background_executor().now());
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

#[cfg(test)]
#[path = "../../../tests/unit/desktop_camera_origins_tests.rs"]
mod origins_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_direct_directory_tests.rs"]
mod direct_directory_tests;
