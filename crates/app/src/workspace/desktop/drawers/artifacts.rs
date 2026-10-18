use super::{glass_button_variant, GLASS_TEXT_MUTED, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use canvas::NodeId;
use gpui::{div, prelude::*, px, rgba, Context, Div, ElementId, Window};
use ui::component::{
    button::{Button, ButtonVariants},
    StyledExt,
};

impl MultisoulApp {
    pub(super) fn render_desktop_artifacts(&self, cx: &mut Context<Self>) -> Div {
        let theme = self.desktop_theme();
        let count = self.desktop_workspace.content_artifacts.len();
        div()
            .mt(px(10.0))
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .font_semibold()
                    .text_color(rgba(GLASS_TEXT_SECONDARY))
                    .child(format!("待看文件 · {count}")),
            )
            .when(count == 0, |list| {
                list.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(GLASS_TEXT_MUTED))
                        .child(if self.desktop_workspace.artifact_refreshing {
                            "正在同步任务成果…"
                        } else {
                            "暂无有任务来源的待看文件"
                        }),
                )
            })
            .children(
                self.desktop_workspace
                    .content_artifacts
                    .iter()
                    .enumerate()
                    .map(|(index, artifact)| {
                        let project_name = self
                            .desktop_workspace
                            .areas
                            .iter()
                            .find(|area| area.id == artifact.area_id)
                            .map(|area| area.display_name.as_str())
                            .unwrap_or("来源项目已移除");
                        let agent_name = self.desktop_artifact_agent_name(artifact, cx);
                        let file_name = artifact
                            .path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "文件".to_owned());
                        let turn = artifact.turn_id.chars().take(8).collect::<String>();
                        let artifact = artifact.clone();
                        Button::new(ElementId::Name(
                            format!("desktop-artifact-open-{index}").into(),
                        ))
                        .debug_selector(move || format!("desktop-artifact-open-{index}"))
                        .w_full()
                        .p(px(8.0))
                        .rounded(px(8.0))
                        .custom(glass_button_variant(theme, cx))
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .child(format!("{file_name} · 打开文件 →"))
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgba(GLASS_TEXT_SECONDARY))
                                        .child(format!(
                                            "{project_name} / {agent_name} · 回合 {turn}"
                                        )),
                                ),
                        )
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.open_desktop_artifact(artifact.clone(), cx);
                        }))
                    }),
            )
    }

    pub(crate) fn desktop_artifact_agent_name(
        &self,
        artifact: &super::super::DesktopArtifact,
        cx: &gpui::App,
    ) -> String {
        self.desktop_workspace
            .active_id
            .as_deref()
            .and_then(|workspace_id| {
                self.desktop_area_status_view(workspace_id, &artifact.area_id, cx)
                    .agents
                    .into_iter()
                    .find(|agent| agent.node_id == artifact.node_id)
                    .map(|agent| agent.name)
            })
            .unwrap_or_else(|| artifact.agent_profile.clone())
    }

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
            self.desktop_workspace.error = Some("审批请求已变化，请查看最新 Agent 状态".into());
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
