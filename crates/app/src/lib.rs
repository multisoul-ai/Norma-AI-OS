//! `app` crate 的库目标：暴露 GPUI shell（`MultisoulApp` / `AppScreen` / actions）
//! 给 e2e 测试 crate。二进制目标 `multisoul`（`src/main.rs`）隐式依赖本 lib。
//!
//! 模块声明保持私有（`mod`，非 `pub mod`）：内部模块树无需对外暴露，仅通过
//! 下方 `pub use` 释放 e2e / bin 所需的公开 API，避免泄漏 `pub(crate)` 类型。
mod app;
mod assets;
mod canvas;
mod cards;
mod frame_diagnostics;
mod license_gate;
mod menu_bar;
mod motion;
mod norma;
mod preview;
mod settings;
mod shortcuts;
mod workspace;

#[cfg(test)]
#[path = "../tests/unit/menu_bar_tests.rs"]
mod menu_bar_tests;

pub use app::{
    AgentDefinitionE2eSnapshot, AgentSettingsE2eSnapshot, ApiKeyDisplayState, AppScreen,
    CopyCardContent, ExitFocus, InitializationAnimationE2eSnapshot, MultisoulApp, NewBrowser,
    NewClaudeAgent, NewCodexAgent, NewCursorAgent, NewOpenCodeAgent, NewTerminal, NewWindow,
    OpenPreviewFile, OpenProject, QuitNormaOs, ReflowLayout, SettingsE2eSnapshot,
    ToggleMasterLayout, ToggleSettings,
};
pub use assets::EmbeddedAssets;

#[cfg(feature = "e2e-test-support")]
pub use license_gate::build_activation_root_e2e;

use gpui::{App, Application, KeyBinding};

pub fn run_norma_os() {
    app::cef_process::exit_if_cef_subprocess();

    // RUST_LOG 控制级别；frame-time / 热点路径打点见各 view。
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    app::browser::initialize_safari_browser_identity();

    Application::new()
        .with_assets(EmbeddedAssets)
        .run(|cx: &mut App| {
            // gpui-component：主题与组件 actions；须在开窗前初始化。
            ui::init_components(cx);

            // Quit 是唯一由 GPUI keymap 固定绑定的菜单快捷键；可配置命令仍由
            // ShortcutSettingsState 统一处理，暂不在原生菜单中显示提示。
            cx.on_action(|_: &QuitNormaOs, cx| cx.quit());
            cx.bind_keys([KeyBinding::new("cmd-q", QuitNormaOs, None)]);
            cx.set_menus(menu_bar::menu_bar());

            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            // Hard gate: Activation shell or Workbench via GateRoot (ADR 0015).
            // Developers: NORMA_SKIP_LICENSE=1. E2E still constructs MultisoulApp directly.
            license_gate::open_gated_window(cx).expect("failed to open window");
            cx.activate(true);
        });
}
