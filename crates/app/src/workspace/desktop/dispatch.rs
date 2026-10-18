use super::DesktopDispatchReceipt;
use crate::app::MultisoulApp;
use agent::{AdapterError, AdapterResult};
use gpui::{Context, Window};

#[cfg(test)]
#[path = "../../../tests/unit/desktop_workspace_task_tests.rs"]
mod dispatch_tests;

impl MultisoulApp {
    pub(crate) fn norma_locate_workspace_agent(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        node_id: canvas::NodeId,
        approval_run_id: Option<&str>,
        approval_turn_id: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<AdapterResult, AdapterError> {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            return host.update(cx, |host, cx| {
                host.norma_locate_workspace_agent_on_host(
                    workspace_id,
                    area_id,
                    node_id,
                    approval_run_id,
                    approval_turn_id,
                    window,
                    cx,
                )
            });
        }
        self.norma_locate_workspace_agent_on_host(
            workspace_id,
            area_id,
            node_id,
            approval_run_id,
            approval_turn_id,
            window,
            cx,
        )
    }

    fn norma_locate_workspace_agent_on_host(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        node_id: canvas::NodeId,
        approval_run_id: Option<&str>,
        approval_turn_id: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<AdapterResult, AdapterError> {
        if self.screen != crate::app::AppScreen::DesktopWorkspace
            || self.desktop_workspace.active_id.as_deref() != Some(workspace_id)
        {
            return Err(AdapterError::Rejected(
                "当前工作空间已变更，请重新查询目标".into(),
            ));
        }
        if !self
            .desktop_workspace
            .areas
            .iter()
            .any(|area| area.id == area_id && area.desktop_workspace_id == workspace_id)
        {
            return Err(AdapterError::Rejected("目标项目不在当前工作空间".into()));
        }
        if self.desktop_workspace.area_errors.contains_key(area_id) {
            return Err(AdapterError::Rejected(
                "项目目录不可用，无法定位 Agent".into(),
            ));
        }
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
            .ok_or_else(|| AdapterError::Rejected("Agent 所属项目尚未加载".into()))?;
        let valid_agent = session
            .read(cx)
            .runtime
            .document
            .node(node_id)
            .is_some_and(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)));
        if !valid_agent {
            return Err(AdapterError::Rejected(
                "Agent 卡片已失效，请重新查询目标".into(),
            ));
        }
        if approval_run_id.is_some() != approval_turn_id.is_some() {
            return Err(AdapterError::InvalidArguments(
                "审批定位缺少完整回合身份".into(),
            ));
        }
        if approval_run_id.is_some() {
            if !self.desktop_approval_is_current(
                area_id,
                node_id,
                approval_run_id,
                approval_turn_id,
                cx,
            ) {
                return Err(AdapterError::Rejected(
                    "审批请求已变化，请重新查询状态".into(),
                ));
            }
            self.locate_desktop_approval(
                area_id,
                node_id,
                approval_run_id,
                approval_turn_id,
                window,
                cx,
            );
        } else {
            self.locate_desktop_agent(area_id, node_id, window, cx);
        }
        if self.desktop_workspace.expanded_area_id.as_deref() != Some(area_id) {
            return Err(AdapterError::Rejected("项目未能切换到目标 Agent".into()));
        }
        Ok(AdapterResult::accepted("已定位 Agent 卡片").with_node_id(node_id))
    }

    pub(crate) fn norma_open_workspace_artifact(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        node_id: canvas::NodeId,
        turn_id: &str,
        source_path: &str,
        version: &str,
        cx: &mut Context<Self>,
    ) -> Result<AdapterResult, AdapterError> {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            return host.update(cx, |host, cx| {
                host.norma_open_workspace_artifact_on_host(
                    workspace_id,
                    area_id,
                    node_id,
                    turn_id,
                    source_path,
                    version,
                    cx,
                )
            });
        }
        self.norma_open_workspace_artifact_on_host(
            workspace_id,
            area_id,
            node_id,
            turn_id,
            source_path,
            version,
            cx,
        )
    }

    fn norma_open_workspace_artifact_on_host(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        node_id: canvas::NodeId,
        turn_id: &str,
        source_path: &str,
        version: &str,
        cx: &mut Context<Self>,
    ) -> Result<AdapterResult, AdapterError> {
        if self.screen != crate::app::AppScreen::DesktopWorkspace
            || self.desktop_workspace.active_id.as_deref() != Some(workspace_id)
        {
            return Err(AdapterError::Rejected(
                "当前工作空间已变更，请重新查询成果".into(),
            ));
        }
        if !self
            .desktop_workspace
            .areas
            .iter()
            .any(|area| area.id == area_id && area.desktop_workspace_id == workspace_id)
        {
            return Err(AdapterError::Rejected("成果来源项目已移除".into()));
        }
        let artifact = self
            .desktop_workspace
            .content_artifacts
            .iter()
            .find(|artifact| {
                artifact.area_id == area_id
                    && artifact.node_id == node_id
                    && artifact.turn_id == turn_id
                    && artifact.source_path == source_path
                    && artifact.version == version
            })
            .cloned()
            .ok_or_else(|| AdapterError::Rejected("成果引用已变化，请重新查询".into()))?;
        self.open_desktop_artifact(artifact, cx);
        Ok(AdapterResult::accepted("正在右侧打开成果；文件与版本仍在校验中").with_node_id(node_id))
    }

    pub(crate) fn norma_assign_workspace_task(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        node_id: canvas::NodeId,
        task: &str,
        cx: &mut Context<Self>,
    ) -> Result<AdapterResult, AdapterError> {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            return host.update(cx, |host, cx| {
                host.norma_assign_workspace_task_on_host(workspace_id, area_id, node_id, task, cx)
            });
        }
        self.norma_assign_workspace_task_on_host(workspace_id, area_id, node_id, task, cx)
    }

    fn norma_assign_workspace_task_on_host(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        node_id: canvas::NodeId,
        task: &str,
        cx: &mut Context<Self>,
    ) -> Result<AdapterResult, AdapterError> {
        if self.screen != crate::app::AppScreen::DesktopWorkspace
            || self.desktop_workspace.active_id.as_deref() != Some(workspace_id)
        {
            return Err(AdapterError::Rejected(
                "当前工作空间已变更，请重新查询目标".into(),
            ));
        }
        let area = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == area_id && area.desktop_workspace_id == workspace_id)
            .ok_or_else(|| AdapterError::Rejected("目标项目不在当前工作空间".into()))?;
        let project_name = area.display_name.clone();
        if self.desktop_workspace.area_errors.contains_key(area_id) {
            return Err(AdapterError::Rejected("项目目录不可用".into()));
        }
        if task.trim().is_empty() {
            return Err(AdapterError::InvalidArguments("任务正文不能为空".into()));
        }
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
            .cloned()
            .ok_or_else(|| AdapterError::Rejected("Agent 会话尚未加载".into()))?;
        let agent_name = session
            .read(cx)
            .runtime
            .document
            .node(node_id)
            .and_then(|node| {
                let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                Some(
                    spec.alias
                        .clone()
                        .unwrap_or_else(|| spec.profile.display_title().to_string()),
                )
            })
            .ok_or_else(|| AdapterError::Rejected("Agent 卡片已失效，请重新查询目标".into()))?;
        let submitted = session.update(cx, |session, cx| {
            session.submit_compact_agent_text(node_id, task, cx)
        });
        self.desktop_workspace
            .dispatch_receipts
            .entry(workspace_id.to_owned())
            .or_default()
            .push(DesktopDispatchReceipt {
                project_name: project_name.clone(),
                agent_name: agent_name.clone(),
                status: match &submitted {
                    Ok(()) => "已提交".into(),
                    Err(error) => format!("失败：{error}"),
                },
                submitted: submitted.is_ok(),
            });
        cx.notify();
        submitted.map_err(AdapterError::Rejected)?;
        Ok(
            AdapterResult::accepted(format!("{project_name} 的 {agent_name} 已接收任务"))
                .with_node_id(node_id),
        )
    }
}
