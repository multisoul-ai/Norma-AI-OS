//! E2E-02：Terminal 卡片对 DSR 光标位置查询的 PTY 回写。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::TestAppContext;
use runtime::PtyFactory;
use terminal::{PtyBackend, PtyEvent, TestPtyBackend};

use crate::harness::{create_terminal, drain_until, enter_canvas, make_app, read_grid};

/// E2E-02：子进程输出 `abc` 后查询 `CSI 6 n`，Terminal 卡片必须向相同 PTY 回写 `CSI 1;4 R`。
///
/// 场景：Grok 这类全屏 TUI 向终端发出 Device Status Report 光标查询时，Terminal
/// 节点在下一次 drain 中识别查询、按当前画布网格光标位置形成响应并写回 PTY；TUI
/// 因而不会在等待光标坐标时超时。
///
/// 数据构造（含关键数值的推导过程）：
///   - 测试 PTY 为 recording backend：它记录 `session.drain()` 写回的每段字节，
///     不把回写再伪装成子进程输出，避免回环掩盖方向错误。
///   - 子进程输出 = b"abc\\x1b[6n" = 3 个可见 ASCII 字节 + DSR 查询 4 字节 = 7 字节。
///     "abc" 后 grid cursor 为 0-based (line=0, col=3)。
///   - 终端协议采用 1-based 坐标：row = 0 + 1 = 1，column = 3 + 1 = 4，
///     所以期望 PTY 回写 = b"\\x1b[1;4R"（CSI 1;4 R，6 字节）。
///   - 错误原点对照 = b"\\x1b[1;1R"；若收到它，说明实现没有读取 abc 后的实际光标。
///   - drain timeout = 500ms；L1 backend 的 `tx.send(Output(...))` 同步完成，
///     app drain 周期约 12ms，500 / 12 ≈ 41 轮，为调度留出充足余量。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app` 以 recording PTY factory 装配 headless Terminal 卡片；factory 保存
///      子进程输出 sender，并把 backend 的写入记录留给测试观察。
///   2. `enter_canvas` 与 `create_terminal` 建立真实 Runtime → TermLive → TerminalSession
///      链路，获得该节点注入 PTY 的 `PtyEvent` sender。
///   3. sender 投递 `Output(b"abc\\x1b[6n")`，模拟 Grok 从 slave PTY 写出可见文本和
///      DSR 查询；没有通过用户输入路径伪造这段终端输出。
///   4. `drain_until` 每轮推进 12ms：session 把输出喂给 TerminalGrid；parser 将 DSR
///      转为 PTY 回写事件；session 把 `ESC[1;4R` 写入 recording backend。
///   5. 读取记录的 PTY 写入及 grid snapshot，验证回写字节方向、坐标与可见文本。
///
/// 预期结果：
///   - 断言 A：500ms 内 recording backend 收到精确 `ESC[1;4R`，证明卡片完整回写协议。
///   - 断言 B：记录不含 `ESC[1;1R`，证明回写并非错误的固定原点。
///   - 断言 C：grid 仍含 abc，证明查询前的可见输出正常渲染。
#[test]
fn e2e_02_terminal_card_replies_to_dsr_cursor_position_query() {
    let mut test_cx = TestAppContext::single();
    let recorded_writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let output_sender = Arc::new(Mutex::new(None));
    let factory: PtyFactory = Box::new({
        let recorded_writes = recorded_writes.clone();
        let output_sender = output_sender.clone();
        move |_command, _cols, _rows, tx| {
            *output_sender
                .lock()
                .expect("DSR test output sender mutex should not be poisoned") = Some(tx.clone());
            Ok(
                Box::new(TestPtyBackend::recording(tx, recorded_writes.clone()))
                    as Box<dyn PtyBackend>,
            )
        }
    });
    let (app, cx) = make_app(&mut test_cx, factory);
    let workspace = tempfile::TempDir::new().expect("DSR e2e workspace tempdir should create");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_terminal(&app, cx);

    let pty_output = output_sender
        .lock()
        .expect("DSR test output sender mutex should not be poisoned")
        .clone()
        .expect("creating the Terminal card should expose its injected PTY output sender");
    pty_output
        .send(PtyEvent::Output(b"abc\x1b[6n".to_vec()))
        .expect("injecting child DSR output into the Terminal card should succeed");

    let expected_reply = b"\x1b[1;4R".to_vec();
    let actual_writes = drain_until(
        &app,
        cx,
        |_| {
            let writes = recorded_writes
                .lock()
                .expect("DSR recorded writes mutex should not be poisoned")
                .clone();
            writes.contains(&expected_reply).then_some(writes)
        },
        Duration::from_millis(500),
    )
    .expect("Terminal card should write DSR cursor response to PTY within 500ms");

    assert!(
        actual_writes.contains(&expected_reply),
        "PTY 写入应包含精确 DSR 响应 ESC[1;4R，实际: {:?}",
        actual_writes
    );
    assert!(
        !actual_writes.contains(&b"\x1b[1;1R".to_vec()),
        "PTY 写入不应包含错误原点响应 ESC[1;1R，实际: {:?}",
        actual_writes
    );

    let grid = read_grid(&app, cx, node_id).expect("DSR Terminal 节点应保留可读取的 grid");
    let text: String = grid.rows.iter().flatten().map(|cell| cell.c).collect();
    assert!(
        text.contains("abc"),
        "DSR 查询前的可见文本 abc 应保留在 grid，实际: {:?}",
        text
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
