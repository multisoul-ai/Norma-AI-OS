use crate::agent_hook::{HookInstallPaths, HookSource, HookTarget};
use crate::protocol::AgentProfile;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrchestrationProvider {
    Claude,
    Codex,
    Cursor,
    OpenCode,
    Grok,
    Kimi,
    ZCode,
    Pi,
    Hermes,
}

impl OrchestrationProvider {
    pub const ALL: [Self; 9] = [
        Self::Codex,
        Self::Claude,
        Self::Cursor,
        Self::OpenCode,
        Self::Grok,
        Self::Kimi,
        Self::ZCode,
        Self::Pi,
        Self::Hermes,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::OpenCode => "opencode",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
            Self::ZCode => "zcode",
            Self::Pi => "pi",
            Self::Hermes => "hermes",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor Agent CLI",
            Self::OpenCode => "OpenCode",
            Self::Grok => "Grok Build",
            Self::Kimi => "Kimi Code",
            Self::ZCode => "ZCode",
            Self::Pi => "Pi",
            Self::Hermes => "Hermes Agent",
        }
    }

    pub const fn profile(self) -> AgentProfile {
        match self {
            Self::Claude => AgentProfile::Claude,
            Self::Codex => AgentProfile::Codex,
            Self::Cursor => AgentProfile::Cursor,
            Self::OpenCode => AgentProfile::OpenCode,
            Self::Grok => AgentProfile::Grok,
            Self::Kimi => AgentProfile::Kimi,
            Self::ZCode => AgentProfile::ZCode,
            Self::Pi => AgentProfile::Pi,
            Self::Hermes => AgentProfile::Hermes,
        }
    }

    pub(crate) const fn hook_source(self) -> Option<HookSource> {
        match self {
            Self::Claude => Some(HookSource::Claude),
            Self::Codex => Some(HookSource::Codex),
            Self::Cursor => Some(HookSource::Cursor),
            Self::OpenCode => Some(HookSource::OpenCode),
            Self::Pi => Some(HookSource::Pi),
            Self::Kimi => Some(HookSource::Kimi),
            Self::ZCode => Some(HookSource::ZCode),
            Self::Hermes => Some(HookSource::Hermes),
            Self::Grok => None,
        }
    }

    pub(crate) const fn hook_target(self) -> Option<HookTarget> {
        match self {
            Self::Claude => Some(HookTarget::Claude),
            Self::Codex => Some(HookTarget::Codex),
            Self::Cursor => Some(HookTarget::Cursor),
            Self::OpenCode => Some(HookTarget::OpenCode),
            Self::Pi => Some(HookTarget::Pi),
            Self::Kimi => Some(HookTarget::Kimi),
            Self::ZCode => Some(HookTarget::ZCode),
            Self::Hermes => Some(HookTarget::Hermes),
            Self::Grok => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentState {
    Missing,
    Installing,
    Ready {
        version: String,
    },
    Outdated {
        installed_version: String,
        required_version: String,
    },
    Conflict {
        path: PathBuf,
        reason: String,
    },
    Failed {
        error_code: String,
        message: String,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AggregateStatus {
    NotInstalled,
    Installing,
    Ready,
    NeedsRepair,
    InstallFailed,
    Uninstalling,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderIntegration {
    pub provider: OrchestrationProvider,
    pub aggregate_status: AggregateStatus,
    pub skill_component: ComponentState,
    pub cli_component: ComponentState,
    pub hook_component: ComponentState,
    pub authorization_component: ComponentState,
    pub installed_version: Option<String>,
    pub last_error: Option<String>,
}

impl ProviderIntegration {
    pub fn from_components(
        provider: OrchestrationProvider,
        skill_component: ComponentState,
        cli_component: ComponentState,
        hook_component: ComponentState,
        authorization_component: ComponentState,
    ) -> Self {
        let components = [
            &skill_component,
            &cli_component,
            &hook_component,
            &authorization_component,
        ];
        let aggregate_status = aggregate_status(&components);
        let installed_version = components.iter().find_map(|component| match component {
            ComponentState::Ready { version } => Some(version.clone()),
            _ => None,
        });
        let last_error = components.iter().find_map(|component| match component {
            ComponentState::Conflict { reason, .. } => Some(reason.clone()),
            ComponentState::Failed { message, .. } => Some(message.clone()),
            ComponentState::Unsupported { reason } => Some(reason.clone()),
            _ => None,
        });
        Self {
            provider,
            aggregate_status,
            skill_component,
            cli_component,
            hook_component,
            authorization_component,
            installed_version,
            last_error,
        }
    }

    pub fn unsupported(provider: OrchestrationProvider) -> Self {
        let reason = "Orchestration is not available for Grok Build in this release.".to_string();
        let component = ComponentState::Unsupported {
            reason: reason.clone(),
        };
        Self::from_components(
            provider,
            component.clone(),
            component.clone(),
            component.clone(),
            component,
        )
    }
}

/// 各 CLI 官方支持的配置根覆盖环境变量取值（空字符串视同未设置）。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CliHomeOverrides {
    pub kimi_code_home: Option<std::ffi::OsString>,
    pub pi_coding_agent_dir: Option<std::ffi::OsString>,
    pub hermes_home: Option<std::ffi::OsString>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallPaths {
    pub managed_root: PathBuf,
    pub bundled_cli: PathBuf,
    pub bundled_hook: PathBuf,
    pub hook_paths: HookInstallPaths,
    pub claude_skill: PathBuf,
    pub codex_skill: PathBuf,
    pub cursor_rule: PathBuf,
    pub opencode_skill: PathBuf,
}

impl InstallPaths {
    pub fn for_home_and_bundle_bin(home: &std::path::Path, bundle_bin: &std::path::Path) -> Self {
        Self::for_home_bundle_bin_and_env(
            home,
            bundle_bin,
            &CliHomeOverrides {
                kimi_code_home: std::env::var_os("KIMI_CODE_HOME"),
                pi_coding_agent_dir: std::env::var_os("PI_CODING_AGENT_DIR"),
                hermes_home: std::env::var_os("HERMES_HOME"),
            },
        )
    }

    /// 与 [`Self::for_home_and_bundle_bin`] 相同，但各 CLI 的目录覆盖变量由调用方显式给出，便于隔离测试。
    pub fn for_home_bundle_bin_and_env(
        home: &std::path::Path,
        bundle_bin: &std::path::Path,
        overrides: &CliHomeOverrides,
    ) -> Self {
        let non_empty = |value: &Option<std::ffi::OsString>| {
            value
                .clone()
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        let pi_agent_dir = non_empty(&overrides.pi_coding_agent_dir)
            .unwrap_or_else(|| home.join(".pi").join("agent"));
        let hermes_home = non_empty(&overrides.hermes_home).unwrap_or_else(|| home.join(".hermes"));
        Self {
            managed_root: home.join("Library/Application Support/Norma OS"),
            bundled_cli: bundle_bin.join("norma"),
            bundled_hook: bundle_bin.join("multisoul-agent-hook"),
            hook_paths: HookInstallPaths {
                claude_settings: home.join(".claude/settings.json"),
                codex_hooks: home.join(".codex/hooks.json"),
                cursor_hooks: home.join(".cursor/hooks.json"),
                grok_hooks: home.join(".grok/hooks.json"),
                opencode_plugin: home.join(".config/opencode/plugins/multisoul-agent-hook.js"),
                pi_extension: pi_agent_dir
                    .join("extensions")
                    .join(crate::agent_hook::PI_EXTENSION_FILE_NAME),
                kimi_config: crate::agent_hook::kimi_config_path_for(
                    home,
                    overrides.kimi_code_home.clone(),
                ),
                hermes_plugin_dir: hermes_home
                    .join("plugins")
                    .join(crate::agent_hook::HERMES_PLUGIN_NAME),
                zcode_config: home.join(".zcode/cli/config.json"),
            },
            claude_skill: home.join(".claude/skills/norma-orchestration/SKILL.md"),
            codex_skill: home.join(".codex/skills/norma-orchestration/SKILL.md"),
            cursor_rule: home.join(".cursor/rules/norma-orchestration.mdc"),
            opencode_skill: home.join(".config/opencode/skills/norma-orchestration/SKILL.md"),
        }
    }

    pub fn for_current_process() -> Result<Self, std::io::Error> {
        let home = dirs::home_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "user home directory not found",
            )
        })?;
        let executable = std::env::current_exe()?;
        let bundle_bin = executable.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "current executable has no parent directory",
            )
        })?;
        Ok(Self::for_home_and_bundle_bin(&home, bundle_bin))
    }

    pub fn managed_cli_path(&self) -> PathBuf {
        self.managed_root.join("bin").join("norma")
    }

    pub fn managed_hook_path(&self) -> PathBuf {
        self.managed_root.join("bin").join("multisoul-agent-hook")
    }

    pub fn credential_path(&self, provider: OrchestrationProvider) -> PathBuf {
        self.managed_root
            .join("orchestration")
            .join(format!("{}.json", provider.slug()))
    }

    /// 四个新 CLI 的 `skills/` 根目录，由各自已验证的 hook 路径推导，保证技能与 hook 位于同一配置根。
    ///
    /// - Kimi：`config.toml` 的父目录（`$KIMI_CODE_HOME` 或 `~/.kimi-code`）。
    /// - ZCode：`~/.zcode/cli/config.json` 上两级，即 `~/.zcode`。
    /// - Pi：`<agent_dir>/extensions/<file>` 上两级，即 `$PI_CODING_AGENT_DIR` 或 `~/.pi/agent`。
    /// - Hermes：`<HERMES_HOME>/plugins/<name>` 上两级，即 `$HERMES_HOME` 或 `~/.hermes`。
    fn derived_skills_root(&self, provider: OrchestrationProvider) -> Option<PathBuf> {
        let base = match provider {
            OrchestrationProvider::Kimi => self.hook_paths.kimi_config.parent()?,
            OrchestrationProvider::ZCode => self.hook_paths.zcode_config.parent()?.parent()?,
            OrchestrationProvider::Pi => self.hook_paths.pi_extension.parent()?.parent()?,
            OrchestrationProvider::Hermes => {
                self.hook_paths.hermes_plugin_dir.parent()?.parent()?
            }
            _ => return None,
        };
        Some(base.join("skills"))
    }

    pub fn skill_path(&self, provider: OrchestrationProvider) -> Option<PathBuf> {
        match provider {
            OrchestrationProvider::Claude => Some(self.claude_skill.clone()),
            OrchestrationProvider::Codex => Some(self.codex_skill.clone()),
            OrchestrationProvider::Cursor => Some(self.cursor_rule.clone()),
            OrchestrationProvider::OpenCode => Some(self.opencode_skill.clone()),
            OrchestrationProvider::Grok => None,
            OrchestrationProvider::Kimi
            | OrchestrationProvider::ZCode
            | OrchestrationProvider::Pi
            | OrchestrationProvider::Hermes => Some(
                self.derived_skills_root(provider)?
                    .join("norma-orchestration")
                    .join("SKILL.md"),
            ),
        }
    }

    fn sibling_skill_path(&self, provider: OrchestrationProvider, name: &str) -> Option<PathBuf> {
        let skills_root = match provider {
            OrchestrationProvider::Claude => self.claude_skill.parent()?.parent()?.to_path_buf(),
            OrchestrationProvider::Codex => self.codex_skill.parent()?.parent()?.to_path_buf(),
            OrchestrationProvider::OpenCode => {
                self.opencode_skill.parent()?.parent()?.to_path_buf()
            }
            OrchestrationProvider::Kimi
            | OrchestrationProvider::ZCode
            | OrchestrationProvider::Pi
            | OrchestrationProvider::Hermes => self.derived_skills_root(provider)?,
            OrchestrationProvider::Cursor | OrchestrationProvider::Grok => return None,
        };
        Some(skills_root.join(name).join("SKILL.md"))
    }

    pub fn presentation_skill_path(&self, provider: OrchestrationProvider) -> Option<PathBuf> {
        match provider {
            OrchestrationProvider::Cursor => self
                .cursor_rule
                .parent()
                .map(|dir| dir.join("norma-presentation.mdc")),
            _ => self.sibling_skill_path(provider, "norma-presentation"),
        }
    }

    /// Path of the managed `/ask-norma` entry-point skill for a Provider.
    ///
    /// ask-norma is the friendly, higher-recall entry into the Norma OS Agent delegation flow
    /// documented by `norma-orchestration`. It is derived from the orchestration skill path so it
    /// stays beside it without adding fields to `InstallPaths`.
    pub fn ask_norma_skill_path(&self, provider: OrchestrationProvider) -> Option<PathBuf> {
        match provider {
            OrchestrationProvider::Cursor => self
                .cursor_rule
                .parent()
                .map(|dir| dir.join("ask-norma.mdc")),
            _ => self.sibling_skill_path(provider, "ask-norma"),
        }
    }
}

fn aggregate_status(components: &[&ComponentState]) -> AggregateStatus {
    if components
        .iter()
        .all(|component| matches!(component, ComponentState::Unsupported { .. }))
    {
        return AggregateStatus::Unsupported;
    }
    if components
        .iter()
        .all(|component| matches!(component, ComponentState::Ready { .. }))
    {
        return AggregateStatus::Ready;
    }
    if components
        .iter()
        .all(|component| matches!(component, ComponentState::Missing))
    {
        return AggregateStatus::NotInstalled;
    }
    if components
        .iter()
        .any(|component| matches!(component, ComponentState::Installing))
    {
        return AggregateStatus::Installing;
    }
    if components
        .iter()
        .any(|component| matches!(component, ComponentState::Failed { .. }))
    {
        return AggregateStatus::InstallFailed;
    }
    AggregateStatus::NeedsRepair
}
