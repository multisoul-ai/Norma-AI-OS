//! Master/detail layout state and app-layer application helpers.

use crate::app::{MultisoulApp, ToggleMasterLayout};
use crate::cards::card_for;
use crate::motion::MotionStore;
use canvas::{
    compute_master_detail_layout, compute_responsive_grid_layout, CanvasRect, LayoutInsets, NodeId,
    ResponsiveGridConfig, DEFAULT_CARD_ASPECT_RATIO,
};
use gpui::{Context, Window};
use runtime::{route, InputEvent, InputMode, NodeResize, Route, Runtime};
use std::collections::BTreeMap;
use storage::{LayoutMode, LayoutOriginalBounds, LayoutStateSnapshot};

const LAYOUT_GAP_SCREEN_PX: f32 = 16.0;
const LAYOUT_MAX_COLUMNS: usize = 3;
const LAYOUT_MIN_COLUMN_SCREEN_PX: f32 = 320.0;
const WORKSPACE_SAFE_LEFT_PX: f32 = 96.0;
const WORKSPACE_SAFE_TOP_PX: f32 = 86.0;
const WORKSPACE_SAFE_RIGHT_PX: f32 = 32.0;
const WORKSPACE_SAFE_BOTTOM_PX: f32 = 84.0;

#[derive(Debug, Default, Clone)]
pub(crate) struct MasterLayoutState {
    active: Option<ActiveMasterLayout>,
}

#[derive(Debug, Clone)]
struct ActiveMasterLayout {
    master_id: NodeId,
    original_bounds: BTreeMap<NodeId, CanvasRect>,
}

impl MasterLayoutState {
    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn master_id(&self) -> Option<NodeId> {
        self.active.as_ref().map(|active| active.master_id)
    }

    pub(crate) fn clear(&mut self) {
        self.active = None;
    }

    pub(crate) fn to_snapshot(&self) -> LayoutStateSnapshot {
        match &self.active {
            Some(active) => LayoutStateSnapshot {
                mode: LayoutMode::MasterDetail,
                master_id: Some(active.master_id),
                original_bounds: active
                    .original_bounds
                    .iter()
                    .map(|(node_id, bounds)| LayoutOriginalBounds {
                        node_id: *node_id,
                        bounds: *bounds,
                    })
                    .collect(),
            },
            None => LayoutStateSnapshot {
                mode: LayoutMode::Tiled,
                master_id: None,
                original_bounds: Vec::new(),
            },
        }
    }

    pub(crate) fn restore_from_snapshot(
        &mut self,
        snapshot: LayoutStateSnapshot,
        runtime: &Runtime,
    ) -> bool {
        if snapshot.mode == LayoutMode::Tiled {
            self.clear();
            return false;
        }

        let Some(master_id) = snapshot.master_id else {
            self.clear();
            return false;
        };
        if runtime.document.node(master_id).is_none() {
            self.clear();
            return false;
        }

        let original_bounds = snapshot
            .original_bounds
            .into_iter()
            .filter(|entry| runtime.document.node(entry.node_id).is_some())
            .map(|entry| (entry.node_id, entry.bounds))
            .collect();
        self.active = Some(ActiveMasterLayout {
            master_id,
            original_bounds,
        });
        true
    }

    fn has_current_nodes_outside_snapshot(&self, runtime: &Runtime) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        runtime
            .document
            .nodes()
            .iter()
            .any(|node| !active.original_bounds.contains_key(&node.id))
    }

    fn begin(&mut self, master_id: NodeId, runtime: &Runtime) -> bool {
        if runtime.document.node(master_id).is_none() {
            return false;
        }

        let original_bounds = runtime
            .document
            .nodes()
            .iter()
            .map(|node| (node.id, node.bounds))
            .collect();
        self.active = Some(ActiveMasterLayout {
            master_id,
            original_bounds,
        });
        true
    }

    fn restore(&mut self, runtime: &mut Runtime, motion: &mut MotionStore, zoom: f32) {
        let Some(active) = self.active.take() else {
            return;
        };

        for (node_id, bounds) in active.original_bounds {
            if runtime.document.node(node_id).is_none() {
                continue;
            }
            let node_type = runtime.document.node(node_id).map(|node| {
                motion.ensure_node(node_id, node.bounds);
                node.kind.node_type()
            });
            runtime.document.set_node_bounds(node_id, bounds);
            motion.set_layout_target(node_id, bounds);
            resize_node_for_bounds(runtime, node_id, node_type, bounds, zoom);
        }
    }

    fn apply(
        &self,
        runtime: &mut Runtime,
        motion: &mut MotionStore,
        area: CanvasRect,
        zoom: f32,
    ) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        apply_master_detail_for_area(runtime, motion, active.master_id, area, zoom)
    }
}

pub(crate) fn layout_gap_for_zoom(zoom: f32) -> f32 {
    LAYOUT_GAP_SCREEN_PX / zoom.max(0.001)
}

pub(crate) fn workspace_reflow_config_for_zoom(zoom: f32) -> ResponsiveGridConfig {
    let zoom = zoom.max(0.001);
    ResponsiveGridConfig {
        max_columns: LAYOUT_MAX_COLUMNS,
        min_column_width: LAYOUT_MIN_COLUMN_SCREEN_PX / zoom,
        aspect_ratio: DEFAULT_CARD_ASPECT_RATIO,
        gap_canvas: LAYOUT_GAP_SCREEN_PX / zoom,
        safe_area: LayoutInsets::new(
            WORKSPACE_SAFE_LEFT_PX / zoom,
            WORKSPACE_SAFE_TOP_PX / zoom,
            WORKSPACE_SAFE_RIGHT_PX / zoom,
            WORKSPACE_SAFE_BOTTOM_PX / zoom,
        ),
    }
}

pub(crate) fn term_grid_size_for_card(
    node_type: canvas::NodeType,
    bounds: CanvasRect,
    zoom: f32,
) -> (u16, u16) {
    match card_for(node_type).resize(bounds, zoom) {
        Some(NodeResize::TermGrid { cols, rows }) => (cols, rows),
        None => (80, 24),
    }
}

pub(crate) fn master_target_from_selection(runtime: &Runtime) -> Option<NodeId> {
    runtime
        .selection
        .iter()
        .find(|node_id| runtime.document.node(*node_id).is_some())
}

pub(crate) fn enter_master_layout_for_area(
    state: &mut MasterLayoutState,
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_id: NodeId,
    area: CanvasRect,
    zoom: f32,
) -> bool {
    if !state.begin(master_id, runtime) {
        return false;
    }

    runtime.selection.select_only(master_id);
    if matches!(runtime.focus.mode(), InputMode::NodeFocused(focused) if focused != master_id) {
        runtime.focus.exit_to_canvas();
    }

    if state.apply(runtime, motion, area, zoom) {
        true
    } else {
        state.clear();
        false
    }
}

pub(crate) fn switch_master_layout_for_area(
    state: &mut MasterLayoutState,
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_id: NodeId,
    area: CanvasRect,
    zoom: f32,
) -> bool {
    if runtime.document.node(master_id).is_none() {
        return false;
    }

    let Some(active) = state.active.as_mut() else {
        return false;
    };
    active.master_id = master_id;

    runtime.selection.select_only(master_id);
    if matches!(runtime.focus.mode(), InputMode::NodeFocused(focused) if focused != master_id) {
        runtime.focus.exit_to_canvas();
    }

    if state.apply(runtime, motion, area, zoom) {
        true
    } else {
        state.clear();
        false
    }
}

pub(crate) fn restore_master_layout_for_area(
    state: &mut MasterLayoutState,
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    zoom: f32,
) {
    state.restore(runtime, motion, zoom);
}

pub(crate) fn exit_master_layout_for_area(
    state: &mut MasterLayoutState,
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    area: CanvasRect,
    zoom: f32,
) -> bool {
    if !state.is_active() {
        return false;
    }

    if state.has_current_nodes_outside_snapshot(runtime) {
        state.clear();
        apply_workspace_reflow_for_area(runtime, motion, area, zoom);
    } else {
        restore_master_layout_for_area(state, runtime, motion, zoom);
    }
    true
}

pub(crate) fn reapply_master_layout_for_area(
    state: &mut MasterLayoutState,
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    area: CanvasRect,
    zoom: f32,
) -> bool {
    if state.apply(runtime, motion, area, zoom) {
        true
    } else {
        state.clear();
        false
    }
}

pub(crate) fn toggle_master_layout_for_area(
    state: &mut MasterLayoutState,
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    target: Option<NodeId>,
    area: CanvasRect,
    zoom: f32,
) -> bool {
    let event = InputEvent::ToggleMasterLayout {
        current_master: state.master_id(),
        target,
    };
    match route(&runtime.focus.mode(), &event) {
        Route::EnterMasterLayout(master_id) => {
            enter_master_layout_for_area(state, runtime, motion, master_id, area, zoom)
        }
        Route::SwitchMasterLayout(master_id) => {
            switch_master_layout_for_area(state, runtime, motion, master_id, area, zoom)
        }
        Route::ExitMasterLayout => exit_master_layout_for_area(state, runtime, motion, area, zoom),
        _ => false,
    }
}

pub(crate) fn apply_workspace_reflow_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    area: CanvasRect,
    zoom: f32,
) {
    let ordered_ids: Vec<_> = runtime
        .document
        .nodes()
        .iter()
        .map(|node| node.id)
        .collect();
    let assignments =
        compute_responsive_grid_layout(&ordered_ids, area, workspace_reflow_config_for_zoom(zoom));
    apply_layout_assignments(runtime, motion, assignments, zoom);
}

#[cfg(test)]
pub(crate) fn close_node_and_reflow_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    node_id: NodeId,
    area: CanvasRect,
    zoom: f32,
) -> bool {
    if !runtime.remove_node(node_id) {
        return false;
    }

    motion.clear_transient();
    motion.retain_nodes(runtime.document.nodes().iter().map(|node| node.id));
    apply_workspace_reflow_for_area(runtime, motion, area, zoom);
    true
}

pub(crate) fn close_node_and_update_layout_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    state: &mut MasterLayoutState,
    node_id: NodeId,
    area: CanvasRect,
    zoom: f32,
) -> bool {
    let removed_master = state.master_id() == Some(node_id);
    if !runtime.remove_node(node_id) {
        return false;
    }

    motion.clear_transient();
    motion.retain_nodes(runtime.document.nodes().iter().map(|node| node.id));

    if removed_master {
        state.clear();
        apply_workspace_reflow_for_area(runtime, motion, area, zoom);
    } else if state.is_active() {
        if !reapply_master_layout_for_area(state, runtime, motion, area, zoom) {
            apply_workspace_reflow_for_area(runtime, motion, area, zoom);
        }
    } else {
        apply_workspace_reflow_for_area(runtime, motion, area, zoom);
    }
    true
}

fn apply_master_detail_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_id: NodeId,
    area: CanvasRect,
    zoom: f32,
) -> bool {
    let ordered_ids: Vec<_> = runtime
        .document
        .nodes()
        .iter()
        .map(|node| node.id)
        .collect();
    let assignments = compute_master_detail_layout(
        &ordered_ids,
        master_id,
        area,
        workspace_reflow_config_for_zoom(zoom),
    );
    if assignments.is_empty() {
        return false;
    }

    apply_layout_assignments(runtime, motion, assignments, zoom);
    true
}

fn apply_layout_assignments(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    assignments: Vec<canvas::LayoutAssignment>,
    zoom: f32,
) {
    for assignment in assignments {
        let node_type = runtime.document.node(assignment.node_id).map(|node| {
            motion.ensure_node(assignment.node_id, node.bounds);
            node.kind.node_type()
        });
        runtime
            .document
            .set_node_bounds(assignment.node_id, assignment.bounds);
        motion.set_layout_target(assignment.node_id, assignment.bounds);
        resize_node_for_bounds(
            runtime,
            assignment.node_id,
            node_type,
            assignment.bounds,
            zoom,
        );
    }
}

fn resize_node_for_bounds(
    runtime: &mut Runtime,
    node_id: NodeId,
    node_type: Option<canvas::NodeType>,
    bounds: CanvasRect,
    zoom: f32,
) {
    if let Some(resize) = node_type.and_then(|node_type| card_for(node_type).resize(bounds, zoom)) {
        let _ = runtime.resize_node(node_id, resize);
    }
}

impl MultisoulApp {
    pub(crate) fn on_toggle_master_layout(
        &mut self,
        _: &ToggleMasterLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, crate::app::AppScreen::Canvas) {
            return;
        }

        let target = master_target_from_selection(&self.runtime);
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let changed = toggle_master_layout_for_area(
            &mut self.master_layout,
            &mut self.runtime,
            &mut self.motion,
            target,
            area,
            zoom,
        );
        if changed {
            self.mark_canvas_dirty();
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/master_layout_tests.rs"]
mod master_layout_tests;

#[cfg(test)]
#[path = "../../tests/unit/master_layout_persistence_tests.rs"]
mod master_layout_persistence_tests;

#[cfg(test)]
#[path = "../../tests/unit/master_layout_exit_tests.rs"]
mod master_layout_exit_tests;
