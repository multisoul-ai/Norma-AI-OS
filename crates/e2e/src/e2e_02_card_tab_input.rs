//! E2E-02：生产同构 component Root 下，终端普通输入、autosuggestion 与 Tab 必须保持各自语义。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::TestAppContext;
use runtime::PtyFactory;
use terminal::{PtyBackend, PtyEvent, TerminalColor, TerminalNamedColor, TestPtyBackend};

use crate::harness::{
    create_codex_agent, create_terminal, drain_until, enter_canvas, make_app, read_grid,
    scripted_factory,
};

fn recording_factory(writes: Arc<Mutex<Vec<Vec<u8>>>>) -> PtyFactory {
    Box::new(move |_command, _cols, _rows, tx| {
        Ok(Box::new(TestPtyBackend::recording(tx, writes.clone())) as Box<dyn PtyBackend>)
    })
}

fn bracketed_recording_factory(writes: Arc<Mutex<Vec<Vec<u8>>>>) -> PtyFactory {
    Box::new(move |_command, _cols, _rows, tx| {
        tx.send(PtyEvent::Output(b"\x1b[?2004h".to_vec()))
            .expect("autosuggestion 测试 PTY 应能声明 bracketed-paste mode");
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
    assert_eq!(
        actual_writes,
        vec![vec![0x09]],
        "Terminal 聚焦时一次 Tab 必须只向 PTY 写入一次 HT(0x09)，实际写入: {actual_writes:?}"
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
    assert_eq!(
        actual_writes,
        vec![vec![0x09]],
        "Agent 聚焦时一次 Tab 必须只向 PTY 写入一次 HT(0x09)，实际写入: {actual_writes:?}"
    );
    assert!(
        !actual_writes.is_empty(),
        "Agent 聚焦时 Tab 写入不得为空；空记录表示 component Root 再次消费了按键"
    );
}

/// E2E-KEY-1：Codex Agent 的 Option+↑ 必须作为带修饰的方向键到达 PTY。
///
/// 数据构造（含关键数值的推导过程）：
///   xterm 方向键前缀 = ESC[1;；Option 修饰值 = 1(default) + 2(alt) = 3。
///   Up 终止字节 = A，所以期望序列为 ESC[1;3A（6 字节）。
///   普通 Up = ESC[A（3 字节），Codex 不会把它识别为“回答问题”。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 recording PTY 创建并聚焦 Codex Agent 卡片。
///   2. 模拟真实的 `alt-up` 按键，经过 Root、Runtime 和 PTY。
///   3. 读取 PTY 收到的完整写入。
///
/// 预期结果：
///   - 正断言：唯一写入是 ESC[1;3A，Codex 能识别 Option+↑。
///   - 负断言：不能退化为普通 Up 的 ESC[A。
#[test]
fn codex_agent_option_up_reaches_pty_with_modifier() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, recording_factory(writes.clone()));
    let workspace = tempfile::TempDir::new().expect("Option+↑ e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _agent_id = create_codex_agent(&app, cx);

    cx.simulate_keystrokes("alt-up");
    cx.run_until_parked();

    let actual_writes = writes.lock().expect("Agent PTY 写入记录锁不应中毒").clone();
    assert_eq!(
        actual_writes,
        vec![b"\x1b[1;3A".to_vec()],
        "Codex Agent 的 Option+↑ 应完整到达 PTY，实际写入: {actual_writes:?}"
    );
    assert_ne!(
        actual_writes,
        vec![b"\x1b[A".to_vec()],
        "Option+↑ 不能被降级为普通 ↑"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// E2E-AUTO-1：仅输入 `g`、尚未按 Tab 时，光标就应与灰色 autosuggestion 首字符占据同一 cell。
///
/// 数据构造（含关键数值的推导过程）：
///   physical input = `g` = 1 个普通字符；Tab 输入次数 = 0
///   scripted shell output = `g` + SGR 90 + `exit` + SGR reset + CSI 4D
///   visible cells = `gexit` = 5 cells
///   cursor after output = col 5；CSI 4D 左移 4 格 → cursor_col = 1
///   suggestion first cell = row 0, col 1 = 'e'，fg = BrightBlack，bg = Background
///   drain timeout = 500ms；生产 drain 周期约 12ms，预算约 500 / 12 ≈ 41 轮
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用生产同构 component Root 创建并聚焦 Terminal 卡片。
///   2. GPUI 只发送物理 `g`；测试 shell 立即回吐灰色 `exit` 建议，全程不发送 Tab。
///   3. ANSI parser 写入 `gexit` 并把 cursor 从 col 5 左移到 col 1。
///   4. App frame loop drain PTY 输出，E2E 从 Runtime 公开 NodeView 读取最终 grid。
///
/// 预期结果：
///   - 正断言：grid 精确包含 `gexit`，cursor 精确落在灰色首字符 'e' 上。
///   - 正断言：'e' 保留 BrightBlack/Background 样式，供 UI 触发 cursor 字符反色重绘。
///   - 负断言：'e' 不得被解析为 inverse 或普通 Foreground；建议出现不依赖 Tab。
#[test]
fn terminal_g_shows_gray_autosuggestion_under_cursor_before_tab() {
    let rules = vec![(b"g".to_vec(), b"g\x1b[90mexit\x1b[39m\x1b[4D".to_vec())];
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, scripted_factory(rules));
    let workspace = tempfile::TempDir::new().expect("g autosuggestion e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let terminal_id = create_terminal(&app, cx);

    cx.simulate_keystrokes("g");

    // drain budget = 500ms / 约 12ms 每轮 ≈ 41 轮，足够 L1 同步剧本输出稳定落入 grid。
    drain_until(
        &app,
        cx,
        |runtime| match runtime.node_view(terminal_id) {
            Some(runtime::NodeView::Term { snapshot, .. })
                if snapshot.cursor_line == 0
                    && snapshot.cursor_col == 1
                    && snapshot
                        .rows
                        .first()
                        .and_then(|row| row.get(1))
                        .is_some_and(|cell| cell.c == 'e') =>
            {
                Some(())
            }
            _ => None,
        },
        Duration::from_millis(500),
    )
    .expect("仅输入 g 后，灰色建议首字符应在 500ms 内与 cursor 收敛到同一 cell");

    let grid = read_grid(&app, cx, terminal_id)
        .expect("仅输入 g 后 Terminal 节点必须保留可读取的 GridSnapshot");
    let visible: String = grid.rows[0].iter().take(5).map(|cell| cell.c).collect();
    let suggestion_first = &grid.rows[0][1];

    assert_eq!(
        visible, "gexit",
        "测试 shell 应在首行生成精确的 `gexit` 建议场景"
    );
    assert_eq!(
        (grid.cursor_line, grid.cursor_col),
        (0, 1),
        "cursor 必须精确落在 row 0, col 1 的建议首字符上"
    );
    assert_eq!(
        suggestion_first.c, 'e',
        "cursor cell 的可见字符必须是 autosuggestion 首字符 'e'"
    );
    assert_eq!(
        suggestion_first.fg,
        TerminalColor::Named(TerminalNamedColor::BrightBlack),
        "autosuggestion 首字符必须保留 ANSI BrightBlack 灰色前景"
    );
    assert_eq!(
        suggestion_first.bg,
        TerminalColor::Named(TerminalNamedColor::Background),
        "autosuggestion 首字符必须保留默认 Background，供 cursor 反色字符使用"
    );
    assert!(
        !suggestion_first.attrs.inverse,
        "autosuggestion 首字符本身不应携带 inverse；反色应由 cursor 绘制语义完成"
    );
    assert_ne!(
        suggestion_first.fg,
        TerminalColor::Named(TerminalNamedColor::Foreground),
        "autosuggestion 首字符不得退化成普通 Foreground，否则未覆盖灰色建议场景"
    );
}

/// E2E-AUTO-2：终端启用 bracketed paste 后，普通字符输入仍必须作为裸 UTF-8 到达 PTY。
///
/// 数据构造（含关键数值的推导过程）：
///   terminal mode = ESC[?2004h = 8 bytes（模拟 zsh 启用 bracketed paste）
///   physical input = `c` + `a` + `r` = 3 个普通字符 = 3 bytes
///   expected combined bytes = "car" = 3 bytes
///   forbidden single paste = ESC[200~(6) + "car"(3) + ESC[201~(6) = 15 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 装配声明 bracketed-paste mode 的 recording PTY，进入 Canvas 并聚焦 Terminal。
///   2. drain mode enable 输出，使 terminal grid 进入与真实 zsh 相同的 paste mode。
///   3. 通过 GPUI 依次模拟 `c a r`，覆盖 keydown → IME commit → Runtime → PTY 公开链路。
///   4. 合并 PTY 的全部写入，验证普通输入没有被错误包装为 Cmd+V 粘贴。
///
/// 预期结果：
///   - 正断言：合并后的写入精确为裸 `car`，zsh 可逐次执行 self-insert 并刷新建议。
///   - 正断言：PTY 至少收到一次写入，证明字符没有被 Root 或 IME 吞掉。
///   - 负断言：任何一次写入都不得含 bracketed-paste begin/end 标记。
#[test]
fn terminal_plain_text_bypasses_bracketed_paste_through_component_root() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, bracketed_recording_factory(writes.clone()));
    let workspace = tempfile::TempDir::new().expect("普通输入协议 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _terminal_id = create_terminal(&app, cx);
    app.update(cx, |app, _cx| {
        assert!(
            app.runtime.drain_all(),
            "测试终端的 bracketed-paste enable 输出必须在输入前被 drain"
        );
    });

    cx.simulate_keystrokes("c a r");
    cx.run_until_parked();

    let actual_writes = writes
        .lock()
        .expect("普通输入 PTY 写入记录锁不应中毒")
        .clone();
    let combined = actual_writes.concat();
    assert_eq!(
        combined, b"car",
        "普通 c、a、r 必须以裸 UTF-8 到达 PTY，实际写入为 {actual_writes:?}"
    );
    assert!(
        !actual_writes.is_empty(),
        "普通字符输入必须产生 PTY 写入，空记录表示 Root 或 IME 吞掉了文本"
    );
    assert!(
        actual_writes.iter().all(|write| {
            !write.windows(6).any(|bytes| bytes == b"\x1b[200~")
                && !write.windows(6).any(|bytes| bytes == b"\x1b[201~")
        }),
        "普通字符输入不得携带 bracketed-paste begin/end，实际写入为 {actual_writes:?}"
    );
}
