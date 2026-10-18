//! `app` crate 的库目标：暴露 GPUI shell（`MultisoulApp` / `AppScreen` / actions）
//! 给 e2e 测试 crate。二进制目标 `multisoul`（`src/main.rs`）隐式依赖本 lib。
//!
//! 模块声明保持私有（`mod`，非 `pub mod`）：内部模块树无需对外暴露，仅通过
//! 下方 `pub use` 释放 e2e / bin 所需的公开 API，避免泄漏 `pub(crate)` 类型。
mod app;
mod assets;
mod browser_auth_host;
mod canvas;
mod cards;
mod database;
mod effects;
mod frame_diagnostics;
mod license_gate;
mod logging;
mod media;
mod menu_bar;
mod mobile;
mod motion;
mod network_crypto;
mod norma;
mod performance_monitor;
mod preview;
mod settings;
mod shortcuts;
mod updater;
mod workspace;

#[cfg(test)]
#[path = "../tests/unit/menu_bar_tests.rs"]
mod menu_bar_tests;

#[cfg(feature = "e2e-test-support")]
pub use app::{
    install_norma_endpoint_for_e2e, mark_orchestration_health_ready_for_e2e,
    publish_canonical_orchestration_health_for_e2e, submit_norma_request_for_e2e,
};
#[cfg(all(target_os = "macos", feature = "l3-test-support"))]
pub use app::{run_ego_live_capture_probe, EgoLiveCaptureProbeReport};
#[cfg(feature = "e2e-test-support")]
pub use app::{
    runtime_frame_interval_for_e2e, CloneProjectE2eSnapshot, EgoCardE2eSnapshot,
    FeatureGuideE2eSnapshot, OrchestrationProviderE2eSnapshot, OrchestrationSettingsE2eSnapshot,
    PresentationE2eSnapshot, ReplyTtsE2ePcmSnapshot, ReplyTtsE2eProbe, ReplyTtsE2eProbeSnapshot,
    ReplyTtsE2eSnapshot,
};
pub use app::{
    AgentDefinitionE2eSnapshot, AgentSettingsE2eSnapshot, ApiKeyDisplayState, AppScreen,
    CanvasLayoutE2eSnapshot, CheckForUpdates, CloneProject, CopyCardContent, ExitFocus,
    InitializationAnimationE2eSnapshot, MultisoulApp, NewBrowser, NewClaudeAgent, NewCodexAgent,
    NewCursorAgent, NewGrokAgent, NewHermesAgent, NewKimiAgent, NewOpenCodeAgent, NewPiAgent,
    NewTerminal, NewWindow, NewZCodeAgent, OpenFeatureGuide, OpenPreviewFile, OpenProject,
    QuitNormaOs, ReflowLayout, SettingsE2eSnapshot, ToggleMasterLayout, ToggleSettings,
    TtsProviderE2eSnapshot,
};
#[cfg(feature = "e2e-test-support")]
pub use app::{DispatchReceiptEffectE2eSnapshot, DispatchReceiptEffectItemE2eSnapshot};
pub use assets::EmbeddedAssets;
pub use browser_auth_host::run_browser_auth_native_host;
#[cfg(all(target_os = "macos", feature = "l3-test-support"))]
pub use mobile::push::configuration::l3::run_push_sqlite_probe;
pub use mobile::{
    CommandLease, LiveApprovalRequest, LiveCommandRequest, LiveConversationRequest,
    LiveReviewRequest, LiveSnapshotRequest, LiveTerminalRequest, LiveTerminalTask, MobileAccess,
    MobileCommand, MobileCommandRequest, MobileProvider, MobileServer, MobileSources,
    ProjectReviewRequest, PushRelayConfig, ThreadTerminalRequest,
};

#[cfg(feature = "e2e-test-support")]
pub use license_gate::{build_activation_root_e2e, build_configured_unlicensed_root_e2e};

use gpui::{App, Application, KeyBinding};

fn register_global_actions(cx: &mut App) {
    cx.on_action(|_: &QuitNormaOs, cx| {
        if workspace::sessions::can_quit(cx) {
            settings::general::quit_after_saving(cx);
        }
    });
    cx.on_action(|_: &OpenFeatureGuide, cx| open_feature_guide_in_active_window(cx));
    cx.on_action(|_: &CheckForUpdates, cx| updater::check_for_updates(cx));
}

fn open_feature_guide_in_active_window(cx: &mut App) {
    let Some(active_window) = cx.active_window().or_else(|| cx.windows().last().copied()) else {
        return;
    };
    let _ = active_window.update(cx, |root, window, cx| {
        let Ok(root) = root.downcast::<ui::component::Root>() else {
            return;
        };
        let Ok(app) = root.read(cx).view().clone().downcast::<MultisoulApp>() else {
            return;
        };
        app.update(cx, |app, cx| {
            app.on_open_feature_guide(&OpenFeatureGuide, window, cx)
        });
    });
}

pub fn run_norma_os() {
    app::cef_process::exit_if_cef_subprocess();
    network_crypto::install_network_crypto_provider();

    // Guard must outlive GPUI so buffered file logs are flushed during shutdown.
    let _logging_guard = logging::initialize();
    app::browser::initialize_safari_browser_identity();

    let application = Application::new().with_assets(EmbeddedAssets);
    application.on_reopen(|cx| {
        if !settings::general::is_ready(cx) {
            return;
        }
        if cx.windows().is_empty() {
            if let Err(error) = license_gate::open_gated_window(cx) {
                tracing::warn!(%error, "failed to reopen Norma window");
            }
        }
        cx.activate(true);
    });
    application.run(|cx: &mut App| {
        // gpui-component：主题与组件 actions；须在开窗前初始化。
        ui::init_components(cx);
        performance_monitor::install(cx);
        settings::orchestration::health::install(cx);
        settings::agent_hook_ingress::install(cx)
            .expect("failed to start App-global Agent Hook ingress");
        updater::install(cx);
        if let Err(error) = norma::endpoint::install(cx) {
            tracing::error!(
                %error,
                "App-global Norma endpoint unavailable; another App process may own it"
            );
        }

        // Quit 是唯一由 GPUI keymap 固定绑定的菜单快捷键；可配置命令仍由
        // ShortcutSettingsState 统一处理，暂不在原生菜单中显示提示。
        register_global_actions(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", QuitNormaOs, None)]);
        cx.set_menus(menu_bar::menu_bar());

        cx.on_window_closed(|cx| {
            let background = workspace::sessions::windows_closed(cx);
            workspace::desktop::sessions::windows_closed(background, cx);
            if cx.windows().is_empty() && !background {
                settings::general::quit_after_saving(cx);
            }
        })
        .detach();
        cx.on_app_quit(|cx| {
            media::shutdown(cx);
            workspace::desktop::sessions::shutdown(cx);
            workspace::sessions::shutdown(cx);
            async {}
        })
        .detach();

        // The opt-in `licenses` feature controls the hard gate (ADR 0015).
        // Default and packaged builds enter the workbench without a license.
        settings::general::open_initial_window(cx);
    });
}
