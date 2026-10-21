//! “每个会话只注入一次”的标记文件（仅 Kimi 用）：放在临时目录，任何 IO 失败都按未注入处理。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const MARKER_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn marker_dir() -> PathBuf {
    std::env::temp_dir().join("norma-path-convention")
}

/// 文件名 = 清洗后的前缀 + 完整 session_id 的哈希，避免清洗/截断造成不同会话碰撞。
fn marker_name(session_id: &str) -> Option<String> {
    let prefix: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(24)
        .collect();
    if session_id.is_empty() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    session_id.hash(&mut hasher);
    Some(format!("{prefix}-{:016x}", hasher.finish()))
}

fn ensure_private_dir(dir: &PathBuf) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

/// 首次针对某会话调用返回 `true`；`create_new` 不跟随符号链接，已存在即视为已注入。
pub(super) fn claim(session_id: &str) -> bool {
    let Some(name) = marker_name(session_id) else {
        return false;
    };
    let dir = marker_dir();
    if ensure_private_dir(&dir).is_err() {
        return false;
    }
    let claimed = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(name))
        .is_ok();
    if claimed {
        purge_stale(&dir);
    }
    claimed
}

/// 输出失败（如 Agent 已关闭管道）时撤销标记，让下一条提示词还有机会注入。
pub(super) fn release(session_id: &str) {
    if let Some(name) = marker_name(session_id) {
        let _ = std::fs::remove_file(marker_dir().join(name));
    }
}

fn purge_stale(dir: &PathBuf) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > MARKER_TTL);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 标记文件名：不同 session_id 即使清洗后相同也不得碰撞；空 id 不产生标记。
    ///
    /// 数据构造："a.b" 与 "ab" 清洗后前缀都是 "ab"，但完整 id 不同。
    /// 预期结果：两者文件名不同（哈希区分）；同一 id 文件名稳定；空串为 None。
    #[test]
    fn test_marker_name_distinguishes_sessions_that_sanitize_alike() {
        assert_ne!(
            marker_name("a.b"),
            marker_name("ab"),
            "清洗后相同的 id 不得共用标记"
        );
        assert_eq!(
            marker_name("s-1"),
            marker_name("s-1"),
            "同一 id 的文件名应稳定"
        );
        assert!(marker_name("").is_none(), "空 id 不应产生标记");
    }

    /// 首次 claim 为 true、重复为 false；release 后又可 claim（输出失败时的补注入机会）。
    ///
    /// 数据构造：带进程号的唯一 session_id，避免与其他运行冲突。
    /// 预期结果：true → false → release → true；最后清理标记。
    #[test]
    fn test_claim_is_once_per_session_and_release_reopens_it() {
        let id = format!("claim-test-{}", std::process::id());
        release(&id);
        assert!(claim(&id), "首次应占用成功");
        assert!(!claim(&id), "重复占用必须失败，保证只注入一次");
        release(&id);
        assert!(claim(&id), "release 后应可再次占用");
        release(&id);
    }
}
