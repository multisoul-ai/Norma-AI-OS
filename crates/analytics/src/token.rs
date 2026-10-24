//! 受限自由文本：只允许短小的 ASCII 标识，杜绝命令、路径等内容混入事件属性。

use serde::{Deserialize, Serialize};

const MAX_LEN: usize = 48;

/// 经过清洗的短标识（设置项名、引导步骤名、版本号）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token(String);

impl Token {
    /// 标识符：只保留 `[a-z0-9_-]`，大写转小写，其余字符丢弃，最长 48。
    pub fn slug(raw: &str) -> Self {
        Self::filtered(raw, |c| c.is_ascii_alphanumeric() || c == '_' || c == '-').lowercase()
    }

    /// 版本号：额外保留 `.` 与 `+`，最长 48。
    pub fn version(raw: &str) -> Self {
        Self::filtered(raw, |c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_')
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn filtered(raw: &str, keep: impl Fn(char) -> bool) -> Self {
        Self(raw.chars().filter(|c| keep(*c)).take(MAX_LEN).collect())
    }

    fn lowercase(self) -> Self {
        Self(self.0.to_ascii_lowercase())
    }
}
