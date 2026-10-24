//! 登录提供方注册表。新增提供方只需在此增加枚举值与映射，状态机不变。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    GitHub,
    Google,
}

impl Provider {
    pub const ALL: [Self; 2] = [Self::GitHub, Self::Google];

    /// 线上协议与路由使用的稳定小写标识。
    pub const fn id(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Google => "google",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// 品牌写法的显示名；界面文案另走 i18n，此处供日志与协议错误使用。
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::Google => "Google",
        }
    }
}
