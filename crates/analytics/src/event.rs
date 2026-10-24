//! 强类型事件：属性只能来自枚举、布尔、数值与受限 Token。

use serde_json::{json, Map, Value};

use crate::props::*;
use crate::token::Token;

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    AppSessionStarted {
        app_version: Token,
        os_version: Token,
    },
    AppForegrounded,
    AppBackgrounded {
        foreground_ms: u64,
        active_ms: u64,
    },
    AppUpdated {
        from: Token,
        to: Token,
    },
    LicenseActivated {
        success: bool,
    },
    FeatureUsed {
        feature: Feature,
        success: bool,
    },
    TerminalNodeCreated {
        source: Source,
    },
    AgentCardCreated {
        agent: AgentKind,
    },
    AgentTaskStarted {
        agent: AgentKind,
        entry: EntryPoint,
    },
    AgentTaskCompleted {
        agent: AgentKind,
        success: bool,
        duration: DurationBucket,
    },
    VoiceInputStarted {
        provider: ProviderKind,
    },
    NormaAssistantUsed {
        entry: AssistantEntry,
    },
    BrowserCardCreated,
    PreviewCardOpened,
    MobileConnectionEnabled,
    MobileDevicePaired,
    IdeaCreated,
    WallpaperChanged {
        kind: WallpaperKind,
    },
    OnboardingStepCompleted {
        step: Token,
    },
    SettingChanged {
        setting: Token,
    },
    AnalyticsOptedOut,
}

impl Event {
    pub fn name(&self) -> &'static str {
        match self {
            Self::AppSessionStarted { .. } => "app_session_started",
            Self::AppForegrounded => "app_foregrounded",
            Self::AppBackgrounded { .. } => "app_backgrounded",
            Self::AppUpdated { .. } => "app_updated",
            Self::LicenseActivated { .. } => "license_activated",
            Self::FeatureUsed { .. } => "feature_used",
            Self::TerminalNodeCreated { .. } => "terminal_node_created",
            Self::AgentCardCreated { .. } => "agent_card_created",
            Self::AgentTaskStarted { .. } => "agent_task_started",
            Self::AgentTaskCompleted { .. } => "agent_task_completed",
            Self::VoiceInputStarted { .. } => "voice_input_started",
            Self::NormaAssistantUsed { .. } => "norma_assistant_used",
            Self::BrowserCardCreated => "browser_card_created",
            Self::PreviewCardOpened => "preview_card_opened",
            Self::MobileConnectionEnabled => "mobile_connection_enabled",
            Self::MobileDevicePaired => "mobile_device_paired",
            Self::IdeaCreated => "idea_created",
            Self::WallpaperChanged { .. } => "wallpaper_changed",
            Self::OnboardingStepCompleted { .. } => "onboarding_step_completed",
            Self::SettingChanged { .. } => "setting_changed",
            Self::AnalyticsOptedOut => "analytics_opted_out",
        }
    }

    pub fn properties(&self) -> Map<String, Value> {
        let value = match self {
            Self::AppSessionStarted {
                app_version,
                os_version,
            } => {
                json!({ "app_version": app_version.as_str(), "os_version": os_version.as_str() })
            }
            Self::AppBackgrounded {
                foreground_ms,
                active_ms,
            } => {
                json!({ "foreground_ms": foreground_ms, "active_ms": active_ms })
            }
            Self::AppUpdated { from, to } => {
                json!({ "from_version": from.as_str(), "to_version": to.as_str() })
            }
            Self::LicenseActivated { success } => json!({ "success": success }),
            Self::FeatureUsed { feature, success } => {
                json!({ "feature": feature.as_str(), "success": success })
            }
            Self::TerminalNodeCreated { source } => json!({ "source": source.as_str() }),
            Self::AgentCardCreated { agent } => json!({ "agent": agent.as_str() }),
            Self::AgentTaskStarted { agent, entry } => {
                json!({ "agent": agent.as_str(), "entry_point": entry.as_str() })
            }
            Self::AgentTaskCompleted {
                agent,
                success,
                duration,
            } => json!({
                "agent": agent.as_str(),
                "success": success,
                "duration_bucket": duration.as_str(),
            }),
            Self::VoiceInputStarted { provider } => json!({ "provider_kind": provider.as_str() }),
            Self::NormaAssistantUsed { entry } => json!({ "entry_point": entry.as_str() }),
            Self::WallpaperChanged { kind } => json!({ "kind": kind.as_str() }),
            Self::OnboardingStepCompleted { step } => json!({ "step": step.as_str() }),
            Self::SettingChanged { setting } => json!({ "setting": setting.as_str() }),
            Self::AppForegrounded
            | Self::BrowserCardCreated
            | Self::PreviewCardOpened
            | Self::MobileConnectionEnabled
            | Self::MobileDevicePaired
            | Self::IdeaCreated
            | Self::AnalyticsOptedOut => json!({}),
        };
        match value {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    }
}
