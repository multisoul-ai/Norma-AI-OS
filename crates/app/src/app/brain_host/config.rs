//! 大脑的本地配置：`~/.norma/brain/config.json`，以及后端和适配器进程的运行环境。

use brain::{Backend, BackendKind, Strategy};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 定时摘要的间隔。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SummaryEvery {
    FiveMinutes,
    TenMinutes,
    Off,
}

impl SummaryEvery {
    pub(crate) fn duration(self) -> Option<Duration> {
        match self {
            Self::FiveMinutes => Some(Duration::from_secs(300)),
            Self::TenMinutes => Some(Duration::from_secs(600)),
            Self::Off => None,
        }
    }
}

/// 判断较慢时是否提醒用户。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SlowReminder {
    OverviewOnly,
    VoiceAfter60s,
    Off,
}

/// 大脑可以自行决定的事；没勾选的一律交给用户。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Autonomy {
    /// 批准只读、测试、构建类的命令。
    pub readonly_commands: bool,
    /// 批准在项目目录内写文件。
    pub project_writes: bool,
    /// 给空闲的 Agent 派后续任务。
    pub assign_followups: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BrainSettings {
    pub enabled: bool,
    pub backend: Option<BackendKind>,
    pub strategy: Strategy,
    /// Claude Code 的模型别名（opus、sonnet、haiku）；`None` 按策略。
    pub main_model: Option<String>,
    pub approval_model: Option<String>,
    pub summary: SummaryEvery,
    pub autonomy: Autonomy,
    pub slow_reminder: SlowReminder,
    /// 多个审批同时到达时并行判断。
    pub parallel_approvals: bool,
}

impl Default for BrainSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: None,
            strategy: Strategy::Auto,
            main_model: None,
            approval_model: None,
            summary: SummaryEvery::FiveMinutes,
            autonomy: Autonomy {
                readonly_commands: true,
                project_writes: true,
                assign_followups: true,
            },
            slow_reminder: SlowReminder::OverviewOnly,
            parallel_approvals: true,
        }
    }
}

/// 默认 `~/.norma/brain`；`NORMA_BRAIN_DIR` 可整体改位置，用于隔离测试。
pub(super) fn state_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("NORMA_BRAIN_DIR").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::home_dir().map(|home| home.join(".norma").join("brain"))
}

/// 读取配置；文件不存在或损坏时大脑保持关闭。
pub(super) fn load_settings() -> BrainSettings {
    let Some(path) = state_dir().map(|dir| dir.join("config.json")) else {
        return BrainSettings::default();
    };
    parse_settings(&std::fs::read_to_string(path).unwrap_or_default())
}

fn model_alias(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|alias| ["opus", "sonnet", "haiku"].contains(alias))
        .map(str::to_string)
}

/// 空文本或无效 JSON 都视为关闭；缺失的字段取默认值。
pub(super) fn parse_settings(text: &str) -> BrainSettings {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return BrainSettings::default();
    };
    let default = BrainSettings::default();
    let flag = |path: &[&str], fallback: bool| {
        path.iter()
            .try_fold(&value, |v, key| v.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(fallback)
    };
    BrainSettings {
        enabled: flag(&["enabled"], false),
        backend: match value["backend"].as_str() {
            Some("claude") => Some(BackendKind::ClaudeCode),
            Some("codex") => Some(BackendKind::Codex),
            Some("opencode") => Some(BackendKind::OpenCode),
            _ => None,
        },
        strategy: match value["strategy"].as_str() {
            Some("speed") => Strategy::Speed,
            Some("quality") => Strategy::Quality,
            _ => Strategy::Auto,
        },
        main_model: model_alias(&value["main_model"]),
        approval_model: model_alias(&value["approval_model"]),
        summary: match value["summary"].as_str() {
            Some("10m") => SummaryEvery::TenMinutes,
            Some("off") => SummaryEvery::Off,
            _ => default.summary,
        },
        autonomy: Autonomy {
            readonly_commands: flag(&["autonomy", "readonly_commands"], true),
            project_writes: flag(&["autonomy", "project_writes"], true),
            assign_followups: flag(&["autonomy", "assign_followups"], true),
        },
        slow_reminder: match value["slow_reminder"].as_str() {
            Some("voice60") => SlowReminder::VoiceAfter60s,
            Some("off") => SlowReminder::Off,
            _ => default.slow_reminder,
        },
        parallel_approvals: flag(&["parallel_approvals"], true),
    }
}

pub(super) fn save_settings(settings: &BrainSettings) -> std::io::Result<()> {
    let dir = state_dir().ok_or_else(|| std::io::Error::other("找不到用户目录"))?;
    save_settings_to(&dir, settings)
}

pub(super) fn save_settings_to(dir: &Path, settings: &BrainSettings) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let value = json!({
        "enabled": settings.enabled,
        "backend": settings.backend.map(|kind| match kind {
            BackendKind::ClaudeCode => "claude",
            BackendKind::Codex => "codex",
            BackendKind::OpenCode => "opencode",
        }),
        "strategy": match settings.strategy {
            Strategy::Speed => "speed",
            Strategy::Auto => "auto",
            Strategy::Quality => "quality",
        },
        "main_model": settings.main_model,
        "approval_model": settings.approval_model,
        "summary": match settings.summary {
            SummaryEvery::FiveMinutes => "5m",
            SummaryEvery::TenMinutes => "10m",
            SummaryEvery::Off => "off",
        },
        "autonomy": {
            "readonly_commands": settings.autonomy.readonly_commands,
            "project_writes": settings.autonomy.project_writes,
            "assign_followups": settings.autonomy.assign_followups,
        },
        "slow_reminder": match settings.slow_reminder {
            SlowReminder::OverviewOnly => "overview",
            SlowReminder::VoiceAfter60s => "voice60",
            SlowReminder::Off => "off",
        },
        "parallel_approvals": settings.parallel_approvals,
    });
    std::fs::write(dir.join("config.json"), serde_json::to_vec_pretty(&value)?)
}

/// GUI 启动的进程 PATH 很短，补上 Homebrew 和 nvm 的 node 目录，适配器是 node 脚本。
pub(super) fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    if let Some(home) = dirs::home_dir() {
        let versions = home.join(".nvm").join("versions").join("node");
        let mut found: Vec<PathBuf> = std::fs::read_dir(versions)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join("bin"))
            .collect();
        found.sort();
        found.reverse();
        dirs.extend(found);
        dirs.push(home.join(".local").join("bin"));
    }
    dirs
}

/// 按配置挑后端；没有指定时按 Claude Code、Codex、OpenCode 的顺序取第一个。
pub(super) fn pick_backend(settings: &BrainSettings) -> Option<Backend> {
    let dir = state_dir()?.join("adapters");
    let found = brain::backend::detect(&dir, &search_dirs());
    match settings.backend {
        Some(kind) => found.into_iter().find(|backend| backend.kind == kind),
        None => found.into_iter().next(),
    }
}

pub(super) fn adapter_env(
    token: &str,
    workspace_id: &str,
    route_id: &str,
) -> Vec<(String, String)> {
    let path = std::env::join_paths(search_dirs())
        .map(|joined| joined.to_string_lossy().into_owned())
        .unwrap_or_default();
    vec![
        ("PATH".into(), path),
        (
            "NORMA_SOCKET".into(),
            norma::client::default_socket_path().display().to_string(),
        ),
        ("NORMA_BRAIN_TOKEN".into(), token.into()),
        ("NORMA_WORKSPACE_ID".into(), workspace_id.into()),
        ("NORMA_WORKSPACE_ROUTE_ID".into(), route_id.into()),
    ]
}
