//! 大脑提议的落地：接收、保存、用户点“派发”或“忽略”。

use super::proposals::{check_target, Draft, Proposal, ProposalBlock, Proposals, RosterLite};
use crate::app::MultisoulApp;
use gpui::Context;
use norma::protocol::ReportPriority;
use std::time::SystemTime;

impl MultisoulApp {
    /// 第一次需要时读本项目的提议文件；之后只在内存里改，并在后台写回。
    pub(crate) fn load_brain_proposals(&mut self, cx: &mut Context<Self>) {
        if self.brain.proposals_ready
            || self.brain.proposals_loading
            || std::time::Instant::now() < self.brain.proposals_retry_at
        {
            return;
        }
        let (Some(dir), Some(identity)) = (
            super::owner::shared(cx).directory(),
            self.workspace_id.clone(),
        ) else {
            return;
        };
        let path = self
            .brain
            .proposals_path
            .clone()
            .unwrap_or_else(|| brain::migration::proposal_file(&dir, &identity));
        self.brain.proposals_path = Some(path.clone());
        self.brain.proposals_loading = true;
        self.brain.proposals_retry_at = std::time::Instant::now() + super::START_RETRY;
        let writer = self.brain.proposal_writer.clone();
        let scope = identity.clone();
        let work = cx.background_executor().spawn(async move {
            writer
                .flush()
                .recv()
                .map_err(std::io::Error::other)?
                .map_err(std::io::Error::other)?;
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    Proposals::from_scoped_json(&text, &scope).map_err(std::io::Error::other)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(Proposals::default())
                }
                Err(error) => Err(error),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                if app.workspace_id.as_ref() != Some(&identity) {
                    return;
                }
                app.brain.proposals_loading = false;
                match result {
                    Ok(proposals) => {
                        app.brain.proposals = proposals;
                        app.brain.proposals_ready = true;
                        app.note_workspace_activity_changed(cx);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "读取幕僚提议失败，保留原文件");
                        app.desktop_workspace.error = Some(ui::i18n::message(
                            "chrome.brain_proposals_load_failed",
                            &[("reason", error.to_string())],
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_brain_proposals(&mut self) {
        if let Some((path, identity)) = self
            .brain
            .proposals_path
            .as_ref()
            .zip(self.workspace_id.as_deref())
        {
            self.brain.proposals_dirty = true;
            self.brain.proposals_save = Some(
                self.brain
                    .proposal_writer
                    .save(path.clone(), self.brain.proposals.to_scoped_json(identity)),
            );
        }
    }

    /// 前台只轮询确认；最新快照失败时保留 dirty，在后台重试，且让工作空间看见错误。
    pub(super) fn poll_brain_proposal_save(&mut self, cx: &mut Context<Self>) {
        use std::sync::mpsc::TryRecvError;
        let result = self
            .brain
            .proposals_save
            .as_ref()
            .and_then(|saved| match saved.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("保存线程已退出".into())),
            });
        if let Some(result) = result {
            self.brain.proposals_save = None;
            match result {
                Ok(()) => self.brain.proposals_dirty = false,
                Err(error) => {
                    self.brain.proposals_retry_at = std::time::Instant::now() + super::START_RETRY;
                    let message =
                        format!("保存幕僚提议失败：{error}；正在重试，已派发的任务请勿重复派发");
                    self.desktop_workspace.error = Some(message.clone().into());
                    if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
                        cx.defer(move |cx| {
                            host.update(cx, |app, cx| {
                                app.desktop_workspace.error = Some(message.into());
                                cx.notify();
                            })
                        });
                    }
                    cx.notify();
                }
            }
        }
        if self.brain.proposals_dirty
            && self.brain.proposals_save.is_none()
            && std::time::Instant::now() >= self.brain.proposals_retry_at
        {
            self.save_brain_proposals();
        }
    }

    fn proposal_roster(&self) -> Vec<RosterLite> {
        self.agent_roster_for_provider()
            .into_iter()
            .map(|entry| RosterLite {
                name: entry.name,
                enabled: entry.enabled,
                has_card: entry.active_node_id.is_some(),
                busy: entry.busy,
            })
            .collect()
    }

    /// 目标 Agent 还没有卡片：派发会新建一张。
    pub(super) fn proposal_needs_card(&self, agent: &str) -> bool {
        self.proposal_roster()
            .iter()
            .any(|entry| entry.name == agent && !entry.has_card)
    }

    /// 展示和点击时使用同一份实时检查；等待用户确认不能当作可派发的空闲。
    pub(super) fn proposal_dispatch_block(&self, agent: &str) -> Option<ProposalBlock> {
        let roster = self.agent_roster_for_provider();
        let Some(target) = roster.iter().find(|entry| entry.name == agent) else {
            return classify_dispatch_block(false, false, false, false);
        };
        let waiting = target.active_node_id.is_some_and(|node| {
            matches!(
                self.runtime.agent_task_state(node),
                Some(
                    runtime::AgentTaskState::WaitingApproval { .. }
                        | runtime::AgentTaskState::WaitingQuestion
                )
            )
        });
        classify_dispatch_block(true, target.enabled, waiting, target.busy)
    }

    /// 大脑交来一条提议：校验目标后放进“需要你处理”，并提醒用户一句。
    pub(super) fn propose_next_step(
        &mut self,
        draft: Draft,
        cx: &mut Context<Self>,
    ) -> Result<u64, String> {
        norma::protocol::validate_proposal_copy(&draft.title, &draft.why)
            .map_err(super::decision_copy::format_error)?;
        check_target(&self.proposal_roster(), &draft.agent)?;
        self.load_brain_proposals(cx);
        if !self.brain.proposals_ready {
            return Err("提议尚未加载完成，请稍后重试".into());
        }
        let title = draft.title.clone();
        let id = self.brain.proposals.add(draft, SystemTime::now())?;
        self.save_brain_proposals();
        self.note_workspace_activity_changed(cx);
        let text = ui::i18n::format("chrome.brain_proposal_spoken", &[("title", title)]);
        self.speak_brain_report(text, ReportPriority::Normal, cx);
        Ok(id)
    }

    /// 把当前内存里的提议投影成岛卡片。阻断在会话上算好，后台投影不再碰 App。
    pub(crate) fn proposal_snapshot(&self, area_id: &str) -> Vec<crate::island::IslandProposal> {
        self.brain
            .proposals
            .items()
            .iter()
            .map(|proposal| crate::island::IslandProposal {
                id: proposal.id,
                area_id: area_id.to_owned(),
                agent: proposal.agent.clone(),
                title: proposal.title.clone(),
                task: proposal.task.clone(),
                why: proposal.why.clone(),
                block: self
                    .proposal_dispatch_block(&proposal.agent)
                    .map(island_block),
                creates_card: self.proposal_needs_card(&proposal.agent),
            })
            .collect()
    }

    /// 用户在岛上点“派发”。阻断或找不到时不删除提议，也不打开共享上下文。
    pub(crate) fn dispatch_brain_proposal(
        &mut self,
        id: u64,
    ) -> Result<(), crate::island::ProposalDispatchError> {
        self.proposal_ready_to_dispatch(id)?;
        let service = self
            .shared_context_service()
            .map_err(|error| crate::island::ProposalDispatchError::Failed(error.to_string()))?;
        service
            .execute_with_state(
                agent::context::ContextOperation::DispatchTask,
                None,
                |state| Ok(self.dispatch_brain_proposal_checked(id, state)),
            )
            .map_err(|error| crate::island::ProposalDispatchError::Failed(error.to_string()))?
    }

    /// 锁内再查一次：点下去之后目标可能变成正忙或正在等确认。
    fn dispatch_brain_proposal_checked(
        &mut self,
        id: u64,
        state: &agent::context::WorkingState,
    ) -> Result<(), crate::island::ProposalDispatchError> {
        let proposal = self.proposal_ready_to_dispatch(id)?;
        self.dispatch_ready_proposal(proposal, state)
    }

    fn proposal_ready_to_dispatch(
        &self,
        id: u64,
    ) -> Result<Proposal, crate::island::ProposalDispatchError> {
        let Some(proposal) = self
            .brain
            .proposals
            .items()
            .iter()
            .find(|proposal| proposal.id == id)
            .cloned()
        else {
            return Err(crate::island::ProposalDispatchError::Missing);
        };
        if let Some(block) = self.proposal_dispatch_block(&proposal.agent) {
            return Err(crate::island::ProposalDispatchError::Blocked {
                block: island_block(block),
                agent: proposal.agent,
            });
        }
        Ok(proposal)
    }

    fn dispatch_ready_proposal(
        &mut self,
        proposal: Proposal,
        state: &agent::context::WorkingState,
    ) -> Result<(), crate::island::ProposalDispatchError> {
        let id = proposal.id;
        self.assign_task(
            proposal.agent,
            proposal.task,
            None,
            super::AutoKind::UserDispatched,
            state,
        )
        .map_err(|error| crate::island::ProposalDispatchError::Failed(error.to_string()))?;
        if self.brain.proposals.take(id).is_none() {
            return Err(crate::island::ProposalDispatchError::Missing);
        }
        self.save_brain_proposals();
        Ok(())
    }

    /// 用户点“忽略”。取走成功才记一条拒绝，并写回提议文件。
    pub(crate) fn decline_brain_proposal(&mut self, id: u64) -> bool {
        let Some(proposal) = self.brain.proposals.take(id) else {
            return false;
        };
        self.save_brain_proposals();
        self.push_brain(brain::BrainEventKind::ProposalDeclined {
            agent: proposal.agent,
            title: proposal.title,
        });
        true
    }
}

/// 派发阻断的优先级：不在名册、已停用、正在等确认、正忙。空闲则可以派发。
pub(super) fn classify_dispatch_block(
    found: bool,
    enabled: bool,
    waiting_confirmation: bool,
    busy: bool,
) -> Option<ProposalBlock> {
    if !found {
        return Some(ProposalBlock::Missing);
    }
    if !enabled {
        return Some(ProposalBlock::Disabled);
    }
    if waiting_confirmation {
        return Some(ProposalBlock::WaitingConfirmation);
    }
    busy.then_some(ProposalBlock::Busy)
}

fn island_block(block: ProposalBlock) -> crate::island::IslandProposalBlock {
    match block {
        ProposalBlock::WaitingConfirmation => {
            crate::island::IslandProposalBlock::WaitingConfirmation
        }
        ProposalBlock::Busy => crate::island::IslandProposalBlock::Busy,
        ProposalBlock::Missing => crate::island::IslandProposalBlock::Missing,
        ProposalBlock::Disabled => crate::island::IslandProposalBlock::Disabled,
    }
}
