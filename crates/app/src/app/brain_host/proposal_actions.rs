//! 大脑提议的落地：接收、保存、用户点“派发”或“忽略”。

use super::proposals::{check_target, file_for, Draft, Proposals, RosterLite};
use super::{config, AutoKind};
use crate::app::MultisoulApp;
use gpui::Context;
use norma::protocol::{BrainOp, CardCommandResult, ReportPriority};
use std::time::SystemTime;

impl MultisoulApp {
    /// 第一次需要时读本项目的提议文件；之后只在内存里改，并在后台写回。
    pub(super) fn load_brain_proposals(&mut self, cx: &mut Context<Self>) {
        if self.brain.proposals_ready
            || self.brain.proposals_loading
            || std::time::Instant::now() < self.brain.proposals_retry_at
        {
            return;
        }
        let (Some(root), Some(dir), Some(identity)) = (
            self.runtime.workspace_root(),
            config::state_dir(),
            self.workspace_id.clone(),
        ) else {
            return;
        };
        let legacy = file_for(&dir, root);
        let path = self
            .brain
            .proposals_path
            .clone()
            .unwrap_or_else(|| config::session_dir(&dir, &identity).join("proposals.json"));
        self.brain.proposals_path = Some(path.clone());
        self.brain.proposals_loading = true;
        self.brain.proposals_retry_at = std::time::Instant::now() + super::START_RETRY;
        let writer = self.brain.proposal_writer.clone();
        let work = cx.background_executor().spawn(async move {
            writer
                .flush()
                .recv()
                .map_err(std::io::Error::other)?
                .map_err(std::io::Error::other)?;
            // 旧版按根目录保存；原子移动只会被一个画布领取，避免复制后重复派发。
            if !path.exists() && legacy.exists() {
                std::fs::create_dir_all(path.parent().expect("状态目录应有父路径"))?;
                match std::fs::rename(&legacy, &path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => Proposals::from_json(&text).map_err(std::io::Error::other),
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
                    }
                    Err(error) => {
                        tracing::warn!(%error, "读取幕僚提议失败，保留原文件");
                        app.desktop_workspace.error =
                            Some(format!("读取幕僚提议失败：{error}").into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_brain_proposals(&mut self) {
        if let Some(path) = &self.brain.proposals_path {
            self.brain.proposals_dirty = true;
            self.brain.proposals_save = Some(
                self.brain
                    .proposal_writer
                    .save(path.clone(), self.brain.proposals.to_json()),
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

    pub(crate) fn brain_proposal_open(&self) -> Option<&(String, u64)> {
        self.brain.proposal_open.as_ref()
    }

    /// 展开或收起概览里的一条提议；同一时间只展开一条。
    pub(crate) fn toggle_brain_proposal(&mut self, key: (String, u64)) {
        self.brain.proposal_open = if self.brain.proposal_open.as_ref() == Some(&key) {
            None
        } else {
            Some(key)
        };
    }

    /// 目标 Agent 还没有卡片：派发会新建一张。
    pub(super) fn proposal_needs_card(&self, agent: &str) -> bool {
        self.proposal_roster()
            .iter()
            .any(|entry| entry.name == agent && !entry.has_card)
    }

    /// 大脑交来一条提议：校验目标后放进“需要你处理”，并提醒用户一句。
    pub(super) fn propose_next_step(
        &mut self,
        draft: Draft,
        cx: &mut Context<Self>,
    ) -> Result<u64, String> {
        check_target(&self.proposal_roster(), &draft.agent)?;
        self.load_brain_proposals(cx);
        if !self.brain.proposals_ready {
            return Err("提议尚未加载完成，请稍后重试".into());
        }
        let title = draft.title.clone();
        let id = self.brain.proposals.add(draft, SystemTime::now())?;
        self.save_brain_proposals();
        let text = ui::i18n::format("chrome.brain_proposal_spoken", &[("title", title)]);
        self.speak_brain_report(text, ReportPriority::Normal, cx);
        Ok(id)
    }

    /// 用户点“派发”：按提议原文派给目标 Agent。Agent 正忙或派发失败时提议保留，返回原因。
    pub(crate) fn dispatch_brain_proposal(
        &mut self,
        id: u64,
    ) -> Result<(BrainOp, CardCommandResult), String> {
        let service = self
            .shared_context_service()
            .map_err(|error| error.to_string())?;
        service
            .execute_with_state(
                agent::context::ContextOperation::DispatchTask,
                None,
                |state| Ok(self.dispatch_brain_proposal_checked(id, state)),
            )
            .map_err(|error| error.to_string())?
    }

    fn dispatch_brain_proposal_checked(
        &mut self,
        id: u64,
        state: &agent::context::WorkingState,
    ) -> Result<(BrainOp, CardCommandResult), String> {
        let Some(proposal) = self
            .brain
            .proposals
            .items()
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return Err(ui::i18n::text("chrome.brain_proposal_closed").to_string());
        };
        let busy = self
            .proposal_roster()
            .iter()
            .any(|entry| entry.name == proposal.agent && entry.has_card && entry.busy);
        if busy {
            return Err(ui::i18n::format(
                "chrome.brain_proposal_busy",
                &[("agent", proposal.agent)],
            ));
        }
        let op = BrainOp::Assign {
            agent: proposal.agent.clone(),
            task: proposal.task.clone(),
        };
        let result = self
            .assign_task(
                proposal.agent,
                proposal.task,
                None,
                AutoKind::UserDispatched,
                state,
            )
            .map_err(|error| error.to_string())?;
        self.brain.proposals.take(id);
        self.save_brain_proposals();
        Ok((op, result))
    }

    /// 用户点“忽略”：移除提议，并告诉大脑别再提同样的事。
    pub(crate) fn decline_brain_proposal(&mut self, id: u64) -> bool {
        let Some(proposal) = self.brain.proposals.take(id) else {
            return false;
        };
        self.save_brain_proposals();
        self.push_brain(brain::BrainEvent::ProposalDeclined {
            agent: proposal.agent,
            title: proposal.title,
        });
        true
    }
}
