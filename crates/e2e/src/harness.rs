//! E2E 测试驱动 harness：在 headless GPUI `TestAppContext` 中装配 `MultisoulApp`、
//! 进入 Canvas、创建 Terminal 节点，并读取 runtime 状态供断言。
//!
//! 本模块仅供 e2e crate 内部测试使用（`#![cfg(test)]` 已在 lib.rs 顶层开启）。
//! 所有 helper 仅依赖 `app` / `runtime` / `canvas` / `terminal` 的公开 API，
//! 不触达 `pub(crate)` 字段或私有方法。

mod settings;
pub(crate) use settings::scroll_settings_target_into_view;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use app::{MultisoulApp, NewClaudeAgent, NewCodexAgent, NewCursorAgent, NewTerminal};
use canvas::{CanvasNodeKind, NodeId};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use runtime::{NodeView, PtyFactory, Runtime};
use terminal::{GridSnapshot, PtyBackend, ScriptRule, TestPtyBackend};

static VOICE_TRANSCRIPT_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static VOICE_MODEL_INSTALL_E2E_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) struct VoiceModelInstallE2eGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl VoiceModelInstallE2eGuard {
    pub(crate) fn acquire() -> Self {
        Self {
            _lock: VOICE_MODEL_INSTALL_E2E_LOCK
                .lock()
                .expect("Voice model install e2e lock should not be poisoned"),
        }
    }
}

pub(crate) struct VoiceTranscriptEnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous: Option<std::ffi::OsString>,
}

impl VoiceTranscriptEnvGuard {
    pub(crate) fn set(transcript: &str) -> Self {
        let lock = VOICE_TRANSCRIPT_ENV_LOCK
            .lock()
            .expect("语音 transcript 环境变量测试锁不应中毒");
        let previous = std::env::var_os("MULTISOUL_VOICE_TRANSCRIPT");
        std::env::set_var("MULTISOUL_VOICE_TRANSCRIPT", transcript);
        Self {
            _lock: lock,
            previous,
        }
    }

    pub(crate) fn clear() -> Self {
        let lock = VOICE_TRANSCRIPT_ENV_LOCK
            .lock()
            .expect("语音 transcript 环境变量测试锁不应中毒");
        let previous = std::env::var_os("MULTISOUL_VOICE_TRANSCRIPT");
        std::env::remove_var("MULTISOUL_VOICE_TRANSCRIPT");
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for VoiceTranscriptEnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            std::env::set_var("MULTISOUL_VOICE_TRANSCRIPT", previous);
        } else {
            std::env::remove_var("MULTISOUL_VOICE_TRANSCRIPT");
        }
    }
}

/// 构造一个 echo PTY 工厂：`write(bytes)` → 同步回吐 `PtyEvent::Output(bytes)`。
///
/// 供 `make_app` / `enter_canvas` 注入，使 `NewTerminal` 走 `TestPtyBackend` 而非
/// 真实 PTY。每次调用返回独立的工厂实例（`PtyFactory` 是 `Box<dyn Send + Sync>`，
/// 不可 `Clone`）。
pub fn echo_factory() -> PtyFactory {
    Box::new(|_cmd, _cols, _rows, tx| Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>))
}

/// 构造一个剧本 PTY 工厂：`write(bytes)` 若含某 `rule.trigger` 子串，
/// 则通过 `tx` 投递 `PtyEvent::Output(rule.output)`；无匹配则静默。
///
/// 与 `echo_factory` 对称，供 E2E-02 等需要“特定输入 → 特定输出”映射的场景使用。
/// 每次 `write` 仅取首个匹配的 rule（`TestPtyBackend::scripted` 语义）。
pub fn scripted_factory(rules: Vec<ScriptRule>) -> PtyFactory {
    Box::new(move |_cmd, _cols, _rows, tx| {
        Ok(Box::new(TestPtyBackend::scripted(tx, rules.clone())) as Box<dyn PtyBackend>)
    })
}

/// 装配 headless GPUI 窗口 + `MultisoulApp`，注入测试 PTY 工厂，并 focus root view。
///
/// 内部创建 `MultisoulApp` entity，再像生产窗口一样用 `ui::component::Root` 包裹。
/// `add_window_view` 末尾的 `run_until_parked` 会绘制初始帧（Launchpad），此时
/// `render()` 中注册的 `.on_action(...)` 监听器已挂到 dispatch tree。随后显式
/// `window.focus(&app.focus_handle)` + `activate_window()`，使后续
/// `dispatch_action` 能解析到 root view 的 dispatch node（GPUI 的 action
/// 分发依赖 focused element；未 focus 时 dispatch 是 no-op）。
///
/// 返回 root view entity 与 `&mut VisualTestContext`（借用传入的 `cx`）。
/// 调用方后续用返回的 `VisualTestContext` dispatch action / `run_until_parked`。
pub fn make_app(
    cx: &mut TestAppContext,
    factory: PtyFactory,
) -> (Entity<MultisoulApp>, &mut VisualTestContext) {
    let view_slot = Arc::new(Mutex::new(None));
    let capture = view_slot.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| MultisoulApp::new_with_pty_factory(cx, factory));
        *capture.lock().expect("e2e app capture mutex should lock") = Some(view.clone());
        ui::component::Root::new(view, window, cx)
    });
    let view = view_slot
        .lock()
        .expect("e2e app capture mutex should lock")
        .take()
        .expect("component root should capture MultisoulApp entity");
    focus_root(&view, cx);
    (view, cx)
}

/// 装配使用指定 SettingsStore 的 headless App，供持久化后重新加载的 e2e 使用。
pub fn make_app_with_settings_store(
    cx: &mut TestAppContext,
    factory: PtyFactory,
    store: storage::SettingsStore,
) -> (Entity<MultisoulApp>, &mut VisualTestContext) {
    let view_slot = Arc::new(Mutex::new(None));
    let capture = view_slot.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_voice_settings_store(cx, factory, store)
        });
        *capture.lock().expect("e2e app capture mutex should lock") = Some(view.clone());
        ui::component::Root::new(view, window, cx)
    });
    let view = view_slot
        .lock()
        .expect("e2e app capture mutex should lock")
        .take()
        .expect("component root should capture MultisoulApp entity");
    focus_root(&view, cx);
    (view, cx)
}

/// 装配 headless GPUI 窗口 + `MultisoulApp`，**不**注入 PTY 工厂（走真实 PTY 路径）。
///
/// 与 `make_app` 对称，但使用独占测试数据库且不调用 `set_pty_factory` →
/// `Runtime.pty_factory = None` → `spawn_term_live(None, ...)` →
/// `TermLive::spawn_command` → 真实 `portable_pty`。供 L2 `#[ignore]` 真 PTY 冒烟
/// （Task 10 / SPEC §3.4）使用：覆盖 `on_new_terminal` → 真实 shell spawn →
/// PTY 读线程 → grid 异步更新链路。
///
/// 注意：真实 shell 为 `/bin/zsh`（`on_new_terminal` 硬编码，无公开 API 覆盖）。
/// 调用方应使用 `contains("hi")` 这类对 zsh prompt/rc 噪声鲁棒的断言，而非精确
/// 行匹配。输出异步到达，必须用 `drain_until(predicate, timeout)` 轮询。
pub fn make_app_real(cx: &mut TestAppContext) -> (Entity<MultisoulApp>, &mut VisualTestContext) {
    let view_slot = Arc::new(Mutex::new(None));
    let capture = view_slot.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(MultisoulApp::new_with_isolated_test_database);
        *capture.lock().expect("e2e app capture mutex should lock") = Some(view.clone());
        ui::component::Root::new(view, window, cx)
    });
    let view = view_slot
        .lock()
        .expect("e2e app capture mutex should lock")
        .take()
        .expect("component root should capture MultisoulApp entity");
    focus_root(&view, cx);
    (view, cx)
}

/// 使用当前 App 已选定的数据库模式打开 workspace ProjectStore。
pub fn project_store(
    view: &Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
    workspace: &Path,
) -> storage::ProjectStore {
    view.read_with(cx, |app, _cx| app.project_store_for_e2e(workspace))
        .expect("E2E ProjectStore 应沿用 App 的测试数据库路径")
}

/// focus root view + activate window，否则 dispatch_action 找不到目标节点。
fn focus_root(view: &Entity<MultisoulApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.focus(&view.read(cx).focus_handle);
        window.activate_window();
    });
    cx.run_until_parked();
}

/// 进入 Canvas 屏幕：调用 `MultisoulApp::enter_canvas_for_test` 直接切换屏幕，
/// 并把 `root` 注入 `Runtime::set_workspace_root` 作为 cwd。
///
/// # 为什么不走真实 `OpenProject` action
/// 真实流程调用 `cx.prompt_for_paths`，而 GPUI 的 `TestPlatform::prompt_for_paths`
/// 是 `unimplemented!()`（只有 `prompt_for_new_path` 能被 `simulate_new_path_selection`
/// 驱动）。即便绕过文件选择器，`enter_workspace_for_path` 也会用全新 runtime
/// 替换旧 runtime，导致 `set_pty_factory` 注入的测试工厂丢失。
///
/// `enter_canvas_for_test`（`crates/app/src/app/mod.rs`）直接置 `screen = Canvas`、
/// 保留现有 runtime（含工厂）、不加载持久化快照，并通过 `set_workspace_root`
/// 注入 cwd。对 fresh tempdir 场景，这与真实流程行为等价（无快照可加载、
/// 无 session 可恢复），且为 L2（真实 PTY）提供了正确的 cwd。
/// Mobile fixtures use the production workspace identity/route, not only a visible canvas.
pub fn enter_mobile_canvas(
    view: &Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
    root: &Path,
) -> tempfile::TempDir {
    let sockets = tempfile::tempdir().expect("隔离手机路由");
    cx.update(|window, cx| {
        app::install_norma_endpoint_for_e2e(sockets.path().join("norma.sock"), cx)
            .expect("正式路由注册");
        view.update(cx, |app, cx| {
            app.enter_canvas_and_register_norma_route_for_e2e(root, window, cx)
        });
    });
    sockets
}

pub fn enter_canvas(view: &Entity<MultisoulApp>, cx: &mut VisualTestContext, root: &Path) {
    view.update(cx, |app, cx| app.enter_canvas_for_test(root, cx));
    cx.run_until_parked();
}

/// Dispatch `NewTerminal` 并返回新建 Term 节点的 id。
///
/// 通过比较 dispatch 前后 `document.nodes()` 中 Term 节点的最大 id 来定位新节点
/// （`Document::add_node` 分配单调递增 id，新节点 id 必然 > 先前最大 id）。
///
/// 前置条件：调用方已通过 `enter_canvas` 进入 Canvas 屏幕（否则 `on_new_terminal`
/// 会拒绝创建并写一条 error 到 `workspace_state`，本函数会 panic 在
/// `expect` 上）。
pub fn create_terminal(view: &Entity<MultisoulApp>, cx: &mut VisualTestContext) -> NodeId {
    let max_before = read_runtime(view, cx, |r| {
        r.document
            .nodes()
            .iter()
            .filter(|n| matches!(n.kind, CanvasNodeKind::Term(_)))
            .map(|n| n.id)
            .max()
            .unwrap_or(0)
    });
    cx.dispatch_action(NewTerminal);
    cx.run_until_parked();
    read_runtime(view, cx, |r| {
        r.document
            .nodes()
            .iter()
            .filter(|n| matches!(n.kind, CanvasNodeKind::Term(_)))
            .map(|n| n.id)
            .max()
            .filter(|id| *id > max_before)
            .expect(
                "NewTerminal should have spawned a new Term node with id greater than prior max",
            )
    })
}

/// Dispatch `NewCodexAgent` 并返回新建 Agent 节点的 id。
///
/// 选择 Codex 仅用于稳定地覆盖一个公开 Agent action；注入的测试 PTY 工厂仍会
/// 承担底层 live node，测试不依赖本机是否安装 Codex CLI。
pub fn create_codex_agent(view: &Entity<MultisoulApp>, cx: &mut VisualTestContext) -> NodeId {
    let max_before = read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .map(|node| node.id)
            .max()
            .unwrap_or(0)
    });
    cx.dispatch_action(NewCodexAgent);
    cx.run_until_parked();
    read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .map(|node| node.id)
            .max()
            .filter(|id| *id > max_before)
            .expect("NewCodexAgent should have spawned an Agent node with a larger id")
    })
}

/// Dispatch `NewClaudeAgent` 并返回新建 Agent 节点的 id。
///
/// 测试 PTY 工厂接管底层 live node，因此该 helper 只覆盖公开 Claude action 的
/// 创建与聚焦链路，不要求测试机安装 Claude Code。
pub fn create_claude_agent(view: &Entity<MultisoulApp>, cx: &mut VisualTestContext) -> NodeId {
    let max_before = read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .map(|node| node.id)
            .max()
            .unwrap_or(0)
    });
    cx.dispatch_action(NewClaudeAgent);
    cx.run_until_parked();
    read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .map(|node| node.id)
            .max()
            .filter(|id| *id > max_before)
            .expect("NewClaudeAgent should have spawned an Agent node with a larger id")
    })
}

/// Dispatch `NewCursorAgent` 并返回新建 Agent 节点的 id。
///
/// 测试 PTY 工厂接管底层 live node，因此该 helper 仅验证公开 Cursor action 的
/// 创建链路，不要求本机安装 Cursor Agent CLI。
pub fn create_cursor_agent(view: &Entity<MultisoulApp>, cx: &mut VisualTestContext) -> NodeId {
    let max_before = read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .map(|node| node.id)
            .max()
            .unwrap_or(0)
    });
    cx.dispatch_action(NewCursorAgent);
    cx.run_until_parked();
    read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .map(|node| node.id)
            .max()
            .filter(|id| *id > max_before)
            .expect("NewCursorAgent should have spawned an Agent node with a larger id")
    })
}

/// 读取 runtime 状态：`view.read_with(cx, |app, _| f(&app.runtime))` 的封装。
///
/// 仅借用 `cx` 的不可变视图，不触发 notify。适合在 dispatch 之间反复读取。
pub fn read_runtime<R>(
    view: &Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&Runtime) -> R,
) -> R {
    view.read_with(cx, |app, _cx| f(&app.runtime))
}

/// 循环 `advance_clock` + `run_until_parked` + 读 runtime，直到 `predicate` 返回
/// `Some(value)` 或超时。
///
/// L1（注入 PTY 同步回吐）通常一轮即收敛；L2（真实 PTY 异步）需要足够 timeout。
/// 超时返回 `None`，由调用方决定是否 panic（推荐 `.expect("...")` 带场景说明）。
///
/// 实现说明（mock 时钟驱动 drain timer）：
///   GPUI 的 `TestAppContext` 用 mock 时钟，`background_executor.timer(dur)` 返回的
///   future 只在 `advance_clock(dur)` 后才就绪。`MultisoulApp` 的生产 Runtime 周期
///   loop 依赖该 timer 触发 `runtime.drain_all()`。
///   仅 `run_until_parked` 不会推进 mock 时钟，drain 永不执行，PTY 输出永远停在
///   `TerminalSession` 的 rx 队列里——这正是 E2E-02 grid 不更新的根因。
///   因此每轮从 app test-support timing contract 读取 step 并推进 mock clock，再
///   `run_until_parked` 执行 drain 回调。wall-clock `sleep(1ms)` 仅用于避免 busy-loop。
pub(crate) fn runtime_drain_step() -> Duration {
    app::runtime_frame_interval_for_e2e()
}

pub fn drain_until<R>(
    view: &Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
    mut predicate: impl FnMut(&Runtime) -> Option<R>,
    timeout: Duration,
) -> Option<R> {
    let deadline = Instant::now() + timeout;
    // 单轮 advance 量直接读取生产 timing contract，保证至少触发一次 drain timer。
    let step = runtime_drain_step();
    loop {
        // 推进 mock 时钟让生产 drain timer 就绪，再跑 executor 执行 drain 回调。
        cx.cx.background_executor.advance_clock(step);
        cx.run_until_parked();
        if let Some(v) = read_runtime(view, cx, &mut predicate) {
            return Some(v);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// 读取指定 Term 节点的 `GridSnapshot`。
///
/// 经 `runtime.node_view(node_id)` 取 `NodeView::Term { snapshot, .. }`。
/// 若节点不存在或不是 Term 视图（如已退出 / session gone），返回 `None`。
/// 调用方通常 `.expect("...")` 带场景说明。生产渲染保留共享 Arc；这里只在
/// 测试 API 的 owned-value 边界显式深拷贝，避免把复制重新带回绘制热路径。
pub fn read_grid(
    view: &Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
    node_id: NodeId,
) -> Option<GridSnapshot> {
    read_runtime(view, cx, |r| match r.node_view(node_id) {
        Some(NodeView::Term { snapshot, .. }) => Some(snapshot.as_ref().clone()),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// 编译 / 烟雾测试：证明 harness 端到端可用（make_app → enter_canvas → create_terminal）。
// 这是 Task 7 唯一需要执行的测试；Task 8/9 会复用本 harness 编写更完整的 E2E。
// ---------------------------------------------------------------------------

/// harness 端到端烟雾：echo PTY → 进入 Canvas → 创建 1 个 Terminal 节点。
///
/// 数据构造：
///   - `echo_factory()`：回显 PTY 工厂，注入 `MultisoulApp::new_with_pty_factory`。
///     进入 canvas 走 `enter_canvas_for_test`（不替换 runtime），工厂得以保留。
///   - `tempfile::TempDir::new()`：临时 workspace root，传给 `enter_canvas`
///     注入 `Runtime::set_workspace_root`，验证 cwd 注入路径可用。
///     `_tmp` 持有到函数末尾以保持目录存活。
///
/// 执行过程：
///   1. `make_app(cx, factory)` → `MultisoulApp::new_with_pty_factory`，
///      初始 screen = Launchpad，runtime 已注入 echo 工厂。
///   2. `enter_canvas(app, cx, tmp.path())` → `app.enter_canvas_for_test(root, cx)`
///      先 `set_workspace_root(root)` 再置 `screen = Canvas`，runtime 不变（工厂仍在）。
///   3. `create_terminal(app, cx)` → `dispatch_action(NewTerminal)` →
///      `runtime.spawn_terminal` 走 echo backend，返回新 Term 节点 id。
///
/// 预期结果：
///   - 正断言：`screen() == Canvas`（enter_canvas 后切换成功）。
///   - 正断言：`runtime.live_len() == 1`（恰好 spawn 了 1 个 Term live node）。
///   - 正断言：`read_grid(node)` 返回 `Some`（Term 视图存在且可读）。
///   - 负断言：`screen() != Launchpad`（确认确实离开了 Launchpad）。
#[test]
fn harness_enters_canvas_and_spawns_terminal() {
    let mut cx = TestAppContext::single();

    let (app, cx) = make_app(&mut cx, echo_factory());
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());
    let node = create_terminal(&app, cx);
    cx.run_until_parked();

    let screen = app.update(cx, |a, _cx| a.screen());
    let live_len = read_runtime(&app, cx, |r| r.live_len());
    let grid = read_grid(&app, cx, node);

    assert_eq!(
        screen,
        app::AppScreen::Canvas,
        "enter_canvas 后屏幕应为 Canvas，实际为 {:?}",
        screen
    );
    assert_ne!(
        screen,
        app::AppScreen::Launchpad,
        "enter_canvas 后不应停留在 Launchpad"
    );
    assert_eq!(
        live_len, 1,
        "create_terminal 后应恰好有 1 个 live node，实际为 {}",
        live_len
    );
    assert!(
        grid.is_some(),
        "新建节点 {} 应有 Term 视图，但 read_grid 返回 None",
        node
    );

    // 清理：杀掉所有 live node 的 PTY（echo backend 无真实进程，但保持习惯）。
    app.update(cx, |a, _cx| a.runtime.kill_all());
}
