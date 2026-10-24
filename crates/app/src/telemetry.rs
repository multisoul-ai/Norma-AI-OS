//! 产品分析的 App 侧适配：初始化、窗口前后台、Agent 任务生命周期。
//!
//! 纯逻辑与网络都在 `analytics` crate；这里只做映射与 GPUI 挂接。所有调用都是
//! 内存操作 + `mpsc::send`，可安全出现在 frame loop 中。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use analytics::{AgentKind, Config, DurationBucket, EntryPoint, Event, Source, Token};
use canvas::AgentProfile;
use gpui::{App, Window};
use norma::agent_hook::AgentHookEvent;

static TASK_STARTS: Mutex<Option<HashMap<u64, Instant>>> = Mutex::new(None);

/// 在 GPUI 启动闭包内调用：后台线程里初始化，随后挂接窗口前后台观察。
pub(crate) fn install(cx: &mut App) {
    std::thread::spawn(|| {
        let Some(home) = dirs::home_dir() else { return };
        let config = Config::from_build_env(home.join(".norma").join("analytics"), os_version());
        let app_version = config.app_version.clone();
        let os = config.os_version.clone();
        let handle = analytics::init(config);
        handle.track(Event::AppSessionStarted {
            app_version: Token::version(&app_version),
            os_version: Token::version(&os),
        });
        handle.on_foreground();
    });
    cx.observe_keystrokes(|_, _, _| analytics::touch()).detach();
    cx.observe_new(
        |_: &mut crate::app::MultisoulApp, window: Option<&mut Window>, cx| {
            if let Some(window) = window {
                cx.observe_window_activation(window, |_, window, _| {
                    if let Some(analytics) = analytics::global() {
                        if window.is_window_active() {
                            analytics.on_foreground();
                        } else {
                            analytics.on_background();
                        }
                    }
                })
                .detach();
            }
        },
    )
    .detach();
    cx.on_app_quit(|_| async {
        if let Some(analytics) = analytics::global() {
            analytics.on_background();
            analytics.flush_blocking(std::time::Duration::from_millis(800));
        }
    })
    .detach();
}

fn os_version() -> String {
    std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|text| text.trim().to_owned())
        .unwrap_or_default()
}

pub(crate) fn agent_kind(profile: AgentProfile) -> AgentKind {
    match profile {
        AgentProfile::Claude => AgentKind::Claude,
        AgentProfile::Codex => AgentKind::Codex,
        AgentProfile::Cursor => AgentKind::Cursor,
        AgentProfile::Grok => AgentKind::Grok,
        AgentProfile::OpenCode => AgentKind::OpenCode,
        AgentProfile::InfCode => AgentKind::InfCode,
        AgentProfile::Kimi => AgentKind::Kimi,
        AgentProfile::ZCode => AgentKind::ZCode,
        AgentProfile::Pi => AgentKind::Pi,
        AgentProfile::Hermes => AgentKind::Hermes,
    }
}

fn protocol_agent_kind(profile: norma::protocol::AgentProfile) -> AgentKind {
    use norma::protocol::AgentProfile as P;
    match profile {
        P::Claude => AgentKind::Claude,
        P::Codex => AgentKind::Codex,
        P::Cursor => AgentKind::Cursor,
        P::Grok => AgentKind::Grok,
        P::OpenCode => AgentKind::OpenCode,
        P::InfCode => AgentKind::InfCode,
        P::Kimi => AgentKind::Kimi,
        P::ZCode => AgentKind::ZCode,
        P::Pi => AgentKind::Pi,
        P::Hermes => AgentKind::Hermes,
    }
}

pub(crate) fn terminal_created() {
    analytics::track(Event::TerminalNodeCreated {
        source: Source::Other,
    });
}

pub(crate) fn agent_card_created(profile: AgentProfile) {
    analytics::track(Event::AgentCardCreated {
        agent: agent_kind(profile),
    });
}

/// 把 Agent hook 事件映射为任务开始 / 完成；耗时分桶，不带任何内容。
pub(crate) fn agent_hook(event: &AgentHookEvent) {
    let agent = protocol_agent_kind(event.profile);
    match event.provider_event.as_str() {
        "UserPromptSubmit" => {
            if let Ok(mut starts) = TASK_STARTS.lock() {
                starts
                    .get_or_insert_with(HashMap::new)
                    .insert(event.node_id, Instant::now());
            }
            analytics::track(Event::AgentTaskStarted {
                agent,
                entry: EntryPoint::Card,
            });
        }
        name @ ("Stop" | "turn/completed" | "StopFailure") => {
            let started = TASK_STARTS
                .lock()
                .ok()
                .and_then(|mut starts| starts.as_mut()?.remove(&event.node_id));
            // 没见过开始事件（例如 App 中途启动）时不上报，避免伪造耗时。
            if let Some(started) = started {
                analytics::track(Event::AgentTaskCompleted {
                    agent,
                    success: name != "StopFailure",
                    duration: DurationBucket::from_secs(started.elapsed().as_secs()),
                });
            }
        }
        _ => {}
    }
}

pub(crate) fn feature_used(feature: analytics::Feature) {
    analytics::track(Event::FeatureUsed {
        feature,
        success: true,
    });
}

pub(crate) fn preview_opened() {
    analytics::track(Event::PreviewCardOpened);
}

pub(crate) fn browser_created() {
    analytics::track(Event::BrowserCardCreated);
}

pub(crate) fn idea_created() {
    analytics::track(Event::IdeaCreated);
}

/// 授权结果；成功时用授权 Key 的盐哈希识别用户，原始 Key 不会被上报。
pub(crate) fn license_activated(success: bool, identity_seed: Option<&str>) {
    analytics::track(Event::LicenseActivated { success });
    if let (Some(seed), Some(analytics)) = (identity_seed, analytics::global()) {
        analytics.identify(seed);
    }
}
