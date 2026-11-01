//! 大脑提议的下一步：放在“需要你处理”里，用户点“派发”后才派给 Agent。
//! 写入 `<大脑目录>/projects/<工作空间哈希>/proposals.json`，重启后仍在。

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub(crate) const MAX_PROPOSALS: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProposalBlock {
    WaitingConfirmation,
    Busy,
    Unavailable(String),
}

impl ProposalBlock {
    pub(crate) fn label(&self) -> &'static str {
        ui::i18n::text(match self {
            Self::WaitingConfirmation => "chrome.brain_proposal_waiting_label",
            Self::Busy => "chrome.brain_proposal_busy_label",
            Self::Unavailable(_) => "chrome.brain_proposal_unavailable_label",
        })
    }

    pub(crate) fn message(&self, agent: &str) -> String {
        match self {
            Self::Unavailable(message) => message.clone(),
            _ => ui::i18n::format(
                if matches!(self, Self::WaitingConfirmation) {
                    "chrome.brain_proposal_waiting"
                } else {
                    "chrome.brain_proposal_busy"
                },
                &[("agent", agent.to_owned())],
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProposalFeedback {
    Dispatching,
    Failed(String),
}

/// 大脑交来的提议内容。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Draft {
    pub agent: String,
    pub title: String,
    pub task: String,
    pub why: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Proposal {
    pub id: u64,
    /// 派给谁：Agent 名称；还没有卡片时派发会新建。
    pub agent: String,
    pub title: String,
    /// 派发时原样发给 Agent 的完整指令。
    pub task: String,
    pub why: String,
    pub at: SystemTime,
}

#[derive(Debug, Default)]
pub(crate) struct Proposals {
    next_id: u64,
    items: Vec<Proposal>,
}

impl Proposals {
    /// 同一个 Agent 只留最新一条；已满时拒绝。返回新提议的编号。
    pub(crate) fn add(&mut self, draft: Draft, at: SystemTime) -> Result<u64, String> {
        norma::protocol::validate_proposal_copy(&draft.title, &draft.why)
            .map_err(super::decision_copy::format_error)?;
        self.items.retain(|p| p.agent != draft.agent);
        if self.items.len() >= MAX_PROPOSALS {
            return Err(format!(
                "too_many：已有 {MAX_PROPOSALS} 条提议等用户确认，先不要再提"
            ));
        }
        self.next_id += 1;
        self.items.push(Proposal {
            id: self.next_id,
            agent: draft.agent,
            title: draft.title.trim().to_owned(),
            task: draft.task,
            why: draft.why.trim().to_owned(),
            at,
        });
        Ok(self.next_id)
    }

    pub(crate) fn take(&mut self, id: u64) -> Option<Proposal> {
        let index = self.items.iter().position(|p| p.id == id)?;
        Some(self.items.remove(index))
    }

    pub(crate) fn items(&self) -> &[Proposal] {
        &self.items
    }

    pub(crate) fn to_json(&self) -> String {
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|p| {
                json!({
                    "id": p.id,
                    "agent": p.agent,
                    "title": p.title,
                    "task": p.task,
                    "why": p.why,
                    "at": p.at.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
                })
            })
            .collect();
        json!({ "next_id": self.next_id, "items": items }).to_string()
    }

    /// 损坏内容必须报错，让磁盘拥有者保留原文件。
    pub(crate) fn from_json(text: &str) -> Result<Self, String> {
        #[derive(serde::Deserialize)]
        struct Saved {
            next_id: u64,
            items: Vec<SavedItem>,
        }
        #[derive(serde::Deserialize)]
        struct SavedItem {
            id: u64,
            agent: String,
            title: String,
            task: String,
            why: String,
            at: u64,
        }
        let saved: Saved =
            serde_json::from_str(text).map_err(|e| format!("提议文件格式错误：{e}"))?;
        let items: Vec<Proposal> = saved
            .items
            .into_iter()
            .map(|item| {
                let at = SystemTime::UNIX_EPOCH
                    .checked_add(Duration::from_secs(item.at))
                    .ok_or_else(|| "提议时间超出系统范围".to_string())?;
                Ok(Proposal {
                    id: item.id,
                    agent: item.agent,
                    title: item.title,
                    task: item.task,
                    why: item.why,
                    at,
                })
            })
            .collect::<Result<_, String>>()?;
        let highest = items.iter().map(|p| p.id).max().unwrap_or(0);
        if saved.next_id.max(highest) == u64::MAX {
            return Err("提议编号超出范围".into());
        }
        Ok(Self {
            next_id: saved.next_id.max(highest),
            items,
        })
    }
}

/// 校验提议用到的名册信息。
#[derive(Clone, Debug)]
pub(crate) struct RosterLite {
    pub name: String,
    pub enabled: bool,
    pub has_card: bool,
    pub busy: bool,
}

/// 目标必须存在且启用；有空闲卡片时不能提议给还没有卡片的 Agent，避免卡片越建越多。
pub(crate) fn check_target(roster: &[RosterLite], agent: &str) -> Result<(), String> {
    let Some(target) = roster.iter().find(|entry| entry.name == agent) else {
        return Err(format!(
            "unknown_agent：没有叫 {agent} 的 Agent，先用 agents 命令查看"
        ));
    };
    if !target.enabled {
        return Err(format!("disabled：{agent} 已停用，换一个 Agent"));
    }
    if !target.has_card {
        if let Some(idle) = roster
            .iter()
            .find(|entry| entry.enabled && entry.has_card && !entry.busy)
        {
            return Err(format!(
                "idle_card_exists：{} 有卡片且空闲，请提议给它，不要新建卡片",
                idle.name
            ));
        }
    }
    Ok(())
}

/// 旧版按项目根目录哈希命名的文件，仅用于一次性迁移。
pub(crate) fn file_for(state_dir: &Path, root: &Path) -> PathBuf {
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    let name: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    state_dir.join("proposals").join(format!("{name}.json"))
}

pub(crate) use brain::persistence::Writer;

#[cfg(test)]
#[path = "../../../tests/unit/brain_proposals_tests.rs"]
mod brain_proposals_tests;
