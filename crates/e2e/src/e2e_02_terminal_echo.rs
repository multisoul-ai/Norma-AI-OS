//! E2E-02：终端输入回显（L1 注入剧本后端）（SPEC §4.3）。
//!
//! 在 headless GPUI `TestAppContext` 中通过**剧本** PTY 工厂装配 `MultisoulApp`，
//! 进入 Canvas 后创建 1 个 Term 节点，向其 `send_node_input(Text("echo hi"))`，
//! 剧本后端匹配 trigger `"echo hi"` → 回吐 `"hi\r\n"`，验证 grid 含 `hi` 且不含 `bye`。
//!
//! 与 E2E-01 的差异：E2E-01 用 `echo_factory`（原样回显，只验证节点存在）；
//! 本测试用 `scripted_factory`（trigger→output 映射），覆盖 SPEC §4.3 的
//! “write 输入 → 后端按剧本回吐 → grid 含回吐内容”路径。
//!
//! L1 only：L2（真实 PTY 异步）为 Task 10，标 `#[ignore]`，本文件不涉及。

use std::time::Duration;

use gpui::TestAppContext;
use runtime::NodeInput;

use crate::harness::{
    create_terminal, drain_until, enter_canvas, make_app, read_grid, read_runtime, scripted_factory,
};

/// E2E-02：向 focused Term 节点写入 `echo hi` → 剧本后端回吐 `hi\r\n` → grid 含 `hi`。
///
/// 场景：用户在已 focus 的 Terminal 节点里输入命令，后端（L1 注入剧本）按规则
/// 回吐结果文本，验证 grid 渲染了回吐内容、且未渲染未发送的字串。
///
/// 数据构造（含关键数值推导）：
///   - `scripted_factory(rules)`：剧本 PTY 工厂，rules = `[("echo hi" → "hi\r\n")]`。
///     `TestPtyBackend::scripted` 在 `write(bytes)` 时遍历 rules，
///     若 `contains_subslice(bytes, trigger)` 为真则 `tx.send(Output(output))`。
///     - trigger = b"echo hi" = 7 字节
///     - output  = b"hi\r\n"  = 4 字节（'h','i','\r','\n'）
///   - 发送文本 `NodeInput::Text("echo hi".to_string())`：
///     `TermLive::input` → `session.write_bytes(b"echo hi")`（不追加 `\r`，
///     见 `crates/runtime/src/nodes/term.rs:119`）。`write_bytes` 经 pty.write 到 backend。
///     `contains_subslice(b"echo hi", b"echo hi")` → true（前 7 字节全等）→ emit(b"hi\r\n")。
///   - grid 推导：`grid.advance(b"hi\r\n")` → ANSI 解析器把 'h' 写入 (line0,col0)、
///     'i' 写入 (line0,col1)，`\r` 回到行首，`\n` 换行到 line1。
///     因此 line0 仍含 "hi"，line1 为空。拼接全 grid 文本必然 `contains("hi")`。
///   - cols=80, rows=24（runtime DEFAULT_SESSION_COLS/ROWS，见 E2E-01 / runtime 测试）。
///   - 负对照字串 "bye"：从未被任何 rule 的 output 包含，grid 中不应出现。
///   - drain timeout = 500ms：L1 剧本同步回吐，一轮 12ms drain 即收敛；
///     500ms 是安全冗余（与 E2E-01 的 200ms 同量级，留足 executor 推进余量）。
///
/// 执行过程：
///   1. `make_app(cx, scripted_factory(rules))` → boot headless GPUI 窗口，root view
///      focused + window activated；初始 screen = Launchpad，runtime 已注入剧本工厂。
///   2. `enter_canvas(app, cx, tmp.path())` → `enter_canvas_for_test` 直接置
///      `screen = Canvas` 并 `set_workspace_root(root)`，不替换 runtime，剧本工厂保留。
///   3. `create_terminal(app, cx)` → dispatch `NewTerminal`：runtime.spawn_terminal 走
///      注入工厂 → scripted backend → 返回 node_id；`activate_spawned_node` 把焦点
///      设为 `NodeFocused(node_id)`（E2E-01 已验证焦点路径，此处复用）。
///   4. `app.update(cx, |a, _| a.runtime.send_node_input(node, NodeInput::Text("echo hi".into())))`
///      → `TermLive::input` → `session.write_bytes(b"echo hi")` → `pty.write(b"echo hi")`
///      → scripted backend 匹配 trigger → `tx.send(Output(b"hi\r\n"))`（同步，rx 立即可取）。
///   5. `drain_until(app, cx, predicate, 500ms)`：每轮 `run_until_parked` 推进 executor
///      → 12ms 周期 drain timer → `session.drain()` → `rx.try_recv` 取 `Output(b"hi\r\n")`
///      → `grid.advance(b"hi\r\n")` → dirty。谓词读 `read_grid` 拼全 grid 文本，
///      `contains("hi")` 为真时返回 `Some(())`。
///   6. 拿到 grid 后做正/负断言：含 `hi`、不含 `bye`。
///
/// 预期结果：
///   - 正断言：`drain_until` 返回 `Some(())`（500ms 内 grid 出现 `hi`，未超时）。
///   - 正断言：grid 拼接文本 `contains("hi")`（剧本 output `hi\r\n` 落到 grid 首行）。
///   - 负断言：grid 拼接文本 `!contains("bye")`（从未发送、无 rule output 含 `bye`）。
///   - 负断言：grid 拼接文本 `!contains("echo hi")`（回吐的是 output 而非 trigger 文本；
///     剧本后端不会原样回显输入，区别于 echo backend）。
#[test]
fn e2e_02_terminal_input_echoes_scripted_output_into_grid() {
    let mut cx = TestAppContext::single();

    // 剧本规则：write 含 "echo hi" → 回吐 "hi\r\n"。
    // trigger/output 派生见函数级文档注释。
    let rules = vec![(b"echo hi".to_vec(), b"hi\r\n".to_vec())];
    let (app, cx) = make_app(&mut cx, scripted_factory(rules));
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());
    let node = create_terminal(&app, cx);

    // 写入输入：send_node_input(Text("echo hi")) → backend 匹配 trigger → emit("hi\r\n")。
    // 不追加 \r：trigger "echo hi" 作为子串已匹配 b"echo hi"。
    app.update(cx, |a, _cx| {
        a.runtime
            .send_node_input(node, NodeInput::Text("echo hi".to_string()))
            .expect("send_node_input(Text(\"echo hi\")) should write to scripted backend")
    });

    // drain_until：推进 12ms 周期 drain timer，直到 grid 含 "hi" 或 500ms 超时。
    // available drain budget = 500ms >> 12ms period → ~40 轮，足够 L1 同步回吐收敛。
    let satisfied = drain_until(
        &app,
        cx,
        |r| {
            let snap = match r.node_view(node) {
                Some(runtime::NodeView::Term { snapshot, .. }) => Some(snapshot),
                _ => None,
            };
            snap.and_then(|s| {
                let text: String = s.rows.iter().flatten().map(|cell| cell.c).collect();
                if text.contains("hi") {
                    Some(text)
                } else {
                    None
                }
            })
        },
        Duration::from_millis(500),
    )
    .expect("scripted backend should emit `hi\\r\\n` into grid within 500ms");

    // ---- PRIMARY 正断言：grid 含 "hi"（剧本 output 已落到 grid）----
    assert!(
        satisfied.contains("hi"),
        "grid 拼接文本应含剧本回吐的 'hi'，实际: {:?}",
        satisfied
    );

    // 复读一份 grid 做负断言（drain_until 返回的是谓词快照，这里再读一次确保一致）。
    let grid = read_grid(&app, cx, node).expect("read_grid 应返回 Some（节点存在且为 Term）");
    let text: String = grid.rows.iter().flatten().map(|cell| cell.c).collect();

    // ---- 负断言：grid 不含未发送字串 "bye" ----
    assert!(
        !text.contains("bye"),
        "grid 不应含未发送的字串 'bye'，实际: {:?}",
        text
    );

    // ---- 负断言：grid 不含 trigger 文本 "echo hi"（剧本不原样回显输入）----
    assert!(
        !text.contains("echo hi"),
        "grid 不应含 trigger 文本 'echo hi'（剧本后端只回吐 output，不回显输入），实际: {:?}",
        text
    );

    // 清理：杀掉所有 live node 的 PTY（剧本 backend 无真实进程，但保持习惯）。
    read_runtime(&app, cx, |_r| ()); // 触发一次稳定读，确保 drain 队列清空
    app.update(cx, |a, _cx| a.runtime.kill_all());
}
