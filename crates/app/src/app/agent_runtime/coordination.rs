use canvas::{CanvasNodeKind, NodeId};
use runtime::InputMode;

use super::super::MultisoulApp;

impl MultisoulApp {
    pub(crate) fn current_canvas_context_for_provider(
        &self,
    ) -> Option<agent::CurrentCanvasContext> {
        let workspace_root = self.runtime.workspace_root()?;
        let project_name = workspace_root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| workspace_root.display().to_string());
        let nodes = self.runtime.document.nodes();
        let total_cards = nodes.len();
        let mut candidates = Vec::with_capacity(agent::MAX_CURRENT_CANVAS_CARDS);
        for (index, node) in nodes.iter().enumerate() {
            let position = index + 1;
            let kind = canvas_card_kind(&node.kind);
            let selected = self.runtime.selection.contains(node.id);
            let focused = matches!(
                self.runtime.focus.mode(),
                InputMode::NodeFocused(node_id) if node_id == node.id
            );
            let key = (canvas_card_priority(kind, selected, focused), node.id);
            let candidate = (key, node, position, kind, selected, focused);
            if candidates.len() < agent::MAX_CURRENT_CANVAS_CARDS {
                candidates.push(candidate);
                continue;
            }
            let Some((worst_index, worst)) = candidates
                .iter()
                .enumerate()
                .max_by_key(|(_, current)| current.0)
            else {
                continue;
            };
            if key < worst.0 {
                candidates[worst_index] = candidate;
            }
        }
        candidates.sort_by_key(|candidate| candidate.0);
        let cards = candidates
            .into_iter()
            .map(
                |(_, node, position, kind, selected, focused)| agent::CanvasCardContext {
                    node_id: node.id,
                    position,
                    kind,
                    title: node.kind.display_title(),
                    selected,
                    focused,
                },
            )
            .collect();
        Some(agent::CurrentCanvasContext {
            project_name,
            workspace_root: workspace_root.display().to_string(),
            total_cards,
            cards,
        })
    }

    pub(super) fn agent_roster_for_provider(&self) -> Vec<agent::AgentRosterEntry> {
        self.agent_definitions
            .definitions()
            .iter()
            .map(|definition| {
                let active_node_id = self.runtime.document.nodes().iter().find_map(|node| {
                    let CanvasNodeKind::Agent(spec) = &node.kind else {
                        return None;
                    };
                    (spec.definition_id == Some(definition.id)).then_some(node.id)
                });
                let busy = active_node_id.is_some_and(|node_id| {
                    matches!(
                        self.runtime.agent_task_state(node_id),
                        Some(runtime::AgentTaskState::Thinking)
                            | Some(runtime::AgentTaskState::RunningTool { .. })
                            | Some(runtime::AgentTaskState::WaitingApproval { .. })
                    )
                });
                agent::AgentRosterEntry {
                    definition_id: definition.id,
                    name: definition.name.clone(),
                    speech_alias: definition.speech_alias.clone(),
                    profile: norma_agent_profile(definition.runtime_profile),
                    enabled: definition.enabled,
                    active_node_id,
                    tracking: self.agent_tracking_for_profile(definition.runtime_profile),
                    busy,
                }
            })
            .collect()
    }

    fn agent_tracking_for_profile(
        &self,
        profile: canvas::AgentProfile,
    ) -> agent::AgentTrackingMode {
        #[cfg(any(test, feature = "e2e-test-support"))]
        {
            let _ = profile;
            return if self.agent_hook_server.runtime_config().is_some() {
                agent::AgentTrackingMode::Managed
            } else {
                agent::AgentTrackingMode::Untracked
            };
        }
        #[cfg(not(any(test, feature = "e2e-test-support")))]
        {
            let provider = orchestration_provider(profile);
            tracking_from_hook_component(
                &self
                    .orchestration_settings
                    .integration(provider)
                    .hook_component,
            )
        }
    }

    pub(crate) fn register_external_agent_root(
        &mut self,
        node_id: NodeId,
        task_title: Option<String>,
    ) -> bool {
        let Some((definition_id, profile)) = self.runtime.document.node(node_id).and_then(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
                .map(|definition_id| (definition_id, norma_agent_profile(spec.profile)))
        }) else {
            return false;
        };
        let roster = self.agent_roster_for_provider();
        let coordinator = match self.agent_task_coordinator.as_mut() {
            Some(coordinator) => {
                if let Err(error) = coordinator.sync_roster(roster) {
                    tracing::warn!(%error, node_id, "failed to sync external Agent root roster");
                    return false;
                }
                coordinator
            }
            None => {
                let Ok(coordinator) = agent::AgentTaskCoordinator::new(roster) else {
                    return false;
                };
                self.agent_task_coordinator.insert(coordinator)
            }
        };
        let task_title = task_title.unwrap_or_else(|| "用户直接任务".to_string());
        let Ok((receipt, created)) =
            coordinator.register_external_root(definition_id, node_id, task_title)
        else {
            return false;
        };
        if !created {
            return false;
        }
        if let Some(writer) = self.persistence.agent_dispatch_writer() {
            let tracking = match receipt.state {
                agent::AgentDispatchState::SubmittedUntracked => {
                    storage::AgentDispatchTracking::Untracked
                }
                agent::AgentDispatchState::Queued | agent::AgentDispatchState::SubmittedManaged => {
                    storage::AgentDispatchTracking::Managed
                }
            };
            writer.create_orchestration(storage::NewOrchestrationAgentDispatchTask {
                runtime_instance_id: coordinator.runtime_instance_id().to_string(),
                runtime_task_id: receipt.task_id,
                task_ref: receipt.task_ref.clone(),
                root_task_ref: receipt.root_task_ref,
                parent_task_ref: None,
                source_definition_id: None,
                source_node_id: None,
                source_provider: Some(profile_slug(profile).to_string()),
                target_definition_id: definition_id,
                target_node_id: node_id,
                original_instruction: receipt.task.clone(),
                task_text: receipt.task,
                state: storage::AgentDispatchTaskState::Submitted,
                tracking,
            });
        }
        true
    }

    pub(crate) fn advance_agent_task_queue(&mut self, node_id: NodeId) -> bool {
        let Some(definition_id) = self.runtime.document.node(node_id).and_then(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
        }) else {
            return false;
        };
        let runtime_state = self.runtime.agent_task_state(node_id);
        if matches!(runtime_state, Some(runtime::AgentTaskState::Ended)) {
            let coordinator_changed = self
                .agent_task_coordinator
                .as_mut()
                .and_then(|coordinator| {
                    coordinator.finish_all_with_state(definition_id, "interrupted")
                })
                .is_some_and(|(was_busy, cancelled_queued)| was_busy || cancelled_queued > 0);
            self.pending_agent_submissions.remove_node(node_id);
            if let Some(writer) = self.persistence.agent_dispatch_writer() {
                writer.interrupt_definition(definition_id);
            }
            return coordinator_changed;
        }
        let cancel_requested = self
            .agent_task_coordinator
            .as_ref()
            .is_some_and(|coordinator| coordinator.is_cancel_requested(definition_id));
        let (terminal_state, coordinator_state, terminal_error) = match runtime_state {
            Some(runtime::AgentTaskState::Done) if cancel_requested => (
                storage::AgentDispatchTaskState::Interrupted,
                "interrupted",
                None,
            ),
            Some(runtime::AgentTaskState::Done) => {
                (storage::AgentDispatchTaskState::Done, "succeeded", None)
            }
            Some(runtime::AgentTaskState::Failed { message }) => (
                storage::AgentDispatchTaskState::Failed,
                "failed",
                Some(message),
            ),
            _ => (
                storage::AgentDispatchTaskState::Interrupted,
                "interrupted",
                None,
            ),
        };
        let writer = self.persistence.agent_dispatch_writer().cloned();
        let next = self
            .agent_task_coordinator
            .as_mut()
            .and_then(|coordinator| {
                coordinator.finish_current_with_state(definition_id, coordinator_state)
            });
        if let Some(writer) = writer.as_ref() {
            writer.finish_current(definition_id, terminal_state, terminal_error);
        }
        if let Some(candidate) = next {
            self.pending_agent_submissions.enqueue_immediate(candidate);
            return true;
        }
        false
    }

    pub(crate) fn agent_completion_report_for_node(&self, node_id: NodeId) -> Option<String> {
        let definition_id = self.runtime.document.node(node_id).and_then(|node| {
            let CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            spec.definition_id
        })?;
        let name = self
            .agent_definitions
            .definitions()
            .iter()
            .find(|definition| definition.id == definition_id)?
            .name
            .as_str();
        let state = self.runtime.agent_task_state(node_id)?;
        agent_completion_report(name, &state)
    }
}

fn canvas_card_kind(kind: &CanvasNodeKind) -> agent::CanvasCardKind {
    match kind {
        CanvasNodeKind::Rect(_) => agent::CanvasCardKind::Rect,
        CanvasNodeKind::Term(_) => agent::CanvasCardKind::Terminal,
        CanvasNodeKind::Agent(_) => agent::CanvasCardKind::Agent,
        CanvasNodeKind::Preview(_) => agent::CanvasCardKind::Preview,
        CanvasNodeKind::Browser(_) => agent::CanvasCardKind::Browser,
    }
}

fn canvas_card_priority(kind: agent::CanvasCardKind, selected: bool, focused: bool) -> u8 {
    if focused {
        0
    } else if kind == agent::CanvasCardKind::Agent {
        1
    } else if selected {
        2
    } else {
        3
    }
}

pub(super) fn agent_completion_report(
    name: &str,
    state: &runtime::AgentTaskState,
) -> Option<String> {
    match state {
        runtime::AgentTaskState::Done => Some(format!("{name} 已完成任务")),
        runtime::AgentTaskState::Failed { message } => Some(format!("{name} 任务失败：{message}")),
        runtime::AgentTaskState::Ended => Some(format!("{name} 任务已中断")),
        runtime::AgentTaskState::Ready
        | runtime::AgentTaskState::Thinking
        | runtime::AgentTaskState::RunningTool { .. }
        | runtime::AgentTaskState::WaitingApproval { .. } => None,
    }
}

fn norma_agent_profile(profile: canvas::AgentProfile) -> norma::protocol::AgentProfile {
    match profile {
        canvas::AgentProfile::Claude => norma::protocol::AgentProfile::Claude,
        canvas::AgentProfile::Codex => norma::protocol::AgentProfile::Codex,
        canvas::AgentProfile::Cursor => norma::protocol::AgentProfile::Cursor,
        canvas::AgentProfile::Grok => norma::protocol::AgentProfile::Grok,
        canvas::AgentProfile::OpenCode => norma::protocol::AgentProfile::OpenCode,
    }
}

fn profile_slug(profile: norma::protocol::AgentProfile) -> &'static str {
    match profile {
        norma::protocol::AgentProfile::Claude => "claude",
        norma::protocol::AgentProfile::Codex => "codex",
        norma::protocol::AgentProfile::Cursor => "cursor",
        norma::protocol::AgentProfile::Grok => "grok",
        norma::protocol::AgentProfile::OpenCode => "opencode",
    }
}

#[cfg(not(any(test, feature = "e2e-test-support")))]
fn orchestration_provider(
    profile: canvas::AgentProfile,
) -> norma::orchestration::OrchestrationProvider {
    match profile {
        canvas::AgentProfile::Claude => norma::orchestration::OrchestrationProvider::Claude,
        canvas::AgentProfile::Codex => norma::orchestration::OrchestrationProvider::Codex,
        canvas::AgentProfile::Cursor => norma::orchestration::OrchestrationProvider::Cursor,
        canvas::AgentProfile::Grok => norma::orchestration::OrchestrationProvider::Grok,
        canvas::AgentProfile::OpenCode => norma::orchestration::OrchestrationProvider::OpenCode,
    }
}

#[cfg(any(test, not(feature = "e2e-test-support")))]
fn tracking_from_hook_component(
    component: &norma::orchestration::ComponentState,
) -> agent::AgentTrackingMode {
    if matches!(
        component,
        norma::orchestration::ComponentState::Ready { .. }
    ) {
        agent::AgentTrackingMode::Managed
    } else {
        agent::AgentTrackingMode::Untracked
    }
}

#[cfg(test)]
mod tests {
    use super::tracking_from_hook_component;
    use norma::orchestration::ComponentState;
    use std::path::PathBuf;

    /// Provider 级追踪：只有当前 Provider 的完整 Hook 为 Ready 时才允许 Managed。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   Ready states     = 1
    ///   non-ready states = Missing + Outdated + Conflict + Failed + Unsupported = 5
    ///   Total            = 1 + 5 = 6 个 Hook 组件状态
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 将当前 Provider 的 Ready Hook 投影到 Agent tracking。
    ///   2. 将 5 种不完整或不可用状态逐一投影。
    ///   3. 比较 Managed 与 Untracked 边界。
    ///
    /// 预期结果：
    ///   - 正断言：Ready 唯一映射为 Managed。
    ///   - 正断言：5 种非 Ready 状态均映射为 Untracked。
    ///   - 负断言：其他 Provider 的全局 Hook server 不得替代当前 Provider 完整性。
    #[test]
    fn only_ready_provider_hook_is_managed() {
        let ready = ComponentState::Ready {
            version: "1".to_string(),
        };
        let non_ready = [
            ComponentState::Missing,
            ComponentState::Outdated {
                installed_version: "0".to_string(),
                required_version: "1".to_string(),
            },
            ComponentState::Conflict {
                path: PathBuf::from("/tmp/unmanaged-hook"),
                reason: "unmanaged".to_string(),
            },
            ComponentState::Failed {
                error_code: "hook_invalid".to_string(),
                message: "invalid".to_string(),
            },
            ComponentState::Unsupported {
                reason: "unsupported".to_string(),
            },
        ];

        assert_eq!(
            tracking_from_hook_component(&ready),
            agent::AgentTrackingMode::Managed,
            "当前 Provider Hook 完整且 Ready 时必须启用 Managed 追踪"
        );
        assert!(
            non_ready.iter().all(|state| {
                tracking_from_hook_component(state) == agent::AgentTrackingMode::Untracked
            }),
            "Missing/Outdated/Conflict/Failed/Unsupported 均必须保持 Untracked"
        );
        assert_ne!(
            tracking_from_hook_component(&non_ready[0]),
            agent::AgentTrackingMode::Managed,
            "当前 Provider Hook 缺失时不得因其他 Provider Hook 在线而误判 Managed"
        );
    }
}
