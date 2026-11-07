//! 大脑提议的落地：接收、保存、用户点“派发”或“忽略”。

use super::proposals::{check_target, Draft, ProposalBlock, Proposals, RosterLite};
use crate::app::MultisoulApp;
use gpui::Context;
use norma::protocol::ReportPriority;
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
            return Some(ProposalBlock::Unavailable(format!("未找到 Agent：{agent}")));
        };
        if !target.enabled {
            return Some(ProposalBlock::Unavailable(format!("Agent {agent} 已停用")));
        }
        if target.active_node_id.is_some_and(|node| {
            matches!(
                self.runtime.agent_task_state(node),
                Some(
                    runtime::AgentTaskState::WaitingApproval { .. }
                        | runtime::AgentTaskState::WaitingQuestion
                )
            )
        }) {
            return Some(ProposalBlock::WaitingConfirmation);
        }
        target.busy.then_some(ProposalBlock::Busy)
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
        let text = ui::i18n::format("chrome.brain_proposal_spoken", &[("title", title)]);
        self.speak_brain_report(text, ReportPriority::Normal, cx);
        Ok(id)
    }
}
