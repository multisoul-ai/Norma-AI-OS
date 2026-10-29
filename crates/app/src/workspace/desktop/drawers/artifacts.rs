use crate::app::MultisoulApp;
use canvas::NodeId;
use gpui::{Context, Window};

impl MultisoulApp {
    pub(crate) fn locate_desktop_approval(
        &mut self,
        area_id: &str,
        node_id: NodeId,
        expected_run: Option<&str>,
        expected_turn: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let valid =
            self.desktop_approval_is_current(area_id, node_id, expected_run, expected_turn, cx);
        if !valid {
            self.desktop_workspace.error =
                Some(ui::i18n::LocalizedText::new("drawers.approval_changed"));
            cx.notify();
            return;
        }
        self.locate_desktop_agent(area_id, node_id, window, cx);
    }

    pub(crate) fn desktop_approval_is_current(
        &self,
        area_id: &str,
        node_id: NodeId,
        expected_run: Option<&str>,
        expected_turn: Option<&str>,
        cx: &gpui::App,
    ) -> bool {
        self.desktop_workspace
            .active_id
            .as_ref()
            .and_then(|workspace_id| {
                self.desktop_workspace
                    .sessions
                    .get(&(workspace_id.clone(), area_id.to_owned()))
            })
            .and_then(|session| {
                session
                    .read(cx)
                    .mobile_card_snapshots()
                    .into_iter()
                    .find(|card| card.node_id == node_id)
            })
            .is_some_and(|card| {
                super::super::status::approval_request_matches(
                    &card.state,
                    card.run_id.as_deref(),
                    card.turn_id.as_deref(),
                    expected_run,
                    expected_turn,
                )
            })
    }
}
