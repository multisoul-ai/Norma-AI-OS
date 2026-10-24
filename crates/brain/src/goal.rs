//! 目标摘要与决策日志，落盘后大脑崩溃或会话压缩时可据此重建。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// 最多保留几条目标。
const MAX_GOALS: usize = 5;
/// 太短的话（“好的”“继续”）不是目标。
const MIN_CHARS: usize = 6;
/// 摘要里每条目标最多显示的字数。
const SUMMARY_CHARS: usize = 200;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Goal {
    pub agent: String,
    pub text: String,
}

/// 用户发给各 Agent 的任务，作为大脑判断“下一步做什么”的依据；同时保存决策日志。
pub struct GoalStore {
    dir: PathBuf,
    goals: Mutex<Vec<Goal>>,
    /// 用户没采纳的提议，只放内存。
    declined: Mutex<VecDeque<String>>,
}

impl GoalStore {
    pub fn open(dir: PathBuf) -> Self {
        let goals = fs::read_to_string(dir.join("goals.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            dir,
            goals: Mutex::new(goals),
            declined: Mutex::new(VecDeque::new()),
        }
    }

    pub fn goals(&self) -> Vec<Goal> {
        self.goals.lock().expect("goal lock").clone()
    }

    /// 记录一条目标。太短、斜杠命令、同一 Agent 连续重复的不记，返回 `false`。
    pub fn record(&self, agent: &str, text: &str) -> bool {
        let text = text.trim();
        if text.chars().count() < MIN_CHARS || text.starts_with('/') {
            return false;
        }
        let mut goals = self.goals.lock().expect("goal lock");
        if goals
            .iter()
            .rev()
            .find(|goal| goal.agent == agent)
            .is_some_and(|goal| goal.text == text)
        {
            return false;
        }
        goals.push(Goal {
            agent: agent.to_string(),
            text: text.to_string(),
        });
        if goals.len() > MAX_GOALS {
            let extra = goals.len() - MAX_GOALS;
            goals.drain(..extra);
        }
        if fs::create_dir_all(&self.dir).is_ok() {
            if let Ok(json) = serde_json::to_string(&*goals) {
                if let Err(error) =
                    crate::persistence::atomic_write(&self.dir.join("goals.json"), json.as_bytes())
                {
                    tracing::warn!(%error, "保存幕僚目标失败");
                }
            }
        }
        true
    }

    /// 每条一行：`- Agent：目标`。换行压平，过长截断；没有目标时是 `None`。
    pub fn summary(&self) -> Option<String> {
        let goals = self.goals();
        if goals.is_empty() {
            return None;
        }
        let lines: Vec<String> = goals
            .iter()
            .map(|goal| {
                let flat = goal.text.split_whitespace().collect::<Vec<_>>().join(" ");
                let shown: String = flat.chars().take(SUMMARY_CHARS).collect();
                let more = if flat.chars().count() > SUMMARY_CHARS {
                    "…"
                } else {
                    ""
                };
                format!("- {}：{shown}{more}", goal.agent)
            })
            .collect();
        Some(lines.join("\n"))
    }

    /// 记一条用户没采纳的提议，只留最近 5 条。
    pub fn decline(&self, agent: &str, title: &str) {
        let flat = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut declined = self.declined.lock().expect("declined lock");
        declined.push_back(format!("- {agent}：{flat}"));
        while declined.len() > MAX_GOALS {
            declined.pop_front();
        }
    }

    pub fn declined_summary(&self) -> Option<String> {
        let declined = self.declined.lock().expect("declined lock");
        (!declined.is_empty()).then(|| declined.iter().cloned().collect::<Vec<_>>().join("\n"))
    }

    /// 追加一条决策记录（一行 JSON）。写失败只记日志，不影响大脑。
    pub fn append_decision(&self, line: &str) {
        let result = fs::create_dir_all(&self.dir).and_then(|_| {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.dir.join("decisions.jsonl"))?;
            writeln!(file, "{}", line.replace('\n', " "))
        });
        if let Err(error) = result {
            tracing::warn!(%error, "写入决策日志失败");
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/goal_tests.rs"]
mod goal_tests;
