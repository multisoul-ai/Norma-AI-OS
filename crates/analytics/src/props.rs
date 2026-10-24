//! 事件属性枚举：取值全部是编译期固定的字面量。

macro_rules! str_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }
    };
}

str_enum!(
    /// 通用 `feature_used` 的功能名。
    Feature {
        Terminal => "terminal",
        Agent => "agent",
        Voice => "voice",
        NormaAssistant => "norma_assistant",
        Browser => "browser",
        Preview => "preview",
        Ideas => "ideas",
        Wallpaper => "wallpaper",
        MobileConnection => "mobile_connection",
        Settings => "settings",
        CommandDock => "command_dock",
        Search => "search",
    }
);

str_enum!(
    /// 创建入口。
    Source { Shortcut => "shortcut", Menu => "menu", Dock => "dock", Other => "other" }
);

str_enum!(
    /// Agent 种类。
    AgentKind {
        Claude => "claude",
        Codex => "codex",
        Cursor => "cursor",
        OpenCode => "opencode",
        Grok => "grok",
        Kimi => "kimi",
        Pi => "pi",
        Hermes => "hermes",
        ZCode => "zcode",
        InfCode => "infcode",
        Other => "other",
    }
);

str_enum!(
    /// Agent 任务入口。
    EntryPoint { Card => "card", Norma => "norma", Mobile => "mobile", Other => "other" }
);

str_enum!(
    /// 语音识别供应商类型。
    ProviderKind { Local => "local", Cloud => "cloud" }
);

str_enum!(
    /// Norma 助手入口。
    AssistantEntry { Text => "text", Voice => "voice" }
);

str_enum!(
    /// 壁纸类型。
    WallpaperKind { Static => "static", Video => "video" }
);

str_enum!(
    /// 耗时分桶，避免上报精确时长。
    DurationBucket {
        Under10s => "lt_10s",
        Under1m => "lt_1m",
        Under5m => "lt_5m",
        Under30m => "lt_30m",
        Over30m => "ge_30m",
    }
);

impl DurationBucket {
    pub fn from_secs(secs: u64) -> Self {
        match secs {
            0..=9 => Self::Under10s,
            10..=59 => Self::Under1m,
            60..=299 => Self::Under5m,
            300..=1799 => Self::Under30m,
            _ => Self::Over30m,
        }
    }
}
