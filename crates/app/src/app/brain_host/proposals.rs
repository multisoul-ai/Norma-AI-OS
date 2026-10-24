//! 大脑提议的下一步：放在“需要你处理”里，用户点“派发”后才派给 Agent。
//! 写入 `<大脑目录>/proposals/<项目>.json`，重启后仍在。

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

pub(crate) const MAX_PROPOSALS: usize = 5;

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
            title: draft.title,
            task: draft.task,
            why: draft.why,
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

    /// 损坏或缺字段的内容按没有提议处理。
    pub(crate) fn from_json(text: &str) -> Self {
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let items: Vec<Proposal> = value["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                Some(Proposal {
                    id: item["id"].as_u64()?,
                    agent: item["agent"].as_str()?.to_string(),
                    title: item["title"].as_str()?.to_string(),
                    task: item["task"].as_str()?.to_string(),
                    why: item["why"].as_str()?.to_string(),
                    at: SystemTime::UNIX_EPOCH + Duration::from_secs(item["at"].as_u64()?),
                })
            })
            .collect();
        let highest = items.iter().map(|p| p.id).max().unwrap_or(0);
        Self {
            next_id: value["next_id"].as_u64().unwrap_or(0).max(highest),
            items,
        }
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

/// 每个项目一个文件，用项目根目录的哈希命名。
pub(crate) fn file_for(state_dir: &Path, root: &Path) -> PathBuf {
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    let name: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    state_dir.join("proposals").join(format!("{name}.json"))
}

/// 后台写文件。写入可能乱序完成，只让最新的一次落盘。
#[derive(Clone, Debug, Default)]
pub(crate) struct Writer {
    issued: Arc<Mutex<u64>>,
    written: Arc<Mutex<u64>>,
}

impl Writer {
    pub(crate) fn save(&self, path: PathBuf, text: String) {
        let version = {
            let mut issued = self.issued.lock().unwrap_or_else(|e| e.into_inner());
            *issued += 1;
            *issued
        };
        let written = self.written.clone();
        std::thread::spawn(move || {
            let mut done = written.lock().unwrap_or_else(|e| e.into_inner());
            if *done > version {
                return;
            }
            let result = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|_| std::fs::write(&path, text));
            match result {
                Ok(()) => *done = version,
                Err(error) => tracing::warn!(%error, "写入大脑提议失败"),
            }
        });
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_proposals_tests.rs"]
mod brain_proposals_tests;
