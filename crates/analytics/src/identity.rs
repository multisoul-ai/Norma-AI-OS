//! 匿名 ID 与登录后哈希 ID。原始用户标识不会离开本机。

use std::fs;
use std::path::PathBuf;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

pub struct Identity {
    anon_path: PathBuf,
    user_path: PathBuf,
    anonymous_id: String,
    user_id: Option<String>,
}

impl Identity {
    /// 读取或创建 `analytics_anon_id`；存在 `analytics_user_id` 时恢复已识别身份。
    pub fn load(dir: &std::path::Path) -> Self {
        let anon_path = dir.join("analytics_anon_id");
        let user_path = dir.join("analytics_user_id");
        let anonymous_id = read_trimmed(&anon_path).unwrap_or_else(|| {
            let id = Uuid::new_v4().to_string();
            write(&anon_path, &id);
            id
        });
        let user_id = read_trimmed(&user_path);
        Self {
            anon_path,
            user_path,
            anonymous_id,
            user_id,
        }
    }

    pub fn anonymous_id(&self) -> &str {
        &self.anonymous_id
    }

    /// PostHog `distinct_id`：已识别用户优先，否则匿名 ID。
    pub fn distinct_id(&self) -> &str {
        self.user_id.as_deref().unwrap_or(&self.anonymous_id)
    }

    /// 以 `HMAC-SHA256(salt, raw)` 生成稳定 ID 并持久化；返回 `Some(新 ID)` 表示身份发生变化。
    pub fn identify(&mut self, raw: &str, salt: &str) -> Option<String> {
        let hashed = hash_user_id(raw, salt);
        if self.user_id.as_deref() == Some(hashed.as_str()) {
            return None;
        }
        write(&self.user_path, &hashed);
        self.user_id = Some(hashed.clone());
        Some(hashed)
    }

    /// 退出登录：丢弃已识别身份并换一个新的匿名 ID，避免同机多用户串联。
    pub fn reset(&mut self) {
        let _ = fs::remove_file(&self.user_path);
        self.user_id = None;
        self.anonymous_id = Uuid::new_v4().to_string();
        write(&self.anon_path, &self.anonymous_id);
    }
}

pub fn hash_user_id(raw: &str, salt: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(salt.as_bytes())
        .unwrap_or_else(|_| unreachable!("HMAC 接受任意长度密钥"));
    mac.update(raw.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn read_trimmed(path: &std::path::Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn write(path: &std::path::Path, value: &str) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, value);
}
