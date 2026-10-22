use super::camera::DesktopCanvasSubject;
use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::{div, prelude::*, px, rgba, Context, Div, Window};
use ui::component::{
    button::{Button, ButtonVariants},
    Sizable, StyledExt,
};

pub(super) enum DesktopNavigation {
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

pub(super) const WORKSPACE_TITLE_HEIGHT: f32 = 40.0;

/// Fit the same workspace canvas below its title, then release the inset as the shell fades.
pub(super) fn workspace_content_rect(
    frame: CanvasRect,
    full: canvas::Vec2,
    chrome_scale: f32,
) -> CanvasRect {
    let padding = 8.0 * chrome_scale;
    let header = WORKSPACE_TITLE_HEIGHT * chrome_scale + padding;
    let width = (frame.size.x - 2.0 * padding).max(1.0);
    let height = (frame.size.y - header - padding).max(1.0);
    let scale = (width / full.x).min(height / full.y);
    let size = canvas::Vec2::new(full.x * scale, full.y * scale);
    CanvasRect::new(
        frame.origin.x + (frame.size.x - size.x) * 0.5,
        frame.origin.y + header + (height - size.y) * 0.5,
        size.x,
        size.y,
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
    /// Where an expanding workspace lands: the canvas between the open drawers. Directory
    /// shells must fade against this rect, not the window, or they linger at the end.
    pub(super) fn desktop_workspace_expanded_rect(
        &self,
        viewport: gpui::Size<gpui::Pixels>,
    ) -> CanvasRect {
        let layout = self.desktop_workspace_landing_layout(f32::from(viewport.width));
        CanvasRect::new(
            layout.left_space,
            0.0,
            layout.canvas_width,
            f32::from(viewport.height),
        )
    }

    /// Drawer geometry a camera leaving the directory lands in. Drawers are hidden while
    /// the directory is shown, so this is evaluated before `screen` changes.
    fn desktop_workspace_landing_layout(
        &self,
        width: f32,
    ) -> super::drawers::motion::DesktopDrawerLayout {
        self.desktop_workspace.drawer_motion.layout_with_widths(
            width,
            self.desktop_workspace.drawers.left_open,
            self.desktop_workspace.drawers.right_open,
            self.motion.reduce_motion,
            self.desktop_workspace.drawer_widths.left,
            self.desktop_workspace.drawer_widths.right,
        )
    }

    /// Opacity of a directory shell while its workspace or project camera is moving.
    /// `None` when that shell is not the transition source or its slot is unknown.
    pub(super) fn desktop_directory_chrome_opacity(
        &self,
        workspace_id: &str,
        viewport: gpui::Size<gpui::Pixels>,
    ) -> Option<f32> {
        if self.desktop_directory_workspace_id() != Some(workspace_id) {
            return None;
        }
        let transition = self.desktop_workspace.transition.as_ref()?;
        if transition.directory_content.is_some() {
            // A project camera magnifies the workspace past the window, so its frame has
            // no fixed landing rect; fade by the camera's own progress instead.
            let (overview, expanded) = transition.directory_ends?;
            return Some(overview_chrome_opacity(
                transition.camera.rect(),
                overview,
                expanded,
            ));
        }
        let source = self
            .desktop_workspace
            .workspace_motion_rects
            .get(workspace_id)?;
        Some(overview_chrome_opacity(
            transition.rect(),
            *source,
            self.desktop_workspace_expanded_rect(viewport),
        ))
    }

    /// Opacity of the destination workspace's drawers while its camera expands from
    /// the directory. `None` when no directory-to-workspace zoom is running.
    pub(crate) fn desktop_workspace_chrome_fade_in(&self, window: &gpui::Window) -> Option<f32> {
        if !self.desktop_workspace_zoom_active() {
            return None;
        }
        let id = self.desktop_directory_workspace_id()?;
        let overview = self.desktop_directory_chrome_opacity(id, window.viewport_size())?;
        Some((1.0 - overview).clamp(0.0, 1.0))
    }

    pub(super) fn desktop_directory_workspace_id(&self) -> Option<&str> {
        let transition = self.desktop_workspace.transition.as_ref()?;
        match &transition.subject {
            DesktopCanvasSubject::Workspace(id) => Some(id),
            DesktopCanvasSubject::Project { workspace_id, .. }
                if transition.directory_content.is_some() =>
            {
                Some(workspace_id)
            }
            _ => None,
        }
    }

    pub(super) fn begin_desktop_project_directory_zoom(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let size = window.viewport_size();
        let full = CanvasRect::new(0.0, 0.0, f32::from(size.width), f32::from(size.height));
        let Some(card) = self.desktop_workspace_directory_rect(workspace_id, window) else {
            return;
        };
        let Some(index) = self
            .desktop_workspace
            .areas
            .iter()
            .position(|area| area.id == area_id)
        else {
            return;
        };
        // The directory thumbnail is drawn against the full Launchpad viewport,
        // even while the live workspace uses only the width between drawers.
        let directory_layout = super::layout::project_area_layout(
            full.size.x,
            full.size.y,
            self.desktop_workspace.areas.len(),
        );
        let project = super::preview_canvas::project_canvas_rect(
            directory_layout[index],
            super::desktop_scene_rect(full.size.x, full.size.y).size,
            1.0,
        );
        let content = if self.desktop_workspace.home_directory {
            card
        } else {
            workspace_content_rect(card, full.size, 1.0)
        };
        let mut nested = project_rect(full, content, project);
        let layout = self.desktop_workspace_landing_layout(full.size.x);
        // The live workspace frame is anchored on the cached card; a card captured under
        // other drawers or another window size would land the workspace off its canvas.
        self.desktop_capture_project_rects(workspace_id, layout, window);
        let mut expanded = super::desktop_scene_rect(layout.canvas_width, full.size.y);
        expanded.origin.x += layout.left_space;
        let directory_logical = super::desktop_scene_rect(full.size.x, full.size.y);
        let live_scale = expanded.size.x / directory_logical.size.x;
        nested.size.x *= live_scale;
        nested.size.y *= live_scale;
        let (source, target) = if reverse {
            (expanded, nested)
        } else {
            (nested, expanded)
        };
        self.begin_desktop_zoom(
            if reverse {
                "project_to_directory"
            } else {
                "directory_to_project"
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
        if let Some(transition) = &mut self.desktop_workspace.transition {
            transition.directory_content = Some(content);
            transition.directory_ends = Some((nested, expanded));
            if reverse {
                if let Some(projects) = &mut transition.projects {
                    projects.show_overview();
                }
            }
        }
    }

    pub(super) fn clip_desktop_directory_workspace(
        &self,
        world: gpui::Stateful<Div>,
        window: &Window,
    ) -> gpui::Stateful<Div> {
        if self
            .desktop_workspace
            .transition
            .as_ref()
            .is_none_or(|t| t.directory_content.is_none())
        {
            return world;
        }
        let size = window.viewport_size();
        let full = CanvasRect::new(0.0, 0.0, f32::from(size.width), f32::from(size.height));
        let frame = self.desktop_scene_frame(full, false);
        div()
            .id("desktop-directory-workspace-clip")
            .absolute()
            .left(px(frame.origin.x))
            .top(px(frame.origin.y))
            .w(px(frame.size.x))
            .h(px(frame.size.y))
            .overflow_hidden()
            .child(
                world
                    .left(px(-frame.origin.x))
                    .top(px(-frame.origin.y))
                    .w(size.width)
                    .h(size.height),
            )
    }

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
                    .child(ui::i18n::text("workspace.a_canvas_for_your_projects")),
            )
            .child(
                div()
                    .text_size(px(13.0 * scale))
                    .text_color(rgba(super::GLASS_TEXT_SECONDARY))
                    .child(ui::i18n::text(
                        "workspace.add_a_project_area_to_start_working_with_its_agents",
                    )),
            )
            .child(
                Button::new("desktop-workspace-empty-add-project")
                    .tab_stop(interactive)
                    .debug_selector(|| "desktop-workspace-empty-add-project".to_string())
                    .child(
                        div()
                            .text_size(px(16.0 * scale))
                            .line_height(px(20.0 * scale))
                            .child(ui::i18n::text("shortcuts.add_project")),
                    )
                    .primary()
                    .with_size(px(32.0 * scale))
                    .h(px(32.0 * scale))
                    .px(px(16.0 * scale))
                    .rounded(px(6.0 * scale))
                    .when(interactive, |button| {
                        button.on_click(cx.listener(|app, _, window, cx| {
                            app.open_desktop_add_project(window, cx)
                        }))
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
                    self.bind_desktop_wallpaper(cx);
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
            } => {
                let source_area = if directory && transition.directory_content.is_some() {
                    transition
                        .projects
                        .as_ref()
                        .and_then(|projects| {
                            projects
                                .layers
                                .iter()
                                .find(|layer| layer.anchor == canvas::Vec2::ZERO)
                        })
                        .map(|layer| &layer.id)
                        .unwrap_or(area_id)
                } else {
                    area_id
                };
                self.desktop_workspace
                    .area_rects
                    .lock()
                    .expect("project bounds mutex poisoned")
                    .get(&(workspace_id.clone(), source_area.clone()))
                    .copied()
                    .map(|card| {
                        super::preview_canvas::project_canvas_rect(
                            card,
                            super::desktop_scene_rect(full.size.x, full.size.y).size,
                            1.0,
                        )
                    })
            }
            _ => None,
        };
        let frame = source
            .map(|source| project_rect(source, transition.rect(), full))
            .unwrap_or(full);
        if directory && matches!(transition.subject, DesktopCanvasSubject::Project { .. }) {
            transition
                .directory_content
                .map(|source| project_rect(source, frame, full))
                .unwrap_or(full)
        } else {
            frame
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_directory_landing_tests.rs"]
mod landing_tests;
