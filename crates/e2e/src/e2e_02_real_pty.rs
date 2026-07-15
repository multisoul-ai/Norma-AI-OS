//! E2E-02-real：真 PTY 集成冒烟（L2，SPEC §3.4，`#[ignore]`）。
//!
//! 与 E2E-02 同形，但 PTY 后端换为真实 `portable_pty`：经 `make_app_real` 装配
//! `MultisoulApp::new(cx)`（不注入工厂）→ `Runtime.pty_factory = None` →
//! `spawn_term_live(None)` → `TermLive::spawn_command` → 真实 `/bin/zsh`。
//!
//! 真 PTY 输出异步到达，必须用 `drain_until(predicate, timeout)` 轮询（仅
//! `run_until_parked` 不够：mock 时钟下 drain timer 需 `advance_clock` 推进）。
//! `#[ignore]` 标记：CI 默认跳过，本地 `cargo test -p e2e -- --ignored` 跑。
//!
//! 与 E2E-02 的差异（详见任务简报 §10 与函数级注释）：
//!   - 工厂：真实（默认 `portable_pty`）而非 `scripted_factory`。
//!   - shell：`on_new_terminal` 硬编码 `/bin/zsh`（无公开 API 覆盖），
//!     故 SPEC §3.4 建议的 `/bin/sh` 不可行；改用 `/bin/zsh` 并依赖
//!     `contains("hi")` 断言对 prompt/rc 噪声鲁棒。
//!   - 输入：`NodeInput::Text("echo hi\r".into())`，`\r` 触发 shell 执行
//!     （`write_bytes` 不追加换行；`TermLive::input` 见 `crates/runtime/src/nodes/term.rs:119`）。

use std::time::Duration;

use gpui::TestAppContext;
use runtime::NodeInput;

use crate::harness::{create_terminal, drain_until, enter_canvas, make_app_real, read_grid};

/// E2E-02-real：真 PTY 链路下 `echo hi` → grid 含 `hi`、不含 `bye`。
///
/// **真 PTY 集成测试，CI 默认跳过，本地 `cargo test -p e2e -- --ignored` 跑。**
///
/// 场景：用户在 focused Terminal 节点输入 `echo hi` + 回车，真实 `/bin/zsh`
/// 经 `portable_pty` 子进程执行命令并把 `hi\r\n` 回写到 PTY，drain loop 异步
/// 把输出推进 grid。验证整条 live spawn → input → async drain → grid 渲染
/// 链路在真实 PTY 下端到端可用，且未发送的字串不出现在 grid。
///
/// 数据构造（含关键数值推导）：
///   - `make_app_real(cx)`：`MultisoulApp::new(cx)` → `Runtime::new()`（pty_factory=None）
///     → `on_new_terminal` 走 `spawn_term_live(None)` → `TermLive::spawn_command`
///     → `PtyHandle::spawn_command` → `portable_pty` 启动真实 `/bin/zsh` 子进程。
///   - shell：`/bin/zsh`（`on_new_terminal` 在 `crates/app/src/app/mod.rs:211` 硬编码）。
///     SPEC §3.4 建议 `/bin/sh` 规避 rc 噪声，但无公开 API 覆盖 shell；此处偏离
///     并依赖 `contains("hi")` 鲁棒断言。zsh 启动可能输出 prompt、rc 提示等噪声，
///     但 `echo hi` 的输出 `hi` 必然作为独立 token 出现在 grid 某行。
///   - 输入 `NodeInput::Text("echo hi\r".to_string())`：
///     `TermLive::input` → `session.write_bytes(b"echo hi\r")`（11 字节）。
///     `write_bytes` 不追加换行（见 `crates/runtime/src/nodes/term.rs:119`），
///     故需在字符串内显式带 `\r` 触发 shell 执行（CR 在 cooked tty 模式下转 LF）。
///   - cols=80, rows=24（runtime DEFAULT_SESSION_COLS/ROWS）。
///   - 负对照字串 "bye"：从未发送，zsh 不会凭空输出。
///   - drain timeout = 2000ms：真 PTY 异步，shell 启动 + rc 解析 + echo 执行 +
///     PTY 读线程 + drain 推进链路耗时远高于 L1 同步回吐；2000ms 留足余量
///     （L1 用 500ms，L2 取 4×）。若仍抖动可上调，但保持 `#[ignore]`。
///
/// 执行过程：
///   1. `make_app_real(cx)` → boot headless GPUI 窗口，root view focused +
///      window activated；初始 screen = Launchpad，runtime 无 PTY 工厂（真 PTY 路径）。
///   2. `enter_canvas(app, cx, tmp.path())` → `enter_canvas_for_test` 直接置
///      `screen = Canvas` 并 `set_workspace_root(root)`，不替换 runtime，
///      `pty_factory` 仍为 `None`（真 PTY 路径保留）。
///   3. `create_terminal(app, cx)` → dispatch `NewTerminal`：`on_new_terminal`
///      调 `runtime.spawn_terminal(bounds, "/bin/zsh", cols, rows)` →
///      `spawn_term_live(None)` → `TermLive::spawn_command` →
///      `PtyHandle::spawn_command` 起 `portable_pty` 子进程 + 读线程；
///      `activate_spawned_node` 把焦点设为 `NodeFocused(node_id)`。
///   4. `app.update(cx, |a, _| a.runtime.send_node_input(node, NodeInput::Text("echo hi\r".into())))`
///      → `TermLive::input` → `session.write_bytes(b"echo hi\r")` →
///      `pty.write(b"echo hi\r")` → 真实 PTY master 写入 → zsh 子进程读取执行。
///   5. `drain_until(app, cx, predicate, 2000ms)`：每轮 `advance_clock(12ms)` 让
///      12ms 周期 drain timer 就绪 + `run_until_parked` 推进 executor →
///      `runtime.drain_all()` → `session.drain()` → `rx.try_recv` 取 PTY 读线程
///      投递的 `Output(b"hi\r\n" + 噪声)` → `grid.advance(...)` → dirty。
///      谓词读 `read_grid` 拼全 grid 文本，`contains("hi")` 为真时返回 `Some(text)`。
///   6. 拿到 grid 后做正/负断言：含 `hi`、不含 `bye`。
///
/// 预期结果：
///   - 正断言：`drain_until` 返回 `Some(text)`（2000ms 内 grid 出现 `hi`，未超时）。
///   - 正断言：grid 拼接文本 `contains("hi")`（zsh 执行 `echo hi` 输出落到 grid）。
///   - 负断言：grid 拼接文本 `!contains("bye")`（从未发送，真实 shell 不会凭空输出）。
#[test]
#[ignore = "真 PTY 集成测试，CI 默认跳过；本地 `cargo test -p e2e -- --ignored` 跑"]
fn e2e_02_real_pty_echo_hi_into_grid() {
    let mut cx = TestAppContext::single();

    let (app, cx) = make_app_real(&mut cx);
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());
    let node = create_terminal(&app, cx);

    // 写入输入：send_node_input(Text("echo hi\r")) → 真实 PTY master → zsh 子进程执行。
    // `\r` 必须显式带：write_bytes 不追加换行，shell 需 CR 触发执行。
    app.update(cx, |a, _cx| {
        a.runtime
            .send_node_input(node, NodeInput::Text("echo hi\r".to_string()))
            .expect("send_node_input(Text(\"echo hi\\r\")) should write to real PTY master")
    });

    // drain_until：推进 12ms 周期 drain timer，直到 grid 含 "hi" 或 2000ms 超时。
    // 真 PTY 异步：shell 启动 + rc + echo 执行 + 读线程 + drain 链路耗时高，
    // 2000ms = 4 × L1 的 500ms，留足余量。available budget = 2000ms / 12ms ≈ 166 轮。
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
        Duration::from_millis(2000),
    )
    .expect("real /bin/zsh should echo `hi` into grid within 2000ms");

    // ---- PRIMARY 正断言：grid 含 "hi"（zsh 执行 echo hi 的输出落到 grid）----
    assert!(
        satisfied.contains("hi"),
        "grid 拼接文本应含真 PTY 回写的 'hi'，实际: {:?}",
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

    // 清理：杀掉所有 live node 的 PTY（真 PTY 子进程需显式 kill 防泄漏）。
    app.update(cx, |a, _cx| a.runtime.kill_all());
}
