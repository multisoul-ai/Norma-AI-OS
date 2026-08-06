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
}

impl OrchestrationProvider {
    pub const ALL: [Self; 5] = [
        Self::Codex,
        Self::Claude,
        Self::Cursor,
        Self::OpenCode,
        Self::Grok,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::OpenCode => "opencode",
            Self::Grok => "grok",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor Agent CLI",
            Self::OpenCode => "OpenCode",
            Self::Grok => "Grok Build",
        }
    }

    pub const fn profile(self) -> AgentProfile {
        match self {
            Self::Claude => AgentProfile::Claude,
            Self::Codex => AgentProfile::Codex,
            Self::Cursor => AgentProfile::Cursor,
            Self::OpenCode => AgentProfile::OpenCode,
            Self::Grok => AgentProfile::Grok,
        }
    }

    pub(crate) const fn hook_source(self) -> Option<HookSource> {
        match self {
            Self::Claude => Some(HookSource::Claude),
            Self::Codex => Some(HookSource::Codex),
            Self::Cursor => Some(HookSource::Cursor),
            Self::OpenCode => Some(HookSource::OpenCode),
            Self::Grok => None,
        }
    }

    pub(crate) const fn hook_target(self) -> Option<HookTarget> {
        match self {
            Self::Claude => Some(HookTarget::Claude),
            Self::Codex => Some(HookTarget::Codex),
            Self::Cursor => Some(HookTarget::Cursor),
            Self::OpenCode => Some(HookTarget::OpenCode),
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

    pub(crate) fn skill_path(&self, provider: OrchestrationProvider) -> Option<&PathBuf> {
        match provider {
            OrchestrationProvider::Claude => Some(&self.claude_skill),
            OrchestrationProvider::Codex => Some(&self.codex_skill),
            OrchestrationProvider::Cursor => Some(&self.cursor_rule),
            OrchestrationProvider::OpenCode => Some(&self.opencode_skill),
            OrchestrationProvider::Grok => None,
        }
    }

    pub fn presentation_skill_path(&self, provider: OrchestrationProvider) -> Option<PathBuf> {
        match provider {
            OrchestrationProvider::Claude => self
                .claude_skill
                .parent()?
                .parent()?
                .join("norma-presentation")
                .join("SKILL.md")
                .into(),
            OrchestrationProvider::Codex => self
                .codex_skill
                .parent()?
                .parent()?
                .join("norma-presentation")
                .join("SKILL.md")
                .into(),
            OrchestrationProvider::Cursor => self
                .cursor_rule
                .parent()?
                .join("norma-presentation.mdc")
                .into(),
            OrchestrationProvider::OpenCode => self
                .opencode_skill
                .parent()?
                .parent()?
                .join("norma-presentation")
                .join("SKILL.md")
                .into(),
            OrchestrationProvider::Grok => None,
        }
    }

    /// Path of the managed `/ask-norma` entry-point skill for a Provider.
    ///
    /// ask-norma is the friendly, higher-recall entry into the Norma OS Agent delegation flow
    /// documented by `norma-orchestration`. It is derived from the orchestration skill path so it
    /// stays beside it without adding fields to `InstallPaths`.
    pub fn ask_norma_skill_path(&self, provider: OrchestrationProvider) -> Option<PathBuf> {
        match provider {
            OrchestrationProvider::Claude => self
                .claude_skill
                .parent()?
                .parent()?
                .join("ask-norma")
                .join("SKILL.md")
                .into(),
            OrchestrationProvider::Codex => self
                .codex_skill
                .parent()?
                .parent()?
                .join("ask-norma")
                .join("SKILL.md")
                .into(),
            OrchestrationProvider::Cursor => {
                self.cursor_rule.parent()?.join("ask-norma.mdc").into()
            }
            OrchestrationProvider::OpenCode => self
                .opencode_skill
                .parent()?
                .parent()?
                .join("ask-norma")
                .join("SKILL.md")
                .into(),
            OrchestrationProvider::Grok => None,
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
