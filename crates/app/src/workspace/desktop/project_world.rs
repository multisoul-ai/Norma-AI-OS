use super::camera::{DESKTOP_SCENE_BOTTOM, DESKTOP_SCENE_SIDE, DESKTOP_SCENE_TOP};
use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::{div, prelude::*, px, AnyView, Context, Div, Stateful, StyleRefinement};

impl MultisoulApp {
    pub(super) fn render_desktop_project_transition_layer(
        &self,
        session: gpui::Entity<MultisoulApp>,
        workspace_name: String,
        frame: CanvasRect,
        returning: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
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
            .child(self.render_desktop_project_world(session, workspace_name, Some(frame), cx))
    }

    pub(super) fn render_desktop_project_world(
        &self,
        session: gpui::Entity<MultisoulApp>,
        _workspace_name: String,
        frame: Option<CanvasRect>,
        _cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut world = div()
            .id("desktop-project-world")
            .debug_selector(|| "desktop-project-world".to_string())
            .absolute()
            .overflow_hidden()
            .child(
                AnyView::from(session).cached(StyleRefinement::default().absolute().size_full()),
            );
        world = if let Some(frame) = frame {
            world
                .left(px(frame.origin.x))
                .top(px(frame.origin.y))
                .w(px(frame.size.x))
                .h(px(frame.size.y))
        } else {
            world
                .left(px(DESKTOP_SCENE_SIDE))
                .right(px(DESKTOP_SCENE_SIDE))
                .top(px(DESKTOP_SCENE_TOP))
                .bottom(px(DESKTOP_SCENE_BOTTOM))
        };
        world
    }
}
