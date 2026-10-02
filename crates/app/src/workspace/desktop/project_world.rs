use super::camera::desktop_scene_rect;
use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::{
    div, point, prelude::*, px, scene, size, AnyView, Context, Div, Stateful, StyleRefinement,
    Window,
};

impl MultisoulApp {
    pub(super) fn render_desktop_project_transition_layer(
        &self,
        session: gpui::Entity<MultisoulApp>,
        workspace_name: String,
        frame: CanvasRect,
        returning: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if self
            .desktop_workspace
            .transition
            .as_ref()
            .is_some_and(|t| t.projects.is_some())
        {
            return self.render_desktop_project_switch(cx);
        }
        let selector = if returning {
            "desktop-project-receding"
        } else {
            "desktop-project-expanded"
        };
        div()
            .id(selector)
            .debug_selector(move || selector.to_string())
            .absolute()
            .inset_0()
            .child(self.render_desktop_project_world(session, workspace_name, Some(frame), window))
    }

    pub(super) fn render_desktop_project_switch(&self, _cx: &mut Context<Self>) -> Stateful<Div> {
        let transition = self
            .desktop_workspace
            .transition
            .as_ref()
            .expect("switch transition");
        let projects = transition.projects.as_ref().expect("project presentation");
        let workspace = self
            .desktop_workspace
            .active_id
            .as_deref()
            .unwrap_or_default();
        let camera = transition.camera.rect();
        let mut world = div()
            .id("desktop-project-switch")
            .debug_selector(|| "desktop-project-switch".to_string())
            .absolute()
            .inset_0()
            .overflow_hidden();
        for layer in &projects.layers {
            if layer.opacity <= 0.0
                && self.desktop_workspace.expanded_area_id.as_deref() != Some(&layer.id)
            {
                continue;
            }
            let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace.to_owned(), layer.id.clone()))
            else {
                continue;
            };
            let frame = projects.rect(&layer.id, camera);
            let scale = frame.size.x / projects.logical_size.x * layer.scale;
            world = world.child(
                div()
                    .id(("desktop-switch-project", session.entity_id()))
                    .absolute()
                    .inset_0()
                    .child(
                        project_canvas_scene(session.clone(), projects.logical_size, scale)
                            .opacity(layer.opacity.clamp(0.0, 1.0))
                            .offset(point(
                                px(frame.origin.x),
                                px(frame.origin.y
                                    + (frame.size.y - projects.logical_size.y * scale) * 0.5),
                            )),
                    ),
            );
        }
        // Navigation remains available in the fixed Dock. Moving, fading cards cannot steal input.
        world.child(div().absolute().inset_0().occlude())
    }

    pub(super) fn render_desktop_project_world(
        &self,
        session: gpui::Entity<MultisoulApp>,
        _workspace_name: String,
        frame: Option<CanvasRect>,
        window: &Window,
    ) -> Stateful<Div> {
        let viewport = window.viewport_size();
        let full = desktop_scene_rect(f32::from(viewport.width), f32::from(viewport.height));
        let frame = frame.unwrap_or(full);
        div()
            .id("desktop-project-world")
            .debug_selector(|| "desktop-project-world".to_string())
            .absolute()
            .left(px(frame.origin.x))
            .top(px(frame.origin.y))
            .w(px(frame.size.x))
            .h(px(frame.size.y))
            .overflow_hidden()
            .child(project_canvas_scene(
                session,
                full.size,
                frame.size.x / full.size.x,
            ))
            .when(self.desktop_project_zoom_active(), |this| {
                this.child(div().absolute().inset_0().occlude())
            })
    }
}

/// One real session and one logical layout at every project presentation size.
pub(super) fn project_canvas_scene(
    session: gpui::Entity<MultisoulApp>,
    logical: canvas::Vec2,
    scale: f32,
) -> gpui::SceneElement {
    scene(
        ("project-presentation", session.entity_id()),
        size(px(logical.x), px(logical.y)),
        AnyView::from(session).cached(StyleRefinement::default().absolute().size_full()),
    )
    .scale(scale)
}
