//! E2E-02：Agent 卡片的多行文本输入必须遵循当前终端应用声明的 paste mode。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{ClipboardItem, TestAppContext};
use runtime::{NodeInput, PtyFactory};
use terminal::{PtyBackend, PtyEvent, TestPtyBackend};

use crate::harness::{
    create_claude_agent, create_codex_agent, create_terminal, drain_until, echo_factory,
    enter_canvas, make_app, read_grid,
};

/// E2E-02：Codex Agent 卡片收到含 LF 的文本粘贴后，第二行必须在第 0 列完整显示。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex Agent cards = 1（经真实 NewCodexAgent action 创建）
///   input = "first\\nsecond" = 5 + 1 + 6 = 12 bytes
///   logical lines = 2；canonical terminal separator = CRLF = 2 control bytes
///   visible rows = 2：row0 前 6 cells = "first "，row1 前 6 cells = "second"
///   drain timeout = 500ms；L1 echo 同步回吐，500ms / 16ms runtime tick ≈ 31 帧，
///                   大于创建卡片、写 PTY 与一次 frame drain 所需的帧数。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 装配带 echo PTY 的 headless GPUI App，并进入临时 Canvas workspace。
///   2. 通过公开 `NewCodexAgent` action 创建 Agent 卡片，使其复用生产 TermLive 输入路径。
///   3. 向该 Agent 节点发送 `NodeInput::Paste("first\\nsecond")`。
///   4. echo PTY 回灌实际写入的 bytes；App frame loop drain 后读取 Agent 的 GridSnapshot。
///   5. 检查两行前六个 cells，验证换行不会只下移光标而让第二行从第 5 列开始。
///
/// 预期结果：
///   - 断言 A：第一行精确显示 "first "，证明多行输入首行未丢失。
///   - 断言 B：第二行精确显示 "second"，证明 Agent 卡片每行从左边界开始。
///   - 断言 C：第二行首格不是空格，且不应回归为 "     s" 的裸 LF 横向错位。
#[test]
fn e2e_02_codex_agent_multiline_input_restarts_each_line_at_left_edge() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent 多行输入 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);

    app.update(cx, |app, _cx| {
        app.runtime
            .send_node_input(node_id, NodeInput::Paste("first\nsecond".to_string()))
            .expect("Agent 卡片应接受含 LF 的多行 NodeInput::Paste")
    });

    let rows = drain_until(
        &app,
        cx,
        |runtime| match runtime.node_view(node_id) {
            Some(runtime::NodeView::Term { snapshot, .. }) => {
                let first_row = snapshot.rows[0]
                    .iter()
                    .take(6)
                    .map(|cell| cell.c)
                    .collect::<String>();
                let second_row = snapshot.rows[1]
                    .iter()
                    .take(6)
                    .map(|cell| cell.c)
                    .collect::<String>();
                (first_row == "first " && second_row == "second").then_some((first_row, second_row))
            }
            _ => None,
        },
        Duration::from_millis(500),
    )
    .expect("500ms 内 Agent 多行输入应经 PTY 与 frame drain 显示为两行");

    assert_eq!(
        rows.0, "first ",
        "Agent 卡片首行应保留 first，实际前六格为 {:?}",
        rows.0
    );
    assert_eq!(
        rows.1, "second",
        "Agent 卡片第二行必须从第 0 列显示 second，实际前六格为 {:?}",
        rows.1
    );

    let snapshot =
        read_grid(&app, cx, node_id).expect("Agent 节点应保留可读取的 terminal GridSnapshot");
    assert_ne!(
        snapshot.rows[1][0].c, ' ',
        "第二行第 0 列必须是 s，不能因裸 LF 继承上一行列位置而留空"
    );
    let second_row: String = snapshot.rows[1].iter().take(6).map(|cell| cell.c).collect();
    assert_ne!(
        second_row, "     s",
        "第二行不得回归为裸 LF 的五列缩进，实际前六格为 {second_row:?}"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// E2E-02：用户向模拟 Codex TUI 的终端按 Cmd+V 时，只能产生一个 bracketed paste。
///
/// 数据构造（含关键数值的推导过程）：
///   Terminal cards = 1（经公开 NewTerminal action 创建并聚焦；PTY 输出模拟 Codex）
///   terminal mode = ESC[?2004h = 8 bytes（模拟 Codex TUI 启用 bracketed paste）
///   clipboard    = "first\\nsecond" = 5 + 1 + 6 = 12 bytes
///   protocol     = ESC[200~(6) + clipboard(12) + ESC[201~(6) = 24 bytes
///   expected PTY writes = 1（一次 Cmd+V 对应一个原子 paste 事件）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 装配 recording PTY；Agent 启动时从输出侧发送 ESC[?2004h。
///   2. 进入 Canvas，通过公开 action 创建并聚焦 Terminal 卡片。
///   3. 用 GPUI clipboard 写入两行文本，再模拟真实 `cmd-v`。
///   4. `on_key_down` 读取剪贴板，经 Runtime/TermLive 写到 recording PTY。
///
/// 预期结果：
///   - 正断言：PTY 只收到一次写入，内容精确为 bracketed-paste 协议包。
///   - 正断言：begin/end 之间保留完整的 first 与 second 两行。
///   - 负断言：不得出现裸 CRLF；Codex 在用户按 Enter 前不能收到提交键。
#[test]
fn e2e_02_cmd_v_keeps_multiline_text_in_one_bracketed_paste() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let recorded_writes = writes.clone();
    let factory: PtyFactory = Box::new(move |_command, _cols, _rows, tx| {
        tx.send(PtyEvent::Output(b"\x1b[?2004h".to_vec()))
            .expect("Codex 测试 PTY 应能声明 bracketed-paste mode");
        Ok(Box::new(TestPtyBackend::recording(tx, recorded_writes.clone())) as Box<dyn PtyBackend>)
    });
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, factory);
    let workspace = tempfile::tempdir().expect("bracketed Cmd+V e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _node_id = create_terminal(&app, cx);
    app.update(cx, |app, _cx| {
        app.runtime.drain_all();
    });

    cx.update(|_window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("first\nsecond".to_string()));
    });
    cx.simulate_keystrokes("cmd-v");

    let recorded = writes
        .lock()
        .expect("bracketed Cmd+V 写入记录锁不应被污染")
        .clone();
    assert_eq!(recorded.len(), 1, "一次 Cmd+V 必须只产生一次 PTY write");
    assert_eq!(
        recorded[0], b"\x1b[200~first\nsecond\x1b[201~",
        "Codex 多行剪贴板内容必须作为一个 bracketed paste 写入，实际为 {:?}",
        recorded[0]
    );
    assert_ne!(
        recorded[0], b"first\r\nsecond",
        "Cmd+V 不得把剪贴板换行转换成 Codex 可执行的裸 Enter"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// E2E-02：Codex 请求增强键盘协议后，Option+Enter 必须作为换行键发送且不能退化为提交。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex Agent cards = 1（经真实 NewCodexAgent action 创建并聚焦）
///   keyboard mode request = CSI > 7 u = ESC(1) + "[>7u"(4) = 5 bytes
///   Option/Alt modifier value = 1(default) + 2(alt bit) = 3
///   expected Option+Enter = CSI 13;3u = ESC(1) + "[13;3u"(6) = 7 bytes
///   forbidden plain Enter = CR = [0x0d] = 1 byte
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 装配 recording PTY，并从输出侧发送 CSI > 7 u，模拟 Codex 请求增强键盘协议。
///   2. 通过公开 NewCodexAgent action 创建并聚焦 Agent 卡片，再 drain 协议声明。
///   3. 通过 GPUI `simulate_keystrokes("alt-enter")` 模拟 macOS Option+Enter，走真实 root
///      keydown、Runtime 和 PTY 路径。
///   4. 读取 recording PTY 的实际写入，区分增强 Option+Enter 与裸 Enter 提交。
///
/// 预期结果：
///   - 正断言：PTY 精确收到 CSI 13;3u，Codex 可识别为 Option+Enter 并插入换行。
///   - 负断言：PTY 不得收到裸 CR，否则 Codex 会把它当成 Enter 并直接发送消息。
#[test]
fn e2e_02_option_enter_in_codex_agent_inserts_newline_instead_of_submitting() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let recorded_writes = writes.clone();
    let factory: PtyFactory = Box::new(move |_command, _cols, _rows, tx| {
        tx.send(PtyEvent::Output(b"\x1b[>7u".to_vec()))
            .expect("Codex 测试 PTY 应能请求增强键盘协议");
        Ok(Box::new(TestPtyBackend::recording(tx, recorded_writes.clone())) as Box<dyn PtyBackend>)
    });
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, factory);
    let workspace = tempfile::tempdir().expect("Option+Enter e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _node_id = create_codex_agent(&app, cx);
    app.update(cx, |app, _cx| {
        app.runtime.drain_all();
    });

    cx.simulate_keystrokes("alt-enter");

    let recorded = writes
        .lock()
        .expect("Option+Enter 写入记录锁不应被污染")
        .clone();
    assert_eq!(
        recorded,
        vec![b"\x1b[13;3u".to_vec()],
        "增强键盘模式下 Option+Enter 应精确编码为 CSI 13;3u，实际写入: {recorded:?}"
    );
    assert_ne!(
        recorded,
        vec![vec![0x0d]],
        "Option+Enter 不得退化为裸 CR，否则 Codex 会直接提交消息"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// E2E-02：Codex 请求增强键盘协议后，Shift+Enter 必须作为换行键发送且不能退化为提交。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex Agent cards = 1（经真实 NewCodexAgent action 创建并聚焦）
///   keyboard mode request = CSI > 7 u = ESC(1) + "[>7u"(4) = 5 bytes
///   Shift modifier value = 1(default) + 1(shift bit) = 2
///   expected Shift+Enter = CSI 13;2u = ESC(1) + "[13;2u"(6) = 7 bytes
///   forbidden plain Enter = CR = [0x0d] = 1 byte
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 装配 recording PTY，并从输出侧发送 CSI > 7 u，模拟 Codex 请求增强键盘协议。
///   2. 通过公开 NewCodexAgent action 创建并聚焦 Agent 卡片，再 drain 协议声明。
///   3. 通过 GPUI `simulate_keystrokes("shift-enter")` 模拟 Shift+Enter，走真实 root
///      keydown、Runtime 和 PTY 路径。
///   4. 读取 recording PTY 的实际写入，区分增强 Shift+Enter 与裸 Enter 提交。
///
/// 预期结果：
///   - 正断言：PTY 精确收到 CSI 13;2u，Codex 可识别为 Shift+Enter 并插入换行。
///   - 负断言：PTY 不得收到裸 CR，否则 Codex 会把它当成 Enter 并直接发送消息。
#[test]
fn e2e_02_shift_enter_in_codex_agent_inserts_newline_instead_of_submitting() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let recorded_writes = writes.clone();
    let factory: PtyFactory = Box::new(move |_command, _cols, _rows, tx| {
        tx.send(PtyEvent::Output(b"\x1b[>7u".to_vec()))
            .expect("Codex Shift+Enter 测试 PTY 应能请求增强键盘协议");
        Ok(Box::new(TestPtyBackend::recording(tx, recorded_writes.clone())) as Box<dyn PtyBackend>)
    });
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, factory);
    let workspace = tempfile::tempdir().expect("Shift+Enter e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _node_id = create_codex_agent(&app, cx);
    app.update(cx, |app, _cx| {
        app.runtime.drain_all();
    });

    cx.simulate_keystrokes("shift-enter");

    let recorded = writes
        .lock()
        .expect("Shift+Enter 写入记录锁不应被污染")
        .clone();
    assert_eq!(
        recorded,
        vec![b"\x1b[13;2u".to_vec()],
        "增强键盘模式下 Shift+Enter 应精确编码为 CSI 13;2u，实际写入: {recorded:?}"
    );
    assert_ne!(
        recorded,
        vec![vec![0x0d]],
        "Shift+Enter 不得退化为裸 CR，否则 Codex 会直接提交消息"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// E2E-02：Terminal 卡片内的 TUI 请求增强键盘协议后，Shift+Enter 必须保留换行语义。
///
/// 数据构造（含关键数值的推导过程）：
///   Terminal cards = 1（经真实 NewTerminal action 创建并聚焦）
///   keyboard mode request = CSI > 7 u = ESC(1) + "[>7u"(4) = 5 bytes
///   Shift modifier value = 1(default) + 1(shift bit) = 2
///   expected Shift+Enter = CSI 13;2u = ESC(1) + "[13;2u"(6) = 7 bytes
///   forbidden plain Enter = CR = [0x0d] = 1 byte
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 装配 recording PTY，并从输出侧发送 CSI > 7 u，模拟 Terminal 中的 TUI 请求增强协议。
///   2. 通过公开 NewTerminal action 创建并聚焦 Terminal 卡片，再 drain 协议声明。
///   3. 通过 GPUI `simulate_keystrokes("shift-enter")` 模拟 Shift+Enter，覆盖 root keydown、
///      Runtime 和 Terminal PTY 的完整输入链路。
///   4. 读取 recording PTY 的实际写入，验证 Terminal 卡片没有丢失 Shift 修饰键。
///
/// 预期结果：
///   - 正断言：PTY 精确收到 CSI 13;2u，卡片内 TUI 可将其解释为插入换行。
///   - 负断言：PTY 不得收到裸 CR，避免 Shift+Enter 被错误解释为直接提交。
#[test]
fn e2e_02_shift_enter_in_terminal_card_preserves_newline_modifier() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let recorded_writes = writes.clone();
    let factory: PtyFactory = Box::new(move |_command, _cols, _rows, tx| {
        tx.send(PtyEvent::Output(b"\x1b[>7u".to_vec()))
            .expect("Terminal Shift+Enter 测试 PTY 应能请求增强键盘协议");
        Ok(Box::new(TestPtyBackend::recording(tx, recorded_writes.clone())) as Box<dyn PtyBackend>)
    });
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, factory);
    let workspace = tempfile::tempdir().expect("Terminal Shift+Enter e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _node_id = create_terminal(&app, cx);
    app.update(cx, |app, _cx| {
        app.runtime.drain_all();
    });

    cx.simulate_keystrokes("shift-enter");

    let recorded = writes
        .lock()
        .expect("Terminal Shift+Enter 写入记录锁不应被污染")
        .clone();
    assert_eq!(
        recorded,
        vec![b"\x1b[13;2u".to_vec()],
        "Terminal 增强键盘模式下 Shift+Enter 应精确编码为 CSI 13;2u，实际写入: {recorded:?}"
    );
    assert_ne!(
        recorded,
        vec![vec![0x0d]],
        "Terminal 的 Shift+Enter 不得退化为裸 CR，否则卡片内 TUI 会直接提交消息"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// E2E-02：Claude、Codex 和 Terminal 在仅启用 bracketed paste 时统一 Shift+Enter 换行。
///
/// 数据构造（含关键数值推导）：
///   三种卡片分别使用 recording PTY，仅声明 ESC[?2004h，不请求 Kitty。
///   每张卡片两次按键：Shift+Enter + Enter，共 3 × 2 = 6 次 PTY write。
///   换行包 = begin(6) + LF(1) + end(6) = 13 bytes；提交 = CR(1)。
/// 执行过程：
///   1. 通过公开 action 逐一创建并聚焦 Claude、Codex、Terminal 卡片。
///   2. drain 模式声明，通过 GPUI 模拟 Shift+Enter 后再模拟普通 Enter。
///   3. 检查每张卡片经共享 Runtime/TerminalSession 写出的字节。
///
/// 预期结果：
///   - 三种卡片都先收到粘贴换行，再收到 CR 提交。
///   - Shift+Enter 不得退化为 CR，也不得向未声明 Kitty 的程序发送 CSI-u。
#[test]
fn e2e_02_shift_enter_without_kitty_is_consistent_across_cards() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let recorded_writes = writes.clone();
    let factory: PtyFactory = Box::new(move |_command, _cols, _rows, tx| {
        tx.send(PtyEvent::Output(b"\x1b[?2004h".to_vec()))
            .expect("测试 PTY 应声明 bracketed paste");
        Ok(Box::new(TestPtyBackend::recording(tx, recorded_writes.clone())) as Box<dyn PtyBackend>)
    });
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, factory);
    let workspace = tempfile::tempdir().expect("应创建隔离 workspace");
    enter_canvas(&app, cx, workspace.path());
    for create in [create_claude_agent, create_codex_agent, create_terminal] {
        create(&app, cx);
        app.update(cx, |app, _cx| {
            app.runtime.drain_all();
        });
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_keystrokes("enter");
    }
    let recorded = writes.lock().expect("写入记录锁应可用").clone();
    assert_eq!(recorded.len(), 6, "每张卡片应产生两次独立写入");
    assert_eq!(recorded[0], b"\x1b[200~\n\x1b[201~", "Claude 应插入换行");
    assert_ne!(recorded[0], b"\r", "Claude Shift+Enter 不得提交");
    assert_eq!(recorded[1], b"\r", "Claude Enter 应提交");
    assert_eq!(recorded[2], b"\x1b[200~\n\x1b[201~", "Codex 应插入换行");
    assert_ne!(recorded[2], b"\x1b[13;2u", "未启用 Kitty 时不得强发 CSI-u");
    assert_eq!(recorded[3], b"\r", "Codex Enter 应提交");
    assert_eq!(recorded[4], b"\x1b[200~\n\x1b[201~", "Terminal 应插入换行");
    assert_ne!(recorded[4], b"\r", "Terminal Shift+Enter 不得提交");
    assert_eq!(recorded[5], b"\r", "Terminal Enter 应提交");
    app.update(cx, |app, _cx| app.runtime.kill_all());
}
