//! E2E 测试 crate：在 headless GPUI `TestAppContext` 中驱动 `MultisoulApp`。
//!
//! 本 crate 隔离 headless GPUI 启动，避免污染域层的快速单测。`gpui` 的
//! `test-support` feature 在此 crate 局部启用，生产 `cargo build -p app`
//! 不受影响。

#![cfg(test)]

use app::{AppScreen, MultisoulApp};
use gpui::TestAppContext;

/// 子模块占位：后续 Task 在此扩展 helper（如 `enter_canvas`、`spawn_term`）。
mod helpers {}

/// Smoke：headless GPUI 启动 `MultisoulApp`，初始屏幕应为 `Launchpad`。
///
/// 数据构造：
///   - `TestAppContext::single()`：单窗口 headless 测试上下文（gpui test-support）。
///   - `MultisoulApp::new(cx)`：装配 Runtime、AgentHookServer、wallpaper 与
///     ~12ms 帧循环；初始 `screen = AppScreen::Launchpad`（见 `assemble`）。
///   无外部 PTY / 项目路径注入，仅验证 boot 路径。
///
/// 执行过程：
///   1. `cx.add_window_view(|_window, cx| MultisoulApp::new(cx))` → GPUI 开窗、
///      构造 root view；内部 `run_until_parked` 让帧循环 / async 任务推进到稳态。
///   2. 取回 `Entity<MultisoulApp>`，读取 `screen()`。
///
/// 预期结果：
///   - 正断言：`screen() == AppScreen::Launchpad`（boot 后默认进入 Launchpad）。
///   - 负断言：`screen() != AppScreen::Canvas`（未 OpenProject，不应进入画布）。
#[test]
fn smoke_headless_gpui_boots_to_launchpad() {
    let mut cx = TestAppContext::single();

    let (app, _window_cx) = cx.add_window_view(|_window, cx| MultisoulApp::new(cx));

    let screen = app.update(&mut cx, |a, _cx| a.screen());

    assert_eq!(
        screen,
        AppScreen::Launchpad,
        "boot 后屏幕应为 Launchpad，实际为 {:?}",
        screen
    );
    assert_ne!(
        screen,
        AppScreen::Canvas,
        "未 OpenProject，屏幕不应为 Canvas"
    );
}
