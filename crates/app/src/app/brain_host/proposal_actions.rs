//! 大脑提议的落地：接收、保存、用户点“派发”或“忽略”。

use super::proposals::{check_target, file_for, Draft, Proposals, RosterLite};
use super::{config, AutoKind};
use crate::app::MultisoulApp;
use gpui::Context;
use norma::protocol::ReportPriority;
use std::time::SystemTime;

impl MultisoulApp {
    /// 第一次需要时读本项目的提议文件；之后只在内存里改，并在后台写回。
    pub(super) fn load_brain_proposals(&mut self) {
        if self.brain.proposals_path.is_some() {
            return;
        }
        let (Some(root), Some(dir)) = (self.runtime.workspace_root(), config::state_dir()) else {
            return;
        };
        let path = file_for(&dir, root);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        self.brain.proposals = Proposals::from_json(&text);
        self.brain.proposals_path = Some(path);
    }

    fn save_brain_proposals(&self) {
        if let Some(path) = &self.brain.proposals_path {
            self.brain
                .proposal_writer
                .save(path.clone(), self.brain.proposals.to_json());
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
        self.load_brain_proposals();
        let title = draft.title.clone();
        let id = self.brain.proposals.add(draft, SystemTime::now())?;
        self.save_brain_proposals();
        let text = ui::i18n::format("chrome.brain_proposal_spoken", &[("title", title)]);
        self.speak_brain_report(text, ReportPriority::Normal, cx);
        Ok(id)
    }

    /// 用户点“派发”：按提议原文派给目标 Agent。Agent 正忙或派发失败时提议保留，返回原因。
    pub(crate) fn dispatch_brain_proposal(&mut self, id: u64) -> Result<(), String> {
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
        self.assign_task(
            proposal.agent,
            proposal.task,
            None,
            AutoKind::UserDispatched,
        )
        .map_err(|error| error.to_string())?;
        self.brain.proposals.take(id);
        self.save_brain_proposals();
        Ok(())
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
