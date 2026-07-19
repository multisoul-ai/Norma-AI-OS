//! E2E-02：生产同构 component Root 下，卡片 Tab 必须进入 PTY。

use std::sync::{Arc, Mutex};

use gpui::TestAppContext;
use runtime::PtyFactory;
use terminal::{PtyBackend, TestPtyBackend};

use crate::harness::{create_codex_agent, create_terminal, enter_canvas, make_app};

fn recording_factory(writes: Arc<Mutex<Vec<Vec<u8>>>>) -> PtyFactory {
    Box::new(move |_command, _cols, _rows, tx| {
        Ok(Box::new(TestPtyBackend::recording(tx, writes.clone())) as Box<dyn PtyBackend>)
    })
}

/// E2E-TAB-1：聚焦 Terminal 卡片后按物理 Tab，PTY 必须收到一个 HT 字节。
///
/// 数据构造（含关键数值的推导过程）：
///   physical key = "tab"                  （GPUI 命名键）
///   terminal encoding = HT = 0x09          （Tab 的标准控制字节）
///   expected PTY writes = [[0x09]]         （一次按键、一次单字节写入）
///   forbidden Enter byte = CR = 0x0d       （用于排除错误控制键映射）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app` 用生产同构的 `ui::component::Root` 装配应用，并注入 recording PTY。
///   2. 进入 Canvas、创建 Terminal；创建动作将 Runtime 切换为 `NodeFocused(terminal)`。
///   3. 通过 GPUI 发送真实 `tab` keystroke，覆盖 keymap → action/raw key → Runtime → PTY。
///   4. 从 recording PTY 读取实际写入，验证 Root 没有把 Tab 消费为焦点导航。
///
/// 预期结果：
///   - 正断言：写入中必须存在精确 `[0x09]`，证明 Tab 已进入 Terminal PTY。
///   - 负断言：写入中不得存在 `[0x0d]`，证明 Tab 未被错误编码为 Enter。
#[test]
fn terminal_card_tab_reaches_pty_through_component_root() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, recording_factory(writes.clone()));
    let workspace = tempfile::TempDir::new().expect("Terminal Tab e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _terminal_id = create_terminal(&app, cx);

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();

    let actual_writes = writes
        .lock()
        .expect("Terminal Tab PTY 写入记录锁不应中毒")
        .clone();
    assert!(
        actual_writes.contains(&vec![0x09]),
        "Terminal 聚焦时 Tab 必须向 PTY 写入精确 HT(0x09)，实际写入: {actual_writes:?}"
    );
    assert!(
        !actual_writes.contains(&vec![0x0d]),
        "Terminal 的 Tab 不得被错误编码为 Enter/CR(0x0d)，实际写入: {actual_writes:?}"
    );
}

/// E2E-TAB-2：聚焦 Agent 卡片后按物理 Tab，Agent PTY 必须收到一个 HT 字节。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent runtime = Codex                  （测试 PTY 接管进程，不依赖本机 Codex CLI）
///   physical key = "tab"                  （GPUI 命名键）
///   agent terminal encoding = HT = 0x09    （Agent 卡片复用终端输入协议）
///   expected PTY writes = [[0x09]]         （一次按键、一次单字节写入）
///   forbidden empty write count = 0        （Root 消费 Tab 时的回归表现）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app` 用生产同构的 `ui::component::Root` 装配应用，并注入 recording PTY。
///   2. 进入 Canvas、创建 Codex Agent；创建动作将 Runtime 切换为 `NodeFocused(agent)`。
///   3. 通过 GPUI 发送真实 `tab` keystroke，覆盖 Root keymap 与 Agent 共用的 PTY 输入链路。
///   4. 从 recording PTY 读取实际写入，验证 Agent 的 Tab 没有变成窗口焦点导航。
///
/// 预期结果：
///   - 正断言：写入中必须存在精确 `[0x09]`，证明 Tab 已进入 Agent PTY。
///   - 负断言：写入记录不得为空，防止恢复为 Root 静默消费 Tab 的回归状态。
#[test]
fn agent_card_tab_reaches_pty_through_component_root() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, recording_factory(writes.clone()));
    let workspace = tempfile::TempDir::new().expect("Agent Tab e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _agent_id = create_codex_agent(&app, cx);

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();

    let actual_writes = writes
        .lock()
        .expect("Agent Tab PTY 写入记录锁不应中毒")
        .clone();
    assert!(
        actual_writes.contains(&vec![0x09]),
        "Agent 聚焦时 Tab 必须向 PTY 写入精确 HT(0x09)，实际写入: {actual_writes:?}"
    );
    assert!(
        !actual_writes.is_empty(),
        "Agent 聚焦时 Tab 写入不得为空；空记录表示 component Root 再次消费了按键"
    );
}
