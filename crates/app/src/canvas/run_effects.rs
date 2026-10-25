//! Agent 卡片运行特效的挂载与同步（同 `working_snakes.rs`）。
//! 经典风格继续走光蛇、呼吸与完成柔光；其他风格关掉这三者，改挂 `RunEffectView`。

use canvas::NodeId;
use gpui::{AppContext, Context};
use runtime::{AgentTaskState, CardAttentionState};
use ui::Theme;

use super::working_snakes::WorkingSnakeGeometry;
use crate::app::MultisoulApp;
use crate::cards::agent::card::{agent_breathing_intent, agent_chase_intent};
use crate::cards::chrome::TITLE_BAR_H;
use crate::cards::run_effect::{RunEffectPresentation, RunEffectView};
use crate::motion::breathing::BreathingIntent;
use crate::motion::run_style::{run_input, RunStyle};

/// 经典风格下的光蛇与呼吸意图；其他风格两者都关闭。
pub(super) fn classic_intents(
    style: RunStyle,
    is_agent: bool,
    state: Option<AgentTaskState>,
    attention: Option<CardAttentionState>,
) -> (bool, BreathingIntent) {
    if style != RunStyle::Classic {
        return (false, BreathingIntent::Inactive);
    }
    let chase = is_agent && agent_chase_intent(state.clone());
    (chase, agent_breathing_intent(state, attention))
}

/// 运行特效遵循系统“减少动态效果”（不提供关闭开关）；经典风格不读系统设置。
pub(super) fn reduce_motion(app: &MultisoulApp, style: RunStyle) -> bool {
    style != RunStyle::Classic
        && (app.motion.reduce_motion || crate::motion::system_reduce_motion())
}

pub(super) fn retain_live_views(app: &mut MultisoulApp, live_node_ids: &[NodeId]) -> bool {
    let before = app.run_effect_views.len();
    app.run_effect_views
        .retain(|node_id, _| live_node_ids.contains(node_id));
    before != app.run_effect_views.len()
}

pub(super) struct RunEffectSync<'a> {
    pub(super) style: RunStyle,
    pub(super) state: Option<&'a AgentTaskState>,
    pub(super) attention: Option<CardAttentionState>,
    pub(super) theme: Theme,
    pub(super) geometry: WorkingSnakeGeometry,
    pub(super) reduce_motion: bool,
}

/// 返回挂载集合是否变化（需要根视图重新挂载）。
pub(super) fn synchronize_view(
    app: &mut MultisoulApp,
    node_id: NodeId,
    sync: RunEffectSync,
    cx: &mut Context<MultisoulApp>,
) -> bool {
    if sync.style == RunStyle::Classic {
        return app.run_effect_views.remove(&node_id).is_some();
    }
    let g = sync.geometry;
    let presentation = RunEffectPresentation {
        style: sync.style,
        input: run_input(sync.state, sync.attention),
        visible: g.visible,
        reduce_motion: sync.reduce_motion,
        theme: sync.theme,
        left_px: g.left_px,
        top_px: g.top_px,
        width_px: g.width_px,
        height_px: g.height_px,
        opacity: g.opacity,
        zoom: g.zoom,
        node_radius_px: g.node_radius_px,
        title_h_px: TITLE_BAR_H * g.zoom,
    };
    if let Some(view) = app.run_effect_views.get(&node_id) {
        view.clone()
            .update(cx, |view, view_cx| view.synchronize(presentation, view_cx));
        return false;
    }
    let now = cx.background_executor().now();
    let view = cx.new(|_| RunEffectView::new(node_id, presentation, now));
    app.run_effect_views.insert(node_id, view);
    true
}

#[cfg(test)]
#[path = "../../tests/unit/canvas_run_effects_tests.rs"]
mod tests;
