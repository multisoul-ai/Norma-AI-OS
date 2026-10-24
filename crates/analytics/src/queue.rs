//! 待发送事件与磁盘持久化（JSONL，条数封顶）。

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 队列最多保留的事件数；超出时淘汰最旧的。
pub const MAX_QUEUED: usize = 1000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Queued {
    /// PostHog 用它对重试去重。
    pub uuid: String,
    pub event: String,
    pub distinct_id: String,
    pub timestamp_ms: u64,
    pub properties: Map<String, Value>,
}

pub struct DiskQueue {
    path: PathBuf,
}

impl DiskQueue {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 读取上次遗留的事件；损坏的行直接跳过。
    pub fn load(&self) -> VecDeque<Queued> {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return VecDeque::new();
        };
        let mut items: VecDeque<Queued> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        while items.len() > MAX_QUEUED {
            items.pop_front();
        }
        items
    }

    /// 覆盖写入；空队列时删除文件。
    pub fn save(&self, items: &VecDeque<Queued>) {
        if items.is_empty() {
            self.clear();
            return;
        }
        let body: String = items
            .iter()
            .filter_map(|item| serde_json::to_string(item).ok())
            .map(|line| line + "\n")
            .collect();
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&self.path, body);
    }

    pub fn clear(&self) {
        let _ = fs::remove_file(&self.path);
    }
}
