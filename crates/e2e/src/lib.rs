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

// Cards 数字快捷键：Cmd+1..Cmd+5 必须创建 Terminal 与四种 Agent profile。
mod e2e_01_card_shortcuts;

// 原生菜单中的 OpenCode Agent action：Canvas root 必须实际注册并创建对应 Agent 节点。
mod e2e_01_menu_actions;

// E2E-02（SPEC §4.3）：向 focused Term 节点写入输入 → 剧本后端回吐 → grid 含回吐内容。
mod e2e_02_terminal_echo;

// E2E-02：用户删除 live 卡片时，必须沿统一关闭路由请求终止 PTY 后端。
mod e2e_02_process_cleanup;

// E2E-02：终端应用输出 DSR 查询时，卡片必须将坐标报告写回同一 PTY。
mod e2e_02_terminal_dsr;

// E2E-02：component Root 不得拦截 Terminal / Agent 卡片需要发送给 PTY 的 Tab。
mod e2e_02_card_tab_input;

// Agent 图片粘贴按 CLI 协议分流：Cursor 路径回退，Codex/Claude 原生附件快捷键。
mod e2e_02_agent_image_paste;

// Agent 卡片重开项目时，应从 Provider 本地历史恢复缺失的持久化 session id。
mod e2e_02_agent_session_restore;

// E2E-02-real（SPEC §3.4，L2 `#[ignore]`）：真实 portable_pty 后端，本地 `--ignored` 跑。
mod e2e_02_real_pty;

// Settings E2E：真实点击 Settings 面板控件、录制快捷键、恢复默认、关闭面板。
mod e2e_03_api_key_visibility;
mod e2e_03_azure_asr_settings;
mod e2e_03_orchestration_settings;
mod e2e_03_settings_clicks;
mod e2e_03_settings_navigation;
mod e2e_03_theme_switching;
mod e2e_03_voice_caption;
mod e2e_03_voice_design;
mod e2e_03_voice_modes;
mod e2e_03_voice_setup;
mod e2e_03_voice_shortcut_settings;
mod e2e_03_wallpaper_badges;

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
mod e2e_09_browser_chrome_auth;

// 语音全链路场景矩阵：快捷键、听写触发、配置持久化、模型切换与运行时路由。
mod e2e_10_tts_model_catalog;
mod e2e_10_tts_providers;
mod e2e_10_voice_config_matrix;
mod e2e_10_voice_matrix;
mod e2e_10_voice_model_matrix;

// Launchpad：启动页固定视频与 Canvas 壁纸设置解耦。
mod e2e_11_launchpad_video;

// 可语音呼叫 Agent：Settings 未保存草稿与快速创建的定义/卡片闭环。
mod e2e_12_agent_presentation;
mod e2e_12_agent_queue_ui;
mod e2e_12_agent_settings_ime;
mod e2e_12_agent_submit;
mod e2e_12_agent_submit_l3;
mod e2e_12_norma_master;
mod e2e_12_orchestration_lifecycle;
mod e2e_12_session_reply;
mod e2e_12_session_reply_card_present;
mod e2e_12_session_reply_queue;
mod e2e_12_voice_addressable_agents;

// Agent/terminal 输出链接：点击后新建 Browser 并仅在该路径进入 master/detail 布局。
mod e2e_13_terminal_links;

// Product UI V2：底部 Dock 真实点击不应断开 Terminal reflow 与 Settings 流程。
mod e2e_14_product_ui_contract;

// Launchpad：Recent projects 的原生右键菜单删除及 SQLite 持久化闭环。
mod e2e_15_recent_project_menu;

// Canvas：创建/清理失败必须渲染可见、可关闭的工作区错误横幅。
mod e2e_16_canvas_error_feedback;

// License Gate：无凭证启动必须安全完成首帧，并展示可编辑的激活界面。
mod e2e_17_license_gate_startup;

// 长回复 TTS：慢速 Provider 下，完整显示、逐段合成与播放收束必须形成跨 crate 闭环。
mod e2e_18_tts_reply_backlog;

// Terminal burst：Runtime drain 与 repaint gate 合并输出但不得丢最终 grid。
mod e2e_19_terminal_repaint_coalescing;

// Open Project：原生选择结果返回后必须从组件 Root 窗口进入所选项目。
mod e2e_20_project_picker;

// 重导出 harness 的公开 helper，使 Task 8/9 在 lib.rs 中编写的测试可直接
// `use harness::{make_app, enter_canvas, ...}` 或经 `crate::harness::...` 访问。
pub use harness::{
    create_claude_agent, create_codex_agent, create_cursor_agent, create_terminal, drain_until,
    echo_factory, enter_canvas, make_app, make_app_real, make_app_with_settings_store, read_grid,
    read_runtime, scripted_factory,
};

/// E2E-T-1：harness 的 mock 时钟步长必须读取生产 Runtime frame timing contract。
///
/// 数据构造（含关键数值的推导过程）：
///   production Runtime interval = 16ms（1000 / 60 ≈ 16.67ms，策略取 16ms）
///   legacy hard-coded interval = 12ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 harness 读取单轮 runtime drain mock-clock step。
///   2. 与 app crate 暴露的 test-support contract 比较。
///   3. 与旧 12ms 魔数做负对照。
///
/// 预期结果：
///   - 正断言：harness step 与生产 contract 完全相同且为 16ms。
///   - 负断言：harness 不得继续硬编码旧的 12ms。
#[test]
fn e2e_runtime_drain_step_matches_production_timing_contract() {
    let step = harness::runtime_drain_step();
    let production = app::runtime_frame_interval_for_e2e();

    assert_eq!(
        step, production,
        "E2E drain step 必须直接读取 app 的生产 Runtime timing contract"
    );
    assert_eq!(
        step,
        std::time::Duration::from_millis(16),
        "生产 Runtime interval 已收敛到 16ms，E2E mock 时钟必须同步"
    );
    assert_ne!(
        step,
        std::time::Duration::from_millis(12),
        "E2E harness 不得继续硬编码旧的 12ms drain 周期"
    );
}

/// 数据库隔离：标准 E2E Harness 必须同时使用独占 app DB 与测试项目文件名。
///
/// 数据构造（含关键数值的推导过程）：
///   App 实例 = 1
///   临时 workspace = 1
///   Test SQLite = app.sqlite(1) + project.test.sqlite(1) = 2
///   Production 哨兵 = app.sqlite(1) + project.sqlite(1) = 2
///   字节不变比较 = before(2) + after(2) = 4 次读取
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在合成 Production app 路径和同一 workspace 的 `project.sqlite` 写入哨兵。
///   2. 通过标准 make_app Harness 创建带 Guard 的 headless App。
///   3. 读取 App 实际选定的 app DB 路径并进入临时 workspace。
///   4. 迁移 App 选定的 Test ProjectStore，再重读两个 Production 哨兵。
///
/// 预期结果：
///   - 正断言：app DB 位于 `norma-os-database-tests` 独占临时根且已创建。
///   - 正断言：ProjectStore 指向并创建 `project.test.sqlite`。
///   - 正断言：Production app/project 哨兵在 E2E 前后字节完全不变。
///   - 负断言：Test app/project 路径不得等于对应的 Production 路径。
#[test]
fn e2e_harness_uses_guarded_app_and_test_project_databases() {
    let fixture = tempfile::tempdir().expect("E2E 数据库隔离应创建 Production fixture 根");
    let production_paths =
        storage::DatabasePaths::production_for_home(&fixture.path().join("production-home"));
    std::fs::create_dir_all(
        production_paths
            .app_db_path()
            .parent()
            .expect("Production app DB 必须有父目录"),
    )
    .expect("E2E 应创建合成 Production app 数据目录");
    std::fs::write(production_paths.app_db_path(), b"production-app-sentinel")
        .expect("E2E 应写入 Production app DB 哨兵");
    let mut test_cx = TestAppContext::single();
    let (app, cx) = harness::make_app(&mut test_cx, harness::echo_factory());
    let workspace = tempfile::tempdir().expect("E2E 数据库隔离应创建临时 workspace");
    let production_project_db = workspace.path().join(".multisoul").join("project.sqlite");
    std::fs::create_dir_all(
        production_project_db
            .parent()
            .expect("Production project DB 必须有父目录"),
    )
    .expect("E2E 应创建 workspace 存储目录");
    std::fs::write(&production_project_db, b"production-project-sentinel")
        .expect("E2E 应写入 Production project DB 哨兵");
    let production_app_before = std::fs::read(production_paths.app_db_path())
        .expect("E2E 运行前应读取 Production app 哨兵");
    let production_project_before =
        std::fs::read(&production_project_db).expect("E2E 运行前应读取 Production project 哨兵");

    let app_db = app.read_with(cx, |app, _cx| app.app_database_path_for_e2e());
    harness::enter_canvas(&app, cx, workspace.path());
    let project_store = harness::project_store(&app, cx, workspace.path());
    project_store
        .migrate()
        .expect("E2E Test ProjectStore migration 应成功");
    let production_app_after = std::fs::read(production_paths.app_db_path())
        .expect("E2E 运行后应读取 Production app 哨兵");
    let production_project_after =
        std::fs::read(&production_project_db).expect("E2E 运行后应读取 Production project 哨兵");

    assert!(
        app_db.to_string_lossy().contains("norma-os-database-tests"),
        "标准 E2E app DB 必须位于 Guard 管理的独占临时根，实际: {}",
        app_db.display()
    );
    assert!(
        app_db.is_file(),
        "标准 E2E App 启动后必须在临时根创建 app.sqlite"
    );
    assert!(
        project_store.path().ends_with("project.test.sqlite"),
        "标准 E2E ProjectStore 必须使用 project.test.sqlite"
    );
    assert!(
        project_store.path().is_file(),
        "E2E workspace 持久化必须创建 project.test.sqlite"
    );
    assert_eq!(
        production_app_after, production_app_before,
        "标准 E2E 运行前后 Production app DB 哨兵字节必须完全不变"
    );
    assert_eq!(
        production_project_after, production_project_before,
        "标准 E2E 运行前后 Production project.sqlite 哨兵字节必须完全不变"
    );
    assert_ne!(
        app_db,
        production_paths.app_db_path(),
        "标准 E2E app DB 不得解析到 Production app DB"
    );
    assert!(
        project_store.path() != production_project_db,
        "标准 E2E ProjectStore 不得解析到 Production project.sqlite"
    );
}

const DATABASE_FAILURE_CHILD_ENV: &str = "NORMA_E2E_DATABASE_FAILURE_CHILD";
const DATABASE_FAILURE_ROOT_ENV: &str = "NORMA_E2E_DATABASE_FAILURE_ROOT";

/// 数据库失败封闭：不可写 Test 根必须明确失败，且不能向当前目录回退。
///
/// 数据构造（含关键数值的推导过程）：
///   独立测试进程 = parent(1) + child(1) = 2
///   blocked root 权限 = 0o500（owner read(4) + execute(1)，write(2) 缺失）
///   候选输出 = blocked/app.sqlite(1) + cwd/app.sqlite fallback(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Parent 创建不可写 Test 根和独立 cwd，再启动当前 E2E binary 的单测试子进程。
///   2. Child 用显式 Test DatabasePaths 初始化 AppStore。
///   3. Child 验证错误包含 Test 模式与 blocked/app.sqlite 目标路径。
///   4. Parent/Child 分别确认 cwd 下没有生成 fallback `app.sqlite`。
///
/// 预期结果：
///   - 正断言：子进程成功完成“预期初始化失败”的检查。
///   - 正断言：错误明确包含 Test 模式和不可写目标路径。
///   - 负断言：失败过程中不得在当前目录创建 `app.sqlite`。
#[test]
fn e2e_database_initialization_failure_is_closed_without_cwd_fallback() {
    if std::env::var_os(DATABASE_FAILURE_CHILD_ENV).is_some() {
        assert_database_initialization_fails_in_child();
        return;
    }

    use std::os::unix::fs::PermissionsExt;

    let fixture = tempfile::tempdir().expect("失败封闭 E2E 应创建临时根");
    let blocked_root = fixture.path().join("blocked");
    let child_cwd = fixture.path().join("cwd");
    std::fs::create_dir(&blocked_root).expect("失败封闭 E2E 应创建 blocked 根");
    std::fs::create_dir(&child_cwd).expect("失败封闭 E2E 应创建 child cwd");
    std::fs::set_permissions(&blocked_root, std::fs::Permissions::from_mode(0o500))
        .expect("失败封闭 E2E 应移除 blocked 根写权限");

    let output = std::process::Command::new(
        std::env::current_exe().expect("失败封闭 E2E 应解析当前测试 binary"),
    )
    .args([
        "--exact",
        "e2e_database_initialization_failure_is_closed_without_cwd_fallback",
        "--nocapture",
    ])
    .env(DATABASE_FAILURE_CHILD_ENV, "1")
    .env(DATABASE_FAILURE_ROOT_ENV, &blocked_root)
    .current_dir(&child_cwd)
    .output()
    .expect("失败封闭 E2E 应启动隔离子进程");

    std::fs::set_permissions(&blocked_root, std::fs::Permissions::from_mode(0o700))
        .expect("失败封闭 E2E 应恢复 blocked 根权限以便清理");

    assert!(
        output.status.success(),
        "子进程必须完成 Test 初始化失败断言；stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !child_cwd.join("app.sqlite").exists(),
        "Test 初始化失败后不得在子进程当前目录创建 fallback app.sqlite"
    );
}

fn assert_database_initialization_fails_in_child() {
    let root = std::path::PathBuf::from(
        std::env::var_os(DATABASE_FAILURE_ROOT_ENV).expect("子进程必须收到不可写 Test 根"),
    );
    let paths = storage::DatabasePaths::isolated_test(&root);
    let target = paths.app_db_path().to_path_buf();

    let error =
        storage::AppStore::initialize(&paths).expect_err("不可写 Test app DB 必须明确初始化失败");
    let message = error.to_string();

    assert!(
        message.contains("Test"),
        "初始化错误必须包含 Test 模式，实际: {message}"
    );
    assert!(
        message.contains(target.to_string_lossy().as_ref()),
        "初始化错误必须包含不可写目标路径，实际: {message}"
    );
    assert!(
        !std::path::Path::new("app.sqlite").exists(),
        "Child 不得在当前目录创建 fallback app.sqlite"
    );
}

/// Smoke：headless GPUI 启动 `MultisoulApp`，初始屏幕应为 `Launchpad`。
///
/// 数据构造：
///   - `TestAppContext::single()`：单窗口 headless 测试上下文（gpui test-support）。
///   - `MultisoulApp::new_with_isolated_test_database(cx)`：装配独占测试数据库、
///     Runtime、AgentHookServer、wallpaper 与
///     ~12ms 帧循环；初始 `screen = AppScreen::Launchpad`（见 `assemble`）。
///     无外部 PTY / 项目路径注入，仅验证 boot 路径。
///
/// 执行过程：
///   1. `cx.add_window_view(...new_with_isolated_test_database...)` → GPUI 开窗、
///      构造 root view；内部 `run_until_parked` 让帧循环 / async 任务推进到稳态。
///   2. 取回 `Entity<MultisoulApp>`，读取 `screen()`。
///
/// 预期结果：
///   - 正断言：`screen() == AppScreen::Launchpad`（boot 后默认进入 Launchpad）。
///   - 负断言：`screen() != AppScreen::Canvas`（未 OpenProject，不应进入画布）。
#[test]
fn smoke_headless_gpui_boots_to_launchpad() {
    let mut cx = TestAppContext::single();

    let (app, _window_cx) =
        cx.add_window_view(|_window, cx| MultisoulApp::new_with_isolated_test_database(cx));

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
