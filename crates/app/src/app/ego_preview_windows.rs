//! Ego screen capture needs a native Windows capture provider; expose the capability state.
use super::MultisoulApp;
use canvas::NodeId;
use gpui::{Context, RenderImage};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct EgoPreviewStore;
impl EgoPreviewStore {
    pub(crate) fn frame_for(&self, _node_id: NodeId) -> Option<Arc<RenderImage>> {
        None
    }
    pub(crate) fn error_for(&self, _node_id: NodeId) -> Option<String> {
        Some(ui::i18n::text("cards.ego_capture_unavailable").into())
    }
}
impl MultisoulApp {
    pub(crate) fn clear_ego_previews(&mut self, _cx: &mut Context<Self>) {}
}
