//! 大脑的本地配置：`~/.norma/brain/config.json`，以及后端和适配器进程的运行环境。

use brain::{Backend, BackendKind, Strategy};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
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

/// 一个后端的主会话与审批会话模型；`None` 交给后端自己的默认值或策略。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ModelPair {
    pub main: Option<String>,
    pub approval: Option<String>,
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
    /// 大脑的汇报是否出声；关闭后大脑照常工作，只是不说话。
    pub speak: bool,
    /// 用户点过“以后这类先问我”的操作类别，见 `log::ask_key`。
    pub always_ask: Vec<String>,
    /// Codex、OpenCode 的模型，按后端分别保存；Claude Code 用 `main_model` 与 `approval_model`。
    pub backend_models: HashMap<BackendKind, ModelPair>,
}

impl BrainSettings {
    /// 所选后端要用的模型。
    pub(crate) fn models_for(&self, kind: BackendKind) -> ModelPair {
        match kind {
            BackendKind::ClaudeCode => ModelPair {
                main: self.main_model.clone(),
                approval: self.approval_model.clone(),
            },
            other => self.backend_models.get(&other).cloned().unwrap_or_default(),
        }
    }
}

fn kind_key(kind: BackendKind) -> &'static str {
    match kind {
        BackendKind::ClaudeCode => "claude",
        BackendKind::Codex => "codex",
        BackendKind::OpenCode => "opencode",
    }
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
                // 默认只提议下一步，用户在概览里确认后才派发。
                assign_followups: false,
            },
            slow_reminder: SlowReminder::OverviewOnly,
            parallel_approvals: true,
            speak: true,
            always_ask: Vec::new(),
            backend_models: HashMap::new(),
        }
    }
}

/// 静默池窗口：普通成功攒这么久再交给大脑扫一遍。默认 30 秒；
/// `NORMA_BRAIN_QUIET_SECS` 可改，供真实 App 验收缩短等待，取值 1 到 600。
pub(super) fn quiet_window() -> std::time::Duration {
    parse_quiet_secs(std::env::var("NORMA_BRAIN_QUIET_SECS").ok().as_deref())
}

pub(super) fn parse_quiet_secs(value: Option<&str>) -> std::time::Duration {
    let secs = value
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|secs| (1..=600).contains(secs))
        .unwrap_or(30);
    std::time::Duration::from_secs(secs)
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
            assign_followups: flag(&["autonomy", "assign_followups"], false),
        },
        slow_reminder: match value["slow_reminder"].as_str() {
            Some("voice60") => SlowReminder::VoiceAfter60s,
            Some("off") => SlowReminder::Off,
            _ => default.slow_reminder,
        },
        parallel_approvals: flag(&["parallel_approvals"], true),
        speak: flag(&["speak"], true),
        always_ask: value["always_ask"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        backend_models: [BackendKind::Codex, BackendKind::OpenCode]
            .into_iter()
            .filter_map(|kind| {
                let entry = &value["backend_models"][kind_key(kind)];
                let text = |key: &str| entry[key].as_str().map(str::to_string);
                entry.is_object().then(|| {
                    (
                        kind,
                        ModelPair {
                            main: text("main"),
                            approval: text("approval"),
                        },
                    )
                })
            })
            .collect(),
    }
}

pub(super) fn save_settings(
    settings: &BrainSettings,
) -> std::io::Result<Receiver<Result<(), String>>> {
    let dir = state_dir().ok_or_else(|| std::io::Error::other("找不到用户目录"))?;
    Ok(brain::persistence::shared_writer().save(dir.join("config.json"), settings_json(settings)?))
}

#[cfg(test)]
pub(super) fn save_settings_to(dir: &Path, settings: &BrainSettings) -> std::io::Result<()> {
    brain::persistence::atomic_write(
        &dir.join("config.json"),
        settings_json(settings)?.as_bytes(),
    )
}

fn settings_json(settings: &BrainSettings) -> std::io::Result<String> {
    let value = json!({
        "enabled": settings.enabled,
        "backend": settings.backend.map(kind_key),
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
        "speak": settings.speak,
        "always_ask": settings.always_ask,
        "backend_models": settings
            .backend_models
            .iter()
            .map(|(kind, pair)| {
                (
                    kind_key(*kind).to_string(),
                    json!({"main": pair.main, "approval": pair.approval}),
                )
            })
            .collect::<serde_json::Map<_, _>>(),
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

/// 桌面画布拥有稳定 workspace 身份；同一目录的两个画布不能共享任务或目标。
pub(super) fn session_dir(dir: &Path, workspace_id: &str) -> PathBuf {
    let digest = Sha256::digest(workspace_id.as_bytes());
    let name: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    dir.join("projects").join(name)
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
    let found = brain::backend::detect(&dir, &search_dirs())
        .into_iter()
        .filter(|backend| brain::backend::unavailable_reason(backend.kind).is_none());
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

#[cfg(test)]
#[path = "../../../tests/unit/brain_config_models_tests.rs"]
mod brain_config_models_tests;
