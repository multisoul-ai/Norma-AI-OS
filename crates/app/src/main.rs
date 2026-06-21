//! multisoul-desktop 入口：tracing 初始化 + 启动 GPUI App。
mod app;
mod app_ime;
mod assets;
mod canvas_view;
mod cards;
mod launchpad;
mod master_layout;
mod motion;
mod workspace_flow;
mod workspace_state;

use app::{
    ExitFocus, MultisoulApp, NewClaudeAgent, NewCodexAgent, NewTerminal, OpenProject, ReflowLayout,
    ToggleMasterLayout,
};
use assets::EmbeddedAssets;
use gpui::{
    px, size, App, AppContext as _, Application, Bounds, KeyBinding, Menu, MenuItem,
    TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};

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
            // 快捷键（SPEC §7）：Cmd+N 新建终端、Esc 退出终端聚焦。
            cx.bind_keys([
                KeyBinding::new("cmd-n", NewTerminal, None),
                KeyBinding::new("cmd-o", OpenProject, None),
                KeyBinding::new("cmd-m", ToggleMasterLayout, None),
                KeyBinding::new("cmd-shift-l", ReflowLayout, None),
                KeyBinding::new("escape", ExitFocus, None),
            ]);

            // 菜单（App 显示名 Multisoul）。
            cx.set_menus(vec![Menu {
                name: "Multisoul".into(),
                items: vec![
                    MenuItem::action("New Terminal", NewTerminal),
                    MenuItem::action("Open Project", OpenProject),
                    MenuItem::action("New Claude Agent", NewClaudeAgent),
                    MenuItem::action("New Codex Agent", NewCodexAgent),
                    MenuItem::action("Reflow Layout", ReflowLayout),
                    MenuItem::action("Toggle Master Layout", ToggleMasterLayout),
                ],
            }]);

            let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Multisoul".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    window_background: WindowBackgroundAppearance::Blurred,
                    ..Default::default()
                },
                |_window, cx| cx.new(MultisoulApp::new),
            )
            .expect("failed to open window");
            cx.activate(true);
        });
}
