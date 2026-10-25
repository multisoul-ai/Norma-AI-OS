use super::*;
use std::collections::BTreeMap;
use storage::{LayoutMode, LayoutOriginalBounds, LayoutStateSnapshot};

#[derive(Debug, Default, Clone)]
pub(crate) struct MasterLayoutState {
    pub(super) active: Option<ActiveMasterLayout>,
    pub(super) repair_pending: Option<LayoutStateSnapshot>,
    pub(crate) chrome_in_parent: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ActiveMasterLayout {
    pub(super) master_id: NodeId,
    pub(super) original_bounds: BTreeMap<NodeId, CanvasRect>,
}

impl MasterLayoutState {
    fn invalidate(&mut self, snapshot: LayoutStateSnapshot) {
        self.active = None;
        self.repair_pending = Some(snapshot);
    }

    pub(super) fn accepts(&self, runtime: &Runtime, node_id: NodeId) -> bool {
        runtime.document.node(node_id).is_some_and(|node| {
            crate::canvas::visibility::embedded_node_visible(
                node.kind.node_type(),
                self.chrome_in_parent,
            )
        })
    }

    /// Desktop policy is bound after project restore, when legacy content may already be pruned.
    pub(crate) fn bind_desktop(&mut self, runtime: &Runtime) {
        let original = self.to_snapshot();
        self.chrome_in_parent = true;
        if self
            .master_id()
            .is_some_and(|id| !self.accepts(runtime, id))
        {
            self.invalidate(original);
        } else if let Some(active) = &mut self.active {
            active.original_bounds.retain(|id, _| {
                runtime.document.node(*id).is_some_and(|node| {
                    crate::canvas::visibility::embedded_node_visible(node.kind.node_type(), true)
                })
            });
            // A legacy detail can have been added after the original snapshot, then pruned
            // before restore. Reapply every restored Master once the desktop has an area.
            self.repair_pending = Some(original);
        }
    }

    pub(crate) fn take_repair_pending(&mut self) -> bool {
        self.repair_pending.take().is_some()
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn master_id(&self) -> Option<NodeId> {
        self.active.as_ref().map(|active| active.master_id)
    }

    pub(crate) fn clear(&mut self) {
        self.active = None;
        self.repair_pending = None;
    }

    pub(crate) fn forget_removed_node(&mut self, node_id: NodeId) {
        if self.master_id() == Some(node_id) {
            self.clear();
        } else if let Some(active) = &mut self.active {
            active.original_bounds.remove(&node_id);
        }
    }

    pub(crate) fn to_snapshot(&self) -> LayoutStateSnapshot {
        // Background projects may flush before a desktop frame supplies their layout area.
        // Keep the unfiltered repair cause until a real layout has replaced the old bounds.
        if let Some(snapshot) = &self.repair_pending {
            return snapshot.clone();
        }
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
            self.invalidate(snapshot);
            return false;
        };
        if !self.accepts(runtime, master_id) {
            self.invalidate(snapshot);
            return false;
        }

        self.repair_pending = snapshot
            .original_bounds
            .iter()
            .any(|entry| !self.accepts(runtime, entry.node_id))
            .then(|| snapshot.clone());
        let original_bounds = snapshot
            .original_bounds
            .into_iter()
            .filter(|entry| self.accepts(runtime, entry.node_id))
            .map(|entry| (entry.node_id, entry.bounds))
            .collect();
        self.active = Some(ActiveMasterLayout {
            master_id,
            original_bounds,
        });
        true
    }

    pub(super) fn has_current_nodes_outside_snapshot(&self, runtime: &Runtime) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| self.accepts(runtime, node.id))
            .any(|node| !active.original_bounds.contains_key(&node.id))
    }

    pub(super) fn begin(&mut self, master_id: NodeId, runtime: &Runtime) -> bool {
        if !self.accepts(runtime, master_id) {
            return false;
        }

        self.repair_pending = None;
        let original_bounds = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| self.accepts(runtime, node.id))
            .map(|node| (node.id, node.bounds))
            .collect();
        self.active = Some(ActiveMasterLayout {
            master_id,
            original_bounds,
        });
        true
    }

    pub(super) fn restore(&mut self, runtime: &mut Runtime, motion: &mut MotionStore, zoom: f32) {
        self.repair_pending = None;
        let Some(active) = self.active.take() else {
            return;
        };

        for (node_id, bounds) in active.original_bounds {
            if !self.accepts(runtime, node_id) {
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

    pub(super) fn apply(
        &mut self,
        runtime: &mut Runtime,
        motion: &mut MotionStore,
        area: CanvasRect,
        zoom: f32,
    ) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        if !self.accepts(runtime, active.master_id) {
            return false;
        }
        let mut config = workspace_reflow_config_for_zoom(zoom);
        config.safe_area = workspace_safe_area_for_zoom(zoom, self.chrome_in_parent);
        let applied = apply_master_detail_for_area(
            runtime,
            motion,
            active.master_id,
            area,
            zoom,
            config,
            self.chrome_in_parent,
        );
        if applied {
            self.repair_pending = None;
        }
        applied
    }
}
