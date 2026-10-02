use super::*;
use crate::test_backend::TestPtyBackend;
use crossbeam_channel::unbounded;

/// 注入式 e2e：spawn_with_backend + TestPtyBackend::echo(tx) 驱动 write→drain→snapshot，
/// 验证 backend 注入路径闭环且不启动真实子进程。
///
/// 数据构造（含关键数值推导）：
///   通道：unbounded() → (tx, rx) 一对
///   backend = TestPtyBackend::echo(tx)：write(bytes) 经 tx.send(Output(bytes)) 同步回吐
///   session = spawn_with_backend(7, 80, 24, Box::new(backend), rx)
///     —— backend 持 tx（write 走 tx），session 持 rx（drain 走 rx），共享通道闭环
///   写入：write_bytes(b"hi") = 2 字节
///     echo 模式 → emit(b"hi") → tx.send(Output(b"hi")) 同步成功 → rx 立即可取
///
/// 执行过程：
///   1. 建通道 (tx, rx)
///   2. echo(tx) 构造 backend，tx 所有权移入 backend
///   3. spawn_with_backend(7, 80, 24, Box::new(backend), rx) 注入 session，rx 移入 session
///   4. write_bytes(b"hi") → backend.write → emit → tx.send(Output) → 通道收到事件
///   5. drain() 从 rx 取 Output(b"hi") → grid.advance → dirty=true，应返回 true
///   6. 再次 drain() 通道已空 → 返回 false
///   7. snapshot() 取 grid 当前屏幕
///
/// 预期结果：
///   - 正断言：首次 drain 返回 true（注入后端确实回吐了事件到通道）
///   - 正断言：快照首行包含 "hi"（echo 回吐的字节经 grid.advance 落盘）
///   - 正断言：session.id == 7（注入路径保留 id）
///   - 负断言：第二次 drain 返回 false（通道无遗留，无幽灵事件）
///   - 负断言：快照首行不应包含未写入的 "bye"
#[test]
fn test_spawn_with_backend_echo_drives_session() {
    let (tx, rx) = unbounded();
    let backend = TestPtyBackend::echo(tx);
    let mut session = TerminalSession::spawn_with_backend(7, 80, 24, Box::new(backend), rx);

    session
        .write_bytes(b"hi")
        .expect("write_bytes 经注入后端应成功");

    let dirty_first = session.drain();
    let dirty_second = session.drain();
    let snapshot = session.snapshot();
    let first_row: String = snapshot.rows[0].iter().map(|cell| cell.c).collect();

    assert!(
        dirty_first,
        "注入后端 write 后应首次 drain 返回 true，否则说明通道未收到 echo 回吐"
    );
    assert!(
        first_row.contains("hi"),
        "快照首行应含 echo 回吐的 'hi'，实际: {:?}",
        first_row
    );
    assert_eq!(session.id, 7, "注入构造应保留 session_id=7");
    assert!(
        !dirty_second,
        "通道应已耗尽，第二次 drain 不应再返回 true（无幽灵事件）"
    );
    assert!(
        !first_row.contains("bye"),
        "快照首行不应含未写入的 'bye'，实际: {:?}",
        first_row
    );
}

/// 已停止的 Agent 会话不得继续接受输入并报告派工成功。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个 echo PTY 会话、会话 ID=10、终端尺寸=80×24；停止后尝试写入 1 条任务。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建会话，调用 kill 结束其运行实例。
///   2. 向同一会话写入任务和 Enter。
///   3. 排空 PTY 输出并检查屏幕内容。
///
/// 预期结果：
///   - 正断言：会话状态为 Exited，两次输入都返回错误。
///   - 负断言：停止后的任务没有出现在屏幕上。
#[test]
fn stopped_session_rejects_agent_input() {
    let (tx, rx) = unbounded();
    let backend = TestPtyBackend::echo(tx);
    let mut session = TerminalSession::spawn_with_backend(10, 80, 24, Box::new(backend), rx);
    session.kill();

    let task_result = session.write_paste("task-after-stop");
    let enter_result = session.write_input(&KeyInput {
        key: crate::input::Key::Enter,
        ctrl: false,
        alt: false,
        shift: false,
    });
    session.drain();
    let first_row: String = session.snapshot().rows[0]
        .iter()
        .map(|cell| cell.c)
        .collect();

    assert_eq!(
        session.status,
        TerminalStatus::Exited { code: None },
        "kill 后的会话必须立即标记为已退出"
    );
    assert!(task_result.is_err(), "已停止会话不得接受新任务");
    assert!(enter_result.is_err(), "已停止会话不得接受提交键");
    assert!(
        !first_row.contains("task-after-stop"),
        "停止后的输入不得到达 PTY 输出"
    );
}

/// Option+Enter：终端应用请求增强键盘协议后必须保留 Alt 修饰，不能退化为提交键。
///
/// 数据构造（含关键数值的推导过程）：
///   keyboard mode request = CSI > 7 u = ESC(1) + "[>7u"(4) = 5 bytes
///   Option/Alt modifier value = 1(default) + 2(alt bit) = 3
///   expected Option+Enter = CSI 13;3u = ESC(1) + "[13;3u"(6) = 7 bytes
///   forbidden plain Enter = CR = [0x0d] = 1 byte
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立 recording backend 与 TerminalSession，保留协议输出 sender。
///   2. 从 PTY 输出侧注入 CSI > 7 u，并 drain 到 TerminalGrid 激活增强键盘模式。
///   3. 通过公开 `write_input` 写入 Key::Enter + alt=true，模拟 macOS Option+Enter。
///   4. 读取 recording backend 的真实 PTY 写入字节。
///
/// 预期结果：
///   - 正断言：协议声明被 drain，确保测试确实处于增强键盘模式。
///   - 正断言：PTY 精确收到 CSI 13;3u，保留 Option 修饰供 TUI 插入换行。
///   - 负断言：PTY 不得收到裸 CR，否则 TUI 会把它当成 Enter 并直接提交。
#[test]
fn test_option_enter_uses_csi_u_when_enhanced_keyboard_is_enabled() {
    let (tx, rx) = unbounded();
    let protocol_tx = tx.clone();
    let writes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let backend = TestPtyBackend::recording(tx, writes.clone());
    let mut session = TerminalSession::spawn_with_backend(8, 80, 24, Box::new(backend), rx);

    protocol_tx
        .send(crate::message::PtyEvent::Output(b"\x1b[>7u".to_vec()))
        .expect("测试 PTY 应能声明增强键盘协议");
    let mode_drained = session.drain();
    session
        .write_input(&crate::input::KeyInput {
            key: crate::input::Key::Enter,
            ctrl: false,
            alt: true,
            shift: false,
        })
        .expect("Option+Enter 应成功写入 recording PTY");

    let recorded = writes
        .lock()
        .expect("Option+Enter 写入记录锁不应被污染")
        .clone();
    assert!(
        mode_drained,
        "CSI > 7 u 应被 session drain，确保增强键盘模式已激活"
    );
    assert_eq!(
        recorded,
        vec![b"\x1b[13;3u".to_vec()],
        "增强键盘模式下 Option+Enter 应精确编码为 CSI 13;3u，实际写入: {recorded:?}"
    );
    assert_ne!(
        recorded,
        vec![vec![0x0d]],
        "Option+Enter 不得退化为裸 CR，否则 TUI 会直接提交消息"
    );
}

/// Shift+Enter 在仅声明 bracketed paste 的 TUI 中插入换行，协议退出后恢复 legacy。
///
/// 数据构造（含关键数值推导）：
///   80×24 为标准终端尺寸；session id=9 仅标识本测试。
///   paste newline = begin(6) + LF(1) + end(6) = 13 bytes。
///   Kitty Shift+Enter = ESC(1) + "[13;2u"(6) = 7 bytes；普通 Enter = CR(1)。
/// 执行过程：
///   1. 开启 bracketed paste，写 Shift+Enter 和 Enter，分别记录换行与提交。
///   2. 同时开启 Kitty，Shift+Enter 应优先保留原始按键协议。
///   3. 退出两个模式，再写 Shift+Enter，验证模式不会泄漏到 legacy shell。
///
/// 预期结果：
///   - 换行是单个 bracketed paste，不能成为裸 CR；普通 Enter 仍提交。
///   - Kitty 返回 CSI-u；关闭协议后返回 legacy CR。
#[test]
fn test_shift_enter_falls_back_to_bracketed_newline() {
    use crate::input::Key;
    let (tx, rx) = unbounded();
    let writes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let backend = TestPtyBackend::recording(tx.clone(), writes.clone());
    let mut session = TerminalSession::spawn_with_backend(9, 80, 24, Box::new(backend), rx);
    let shift_enter = KeyInput {
        key: Key::Enter,
        ctrl: false,
        alt: false,
        shift: true,
    };
    tx.send(PtyEvent::Output(b"\x1b[?2004h".to_vec())).unwrap();
    session.drain();
    session.write_input(&shift_enter).unwrap();
    session
        .write_input(&KeyInput {
            shift: false,
            ..shift_enter
        })
        .unwrap();
    tx.send(PtyEvent::Output(b"\x1b[>1u".to_vec())).unwrap();
    session.drain();
    session.write_input(&shift_enter).unwrap();
    tx.send(PtyEvent::Output(b"\x1b[<u\x1b[?2004l".to_vec()))
        .unwrap();
    session.drain();
    session.write_input(&shift_enter).unwrap();
    let recorded = writes.lock().unwrap();
    assert_eq!(recorded.len(), 4, "四次按键必须各写一次 PTY");
    assert_eq!(
        recorded[0], b"\x1b[200~\n\x1b[201~",
        "legacy TUI 必须收到粘贴换行"
    );
    assert_ne!(recorded[0], b"\r", "Shift+Enter 不得提交");
    assert_eq!(recorded[1], b"\r", "普通 Enter 必须保持提交");
    assert_eq!(recorded[2], b"\x1b[13;2u", "Kitty 必须优先于粘贴兼容路径");
    assert_eq!(recorded[3], b"\r", "关闭协议后必须恢复 legacy 编码");
}
