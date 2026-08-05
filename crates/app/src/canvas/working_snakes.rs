use canvas::NodeId;
use gpui::{AppContext, Context};
use ui::Theme;

use crate::app::MultisoulApp;
use crate::cards::snake::{WorkingSnakePresentation, WorkingSnakeView};

#[derive(Clone, Copy)]
pub(super) struct WorkingSnakeGeometry {
    pub(super) visible: bool,
    pub(super) left_px: f32,
    pub(super) top_px: f32,
    pub(super) width_px: f32,
    pub(super) height_px: f32,
    pub(super) opacity: f32,
    pub(super) zoom: f32,
    pub(super) node_radius_px: f32,
}

pub(super) fn retain_live_views(app: &mut MultisoulApp, live_node_ids: &[NodeId]) -> bool {
    let count_before = app.working_snake_views.len();
    app.working_snake_views
        .retain(|node_id, _| live_node_ids.contains(node_id));
    count_before != app.working_snake_views.len()
}

pub(super) fn synchronize_view(
    app: &mut MultisoulApp,
    node_id: NodeId,
    active: bool,
    theme: Theme,
    geometry: WorkingSnakeGeometry,
    cx: &mut Context<MultisoulApp>,
) -> bool {
    let now = cx.background_executor().now();
    let presentation = WorkingSnakePresentation {
        active,
        visible: geometry.visible,
        reduce_motion: app.motion.reduce_motion,
        theme,
        left_px: geometry.left_px,
        top_px: geometry.top_px,
        width_px: geometry.width_px,
        height_px: geometry.height_px,
        opacity: geometry.opacity,
        zoom: geometry.zoom,
        node_radius_px: geometry.node_radius_px,
    };
    let (snake_view, inserted) = if let Some(snake_view) = app.working_snake_views.get(&node_id) {
        (snake_view.clone(), false)
    } else {
        let snake_view = cx.new(|_snake_cx| WorkingSnakeView::new(node_id, presentation, now));
        app.working_snake_views.insert(node_id, snake_view.clone());
        (snake_view, true)
    };
    snake_view.update(cx, |snake, snake_cx| {
        snake.synchronize(presentation, now, snake_cx);
    });
    inserted
}
