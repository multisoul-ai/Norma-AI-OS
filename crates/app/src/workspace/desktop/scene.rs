use super::camera::DesktopCanvasSubject;
use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::{div, prelude::*, px, rgba, Context, Div, Window};
use ui::component::{
    button::{Button, ButtonVariants},
    Sizable, StyledExt,
};

pub(super) enum DesktopNavigation {
    Directory,
    Workspace(String),
    Project(String),
}

/// Fade the overview shell with the same geometry as its content, including same-width layouts.
pub(super) fn overview_chrome_opacity(
    frame: CanvasRect,
    overview: CanvasRect,
    expanded: CanvasRect,
) -> f32 {
    let distance = |rect: CanvasRect| {
        [
            rect.origin.x - expanded.origin.x,
            rect.origin.y - expanded.origin.y,
            rect.size.x - expanded.size.x,
            rect.size.y - expanded.size.y,
        ]
        .into_iter()
        .map(f32::abs)
        .fold(0.0_f32, f32::max)
    };
    (distance(frame) / distance(overview).max(1.0)).clamp(0.0, 1.0)
}

pub(super) fn directory_grid(width: f32, count: usize) -> (usize, f32, f32) {
    let available = if width >= 940.0 {
        3
    } else if width >= 610.0 {
        2
    } else {
        1
    };
    let columns = available.min(count.max(1));
    let card =
        ((width - 112.0 - 16.0 * (available - 1) as f32) / available as f32).clamp(280.0, 460.0);
    (
        columns,
        card,
        card * columns as f32 + 16.0 * (columns - 1) as f32,
    )
}

/// Project every sibling from the same stable anchor; animated bounds are never layout inputs.
pub(super) fn project_rect(source: CanvasRect, frame: CanvasRect, rect: CanvasRect) -> CanvasRect {
    let scale = frame.size.x / source.size.x.max(1.0);
    CanvasRect::new(
        frame.origin.x + (rect.origin.x - source.origin.x) * scale,
        frame.origin.y + (rect.origin.y - source.origin.y) * scale,
        rect.size.x * scale,
        rect.size.y * scale,
    )
}

impl MultisoulApp {
    pub(super) fn desktop_empty_canvas(
        &self,
        scale: f32,
        interactive: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(500.0 * scale))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(15.0 * scale))
            .child(
                div()
                    .text_size(px(24.0 * scale))
                    .font_semibold()
                    .text_color(rgba(super::GLASS_TEXT))
                    .child("A canvas for your projects"),
            )
            .child(
                div()
                    .text_size(px(13.0 * scale))
                    .text_color(rgba(super::GLASS_TEXT_SECONDARY))
                    .child("Add a project area to start working with its agents."),
            )
            .child(
                Button::new("desktop-workspace-empty-add-project")
                    .debug_selector(|| "desktop-workspace-empty-add-project".to_string())
                    .child(
                        div()
                            .text_size(px(16.0 * scale))
                            .line_height(px(20.0 * scale))
                            .child("Add Project"),
                    )
                    .primary()
                    .with_size(px(32.0 * scale))
                    .h(px(32.0 * scale))
                    .px(px(16.0 * scale))
                    .rounded(px(6.0 * scale))
                    .when(interactive, |button| {
                        button
                            .on_click(cx.listener(|app, _, _, cx| app.open_desktop_add_project(cx)))
                    }),
            )
    }

    pub(super) fn defer_desktop_navigation(
        &mut self,
        destination: DesktopNavigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.motion.reduce_motion {
            return false;
        }
        #[cfg(feature = "e2e-test-support")]
        if !self.desktop_workspace.animate_camera_for_e2e {
            return false;
        }
        let moving_subject = self
            .desktop_workspace
            .transition
            .as_ref()
            .map(|t| t.subject.clone());
        let project = match &moving_subject {
            Some(DesktopCanvasSubject::Project { area_id, .. }) => Some(area_id.clone()),
            Some(DesktopCanvasSubject::Workspace(_)) => None,
            None if matches!(self.screen, crate::app::AppScreen::DesktopWorkspace) => {
                self.desktop_workspace.expanded_area_id.clone()
            }
            None => None,
        };
        if let Some(area) = project {
            if !matches!(&destination, DesktopNavigation::Project(id) if id == &area) {
                let workspace = self.desktop_workspace.active_id.clone().unwrap_or_default();
                self.desktop_workspace.pending_navigation = Some(destination);
                self.begin_desktop_project_zoom(&workspace, &area, true, window, cx);
                // Preserve the remembered expanded area; only the current presentation recedes.
                self.desktop_workspace.expanded_area_id = None;
                self.close_desktop_switcher(cx);
                return true;
            }
        } else if let Some(DesktopCanvasSubject::Workspace(current)) = moving_subject {
            match &destination {
                DesktopNavigation::Project(_) => {
                    self.desktop_workspace.pending_navigation = Some(destination);
                    return true;
                }
                DesktopNavigation::Workspace(id) if id != &current => {
                    self.desktop_workspace.pending_navigation = Some(destination);
                    self.begin_desktop_workspace_zoom(&current, true, window, cx);
                    self.screen = crate::app::AppScreen::Launchpad;
                    self.close_desktop_switcher(cx);
                    return true;
                }
                _ => {}
            }
        }
        self.desktop_workspace.pending_navigation = None;
        false
    }

    pub(super) fn finish_desktop_navigation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(destination) = self.desktop_workspace.pending_navigation.take() else {
            return false;
        };
        match destination {
            DesktopNavigation::Directory => self.show_desktop_launchpad(window, cx),
            DesktopNavigation::Workspace(id) => self.open_desktop_workspace(id, window, cx),
            DesktopNavigation::Project(id) => self.expand_desktop_project_area(id, window, cx),
        }
        true
    }

    pub(super) fn desktop_scene_frame(&self, full: CanvasRect, directory: bool) -> CanvasRect {
        let Some(transition) = self.desktop_workspace.transition.as_ref() else {
            return full;
        };
        let source = match &transition.subject {
            DesktopCanvasSubject::Workspace(id) if directory => self
                .desktop_workspace
                .workspace_motion_rects
                .get(id)
                .copied(),
            DesktopCanvasSubject::Project {
                workspace_id,
                area_id,
            } if !directory => self
                .desktop_workspace
                .area_rects
                .lock()
                .expect("project bounds mutex poisoned")
                .get(&(workspace_id.clone(), area_id.clone()))
                .copied(),
            _ => None,
        };
        source
            .map(|source| project_rect(source, transition.rect(), full))
            .unwrap_or(full)
    }
}
