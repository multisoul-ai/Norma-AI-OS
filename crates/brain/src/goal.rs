//! 目标摘要与决策日志，落盘后大脑崩溃或会话压缩时可据此重建。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct GoalStore {
    dir: PathBuf,
    cache: Mutex<Option<String>>,
}

impl GoalStore {
    pub fn open(dir: PathBuf) -> Self {
        let cached = fs::read_to_string(dir.join("goal.md"))
            .ok()
            .filter(|s| !s.trim().is_empty());
        Self {
            dir,
            cache: Mutex::new(cached),
        }
    }

    pub fn goal(&self) -> Option<String> {
        self.cache.lock().expect("goal lock").clone()
    }

    pub fn set_goal(&self, text: &str) {
        *self.cache.lock().expect("goal lock") = Some(text.to_string());
        if fs::create_dir_all(&self.dir).is_ok() {
            let _ = fs::write(self.dir.join("goal.md"), text);
        }
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
