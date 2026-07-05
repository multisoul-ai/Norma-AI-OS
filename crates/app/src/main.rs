//! multisoul-desktop 入口：tracing 初始化 + 启动 GPUI App。
//! multisoul-desktop 入口：tracing 初始化 + 启动 GPUI App。
//! 模块声明见 `src/lib.rs`（bin 隐式依赖 lib）。此处仅装配运行时。
use app::{
    open_multisoul_window, CopyCardContent, EmbeddedAssets, InitialWindowState, NewClaudeAgent,
    NewCodexAgent, NewOpenCodeAgent, NewTerminal, NewWindow, OpenPreviewFile, OpenProject,
    ReflowLayout, ToggleMasterLayout, ToggleSettings,
};
use gpui::{App, Application, Menu, MenuItem};

fn main() {
    // RUST_LOG 控制级别；frame-time / 热点路径打点见各 view。
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    Application::new()
        .with_assets(EmbeddedAssets)
        .run(|cx: &mut App| {
            // 菜单（App 显示名 Multisoul）。
            cx.set_menus(vec![Menu {
                name: "Multisoul".into(),
                items: vec![
                    MenuItem::action("New Window", NewWindow),
                    MenuItem::action("New Terminal", NewTerminal),
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
