//! Explicit read receipts and artifact navigation share the existing frozen-version identity.
use super::{
    actions::IslandTarget,
    controller::IslandController,
    style,
    view::{button, IslandView},
    IslandProject, IslandWorkspace,
};
use gpui::{div, prelude::*, px, rgb, AnyElement, Context};
use norma::mobile::{RunOverviewAttention, RunOverviewAttentionKind};
use ui::{component::Disableable, i18n::text};

fn save_artifact_receipts(
    settings: &storage::SettingsStore,
    workspace: &str,
    row: &RunOverviewAttention,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        matches!(
            row.kind,
            RunOverviewAttentionKind::File | RunOverviewAttentionKind::Review
        ),
        "not an artifact"
    );
    let version = row
        .fingerprint
        .as_deref()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("artifact has no version"))?;
    anyhow::ensure!(!row.paths.is_empty(), "artifact has no files");
    let keys: Vec<_> = row
        .paths
        .iter()
        .map(|path| {
            (
                crate::workspace::desktop::artifact_read_key(
                    workspace,
                    &row.area_id,
                    row.node_id,
                    version,
                    path,
                ),
                Some("true".to_owned()),
            )
        })
        .collect();
    settings.apply_string_changes(&keys)?;
    crate::mobile::project_events::changed();
    Ok(())
}

fn artifact_pending_id(route: &IslandTarget, row: &RunOverviewAttention) -> String {
    format!(
        "artifact:{}:{}:{}:{}:{:?}:{:?}:{}",
        route.source,
        route.workspace,
        route.area,
        row.node_id,
        row.fingerprint,
        row.kind,
        serde_json::to_string(&row.paths).expect("artifact paths are serializable")
    )
}

impl IslandController {
    pub fn mark_artifact_read(
        &mut self,
        route: IslandTarget,
        row: RunOverviewAttention,
        cx: &mut Context<Self>,
    ) {
        let database = self
            .model
            .read(cx)
            .workspaces
            .iter()
            .find(|ws| {
                ws.id == route.workspace
                    && ws.projects.iter().any(|p| {
                        p.source == route.source
                            && p.area_id == route.area
                            && p.overview.attention.contains(&row)
                    })
            })
            .map(|ws| ws.database.clone());
        let Some(database) = database else {
            self.message = Some(ui::i18n::LocalizedText::new("island.expired"));
            cx.notify();
            return;
        };
        let id = artifact_pending_id(&route, &row);
        if !self.pending.insert(id.clone()) {
            return;
        }
        let workspace = route.workspace.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    save_artifact_receipts(&storage::SettingsStore::new(database), &workspace, &row)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.pending.remove(&id);
                this.message = Some(ui::i18n::LocalizedText::new(if result.is_ok() {
                    "island.marked_read"
                } else {
                    "island.read_failed"
                }));
                if let Some(host) = route.host(cx) {
                    host.update(cx, |host, cx| host.note_workspace_activity_changed(cx));
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl IslandView {
    pub fn render_artifacts(
        &self,
        workspace: &IslandWorkspace,
        project: &IslandProject,
        only: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let route = IslandTarget::new(workspace, project);
        super::faces::result_cards(&project.overview.attention)
            .into_iter()
            .enumerate()
            .filter(|(_, row)| {
                let review = super::faces::review_id(
                    route.source,
                    &route.workspace,
                    &route.area,
                    row.node_id,
                    &row.turn_id,
                );
                only.is_none_or(|want| want == review)
            })
            .map(|(index, row)| {
                let id = format!(
                    "artifact:{}:{}:{}:{index}",
                    route.source, route.workspace, route.area
                );
                let waiting = self
                    .controller
                    .read(cx)
                    .pending
                    .contains(&artifact_pending_id(&route, row));
                let read_route = route.clone();
                let read_row = row.clone();
                let open_route = route.clone();
                let open_row = row.clone();
                style::card()
                    .child(style::card_heading(
                        &row.agent,
                        &row.agent,
                        format!("{} · {}", workspace.name, project.name),
                        text("island.artifact_title"),
                        text("island.artifact_label"),
                        style::OK,
                        runtime::island::bloub::Mood::Responded,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .children(row.paths.iter().map(|path| {
                                div()
                                    .px(px(7.))
                                    .py(px(2.))
                                    .rounded(px(6.))
                                    .bg(rgb(0x060606))
                                    .text_color(rgb(style::MUTED))
                                    .font_family("Menlo")
                                    .text_size(px(11.5))
                                    .child(path.clone())
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .justify_end()
                            .child(
                                button(format!("{id}-read"), text("island.mark_read"), false, cx)
                                    .disabled(waiting || row.fingerprint.is_none())
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.controller.update(cx, |state, cx| {
                                            state.mark_artifact_read(
                                                read_route.clone(),
                                                read_row.clone(),
                                                cx,
                                            )
                                        });
                                    })),
                            )
                            .child(
                                button(
                                    format!("{id}-open"),
                                    text("island.view_on_canvas"),
                                    true,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    move |view, _, _, cx| {
                                        view.controller.update(cx, |state, cx| {
                                            state.open_artifact(
                                                open_route.clone(),
                                                open_row.clone(),
                                                cx,
                                            )
                                        });
                                    },
                                )),
                            ),
                    )
                    .into_any_element()
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/island_artifact_tests.rs"]
mod tests;
