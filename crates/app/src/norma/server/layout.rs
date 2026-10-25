//! Norma 命令所需的卡片摘要与新增卡片布局桥接。

use super::conversion::{card_bounds_to_canvas, norma_agent_profile};
use crate::canvas::layout_strategy::{
    apply_new_workspace_node_layout_for_area, workspace_bounds_for_index, CanvasLayoutStrategy,
};
use crate::motion::MotionStore;
use canvas::{CanvasNodeKind, CanvasRect};
use norma::protocol::{CardSummary, CardSummaryKind, NodeId, Placement};
use runtime::{InputMode, Runtime};

pub(super) struct CardCreationState<'a> {
    pub(super) runtime: &'a mut Runtime,
    pub(super) motion: &'a mut MotionStore,
    pub(super) master_layout: &'a mut crate::canvas::master_layout::MasterLayoutState,
}

pub(super) fn summarize_cards(runtime: &Runtime) -> Vec<CardSummary> {
    runtime
        .document
        .nodes()
        .iter()
        .map(|node| CardSummary {
            node_id: node.id,
            title: node.kind.display_title().to_string(),
            alias: match &node.kind {
                CanvasNodeKind::Agent(spec) => spec.alias.clone(),
                _ => None,
            },
            kind: match &node.kind {
                CanvasNodeKind::Rect(_) => CardSummaryKind::Rect,
                CanvasNodeKind::Term(_) => CardSummaryKind::Terminal,
                CanvasNodeKind::Agent(spec) => CardSummaryKind::Agent {
                    profile: norma_agent_profile(spec.profile),
                },
                CanvasNodeKind::Preview(_) => CardSummaryKind::Preview,
                CanvasNodeKind::Browser(_) => CardSummaryKind::Browser,
            },
            selected: runtime.selection.contains(node.id),
            focused: matches!(runtime.focus.mode(), InputMode::NodeFocused(id) if id == node.id),
        })
        .collect()
}

pub(super) fn apply_layout_for_new_node_or_master(
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    node_id: NodeId,
    area: CanvasRect,
    zoom: f32,
    strategy: CanvasLayoutStrategy,
) {
    if master_layout.is_active()
        && crate::canvas::master_layout::reapply_master_layout_for_area(
            master_layout,
            runtime,
            motion,
            area,
            zoom,
        )
    {
        return;
    }
    if apply_new_workspace_node_layout_for_area(runtime, motion, node_id, area, zoom, strategy) {
        master_layout.clear();
    }
}

pub(super) fn resolve_bounds(
    placement: Placement,
    area: CanvasRect,
    zoom: f32,
    strategy: CanvasLayoutStrategy,
    node_index: usize,
) -> CanvasRect {
    match placement {
        Placement::VisibleArea => workspace_bounds_for_index(node_index, area, zoom, strategy),
        Placement::Bounds(bounds) => card_bounds_to_canvas(bounds),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_pending_layout_tests.rs"]
mod pending_layout_tests;
