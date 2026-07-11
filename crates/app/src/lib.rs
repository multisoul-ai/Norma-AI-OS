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
mod motion;
mod norma;
mod preview;
mod settings;
mod shortcuts;
mod workspace;

pub use app::{
    ApiKeyDisplayState, AppScreen, CopyCardContent, ExitFocus, InitializationAnimationE2eSnapshot,
    MultisoulApp, NewBrowser, NewClaudeAgent, NewCodexAgent, NewOpenCodeAgent, NewTerminal,
    NewWindow, OpenPreviewFile, OpenProject, ReflowLayout, SettingsE2eSnapshot, ToggleMasterLayout,
    ToggleSettings,
};
pub use assets::EmbeddedAssets;

use gpui::{App, Application, Menu, MenuItem};
use workspace::windowing::{open_multisoul_window, InitialWindowState};

pub fn run_norma_os() {
    app::cef_process::exit_if_cef_subprocess();

    // RUST_LOG 控制级别；frame-time / 热点路径打点见各 view。
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    Application::new()
        .with_assets(EmbeddedAssets)
        .run(|cx: &mut App| {
            // gpui-component：主题与组件 actions；须在开窗前初始化。
            ui::init_components(cx);

            // 菜单（App 显示名 Norma OS）。
            cx.set_menus(vec![Menu {
                name: "Norma OS".into(),
                items: vec![
                    MenuItem::action("New Window", NewWindow),
                    MenuItem::action("New Terminal", NewTerminal),
                    MenuItem::action("New Browser", NewBrowser),
                    MenuItem::action("Copy", CopyCardContent),
                    MenuItem::action("Open Project", OpenProject),
                    MenuItem::action("Preview File", OpenPreviewFile),
                    MenuItem::action("New Claude Agent", NewClaudeAgent),
                    MenuItem::action("New Codex Agent", NewCodexAgent),
                    MenuItem::action("New OpenCode Agent", NewOpenCodeAgent),
                    MenuItem::action("Reflow Layout", ReflowLayout),
                    MenuItem::action("Toggle Master Layout", ToggleMasterLayout),
                    MenuItem::action("Settings", ToggleSettings),
                ],
            }]);

            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            open_multisoul_window(InitialWindowState::Launchpad, cx)
                .expect("failed to open window");
            cx.activate(true);
        });
}
