//! E2E 测试 crate：在 headless GPUI `TestAppContext` 中驱动 `MultisoulApp`。
//!
//! 本 crate 隔离 headless GPUI 启动，避免污染域层的快速单测。`gpui` 的
//! `test-support` feature 在此 crate 局部启用，生产 `cargo build -p app`
//! 不受影响。跨 crate domain-level 验收测试放在 `tests/`。

#![cfg(test)]

use app::{AppScreen, MultisoulApp};
use gpui::TestAppContext;

/// E2E 驱动 harness：装配 MultisoulApp、进入 Canvas、创建 Terminal、读取 runtime。
/// 详见 `harness.rs` 顶部模块文档。Task 8/9 (E2E-01/02) 在此模块上构建。
mod harness;

// E2E-01（SPEC §4.2）：创建 Terminal 节点 + 焦点 + layout reflow 正/负断言矩阵。
mod e2e_01_create_terminal;

// 原生菜单中的 OpenCode Agent action：Canvas root 必须实际注册并创建对应 Agent 节点。
mod e2e_01_menu_actions;

// E2E-02（SPEC §4.3）：向 focused Term 节点写入输入 → 剧本后端回吐 → grid 含回吐内容。
mod e2e_02_terminal_echo;

// E2E-02-real（SPEC §3.4，L2 `#[ignore]`）：真实 portable_pty 后端，本地 `--ignored` 跑。
mod e2e_02_real_pty;

// Settings E2E：真实点击 Settings 面板控件、录制快捷键、恢复默认、关闭面板。
mod e2e_03_settings_clicks;
mod e2e_03_settings_navigation;
mod e2e_03_voice_modes;

// Settings E2E：Voice Provider 文本输入无需 Enter 即自动保存。
mod e2e_04_voice_autosave;

// 卡片初始化动画 E2E：新建路径登记、帧推进和恢复路径不重播。
mod e2e_05_card_initialization_animation;

// Agent 任务结束后：即使交互式 PTY 存活，光标运行态动画也必须停止。
mod e2e_05_agent_task_cursor_animation;

// 语音驱动窗口内缘光：PCM 能量、Settings 层级与正常收束。
mod e2e_06_agent_announcements;
mod e2e_06_voice_window_inset;

// 麦克风权限被拒绝后：语音栏提供 macOS 隐私设置恢复入口。
mod e2e_07_microphone_permission_recovery;

// Browser 地址栏：平台文本输入、选区、复制、粘贴与 Enter 提交。
mod e2e_08_browser_address_input;

// Browser 卡片：内容区选择与清除选择的画布状态闭环。
mod e2e_09_browser_card_selection;

// 语音全链路场景矩阵：快捷键、听写触发、配置持久化、模型切换与运行时路由。
mod e2e_10_tts_model_catalog;
mod e2e_10_voice_config_matrix;
mod e2e_10_voice_matrix;
mod e2e_10_voice_model_matrix;

// Launchpad：启动页固定视频与 Canvas 壁纸设置解耦。
mod e2e_11_launchpad_video;

// 可语音呼叫 Agent：Settings 未保存草稿与快速创建的定义/卡片闭环。
mod e2e_12_voice_addressable_agents;

// 重导出 harness 的公开 helper，使 Task 8/9 在 lib.rs 中编写的测试可直接
// `use harness::{make_app, enter_canvas, ...}` 或经 `crate::harness::...` 访问。
pub use harness::{
    create_codex_agent, create_cursor_agent, create_terminal, drain_until, echo_factory,
    enter_canvas, make_app, make_app_real, make_app_with_settings_store, read_grid, read_runtime,
    scripted_factory,
};

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
