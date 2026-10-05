use super::DesktopDispatchReceipt;
use crate::app::MultisoulApp;
use gpui::{Context, Window};
use runtime::desktop_dispatch::{route_desktop_command, DesktopAgentTarget};
use ui::component::input::InputState;

impl MultisoulApp {
    pub(super) fn send_desktop_workspace_command(
        &mut self,
        input: gpui::Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let text = input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let mut targets = Vec::new();
        for area in &self.desktop_workspace.areas {
            let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.clone(), area.id.clone()))
            else {
                targets.push((
                    area.id.clone(),
                    None,
                    DesktopAgentTarget {
                        project_name: area.display_name.clone(),
                        agent_name: String::new(),
                    },
                ));
                continue;
            };
            let session_app = session.read(cx);
            for node in session_app.runtime.document.nodes() {
                let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                    continue;
                };
                targets.push((
                    area.id.clone(),
                    Some(node.id),
                    DesktopAgentTarget {
                        project_name: area.display_name.clone(),
                        agent_name: spec
                            .alias
                            .clone()
                            .unwrap_or_else(|| spec.profile.display_title().to_string()),
                    },
                ));
            }
        }
        let names: Vec<_> = targets
            .iter()
            .map(|(_, _, target)| target.clone())
            .collect();
        let dispatches = match route_desktop_command(&text, &names) {
            Ok(dispatches) => dispatches,
            Err(clarification) => {
                self.desktop_workspace.error = Some(format!("Norma: {clarification}").into());
                cx.notify();
                return;
            }
        };
        let mut receipts = Vec::new();
        for dispatch in dispatches {
            let (area_id, node_id, target) = &targets[dispatch.target_index];
            let result = if self.desktop_workspace.area_errors.contains_key(area_id) {
                Err(ui::i18n::text("workspace.project_directory_unavailable").to_string())
            } else if node_id.is_none() {
                Err(ui::i18n::text("workspace.agent_session_is_still_loading").to_string())
            } else if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.clone(), area_id.clone()))
                .cloned()
            {
                session.update(cx, |session, cx| {
                    session.submit_compact_agent_text(
                        node_id.expect("loaded Agent target has a node ID"),
                        &dispatch.instruction,
                        cx,
                    )
                })
            } else {
                Err(ui::i18n::text("workspace.agent_session_is_still_loading").to_string())
            };
            receipts.push(DesktopDispatchReceipt {
                project_name: target.project_name.clone(),
                agent_name: target.agent_name.clone(),
                submitted: result.is_ok(),
                status: match result {
                    Ok(()) => ui::i18n::text("workspace.submitted").to_string(),
                    Err(error) => ui::i18n::format(
                        "workspace.failed_error",
                        &[("error", (error).to_string())],
                    ),
                },
            });
        }
        self.desktop_workspace
            .dispatch_receipts
            .insert(workspace_id, receipts);
        self.desktop_workspace.error = None;
        input.update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }
}
