use super::*;
use crate::live::{LiveNode, NodeInput, NodeView};
use crossbeam_channel::unbounded;
use std::sync::Arc;
use terminal::{PtyBackend, PtyCommand, PtyEvent, TerminalSession, TestPtyBackend};

/// TNV-6：向 Agent 终端粘贴多行文本时，每个逻辑行都必须从终端左边界开始可见。
///
/// 数据构造（含关键数值的推导过程）：
///   第一行 "first"  = 5 个 ASCII cells
///   分行符 "\n"     = 1 byte，粘贴语义应标准化为 "\r\n" = 2 个终端控制 bytes
///   第二行 "second" = 6 个 ASCII cells
///   Total           = 5 + 1 + 6 = 12 个输入 bytes；显示占两行 5 + 6 cells
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立 20×4 的 echo PTY，使写入字节原样回灌到终端 grid。
///   2. 通过真实 `TermLive::input(NodeInput::Paste)` 写入含 LF 的两行粘贴文本。
///   3. `drain()` 将回灌数据解析成可供 Agent 卡片渲染的 `GridSnapshot`。
///   4. 读取第 0、1 行前六格，验证 LF 不会让第二行继承第一行的水平光标位置。
///
/// 预期结果：
///   - 断言 A：第一行保留 "first"，证明原始内容没有丢失。
///   - 断言 B：第二行从第 0 列显示 "second"，保证多行内容不会横向错位或被裁出卡片。
///   - 断言 C：第二行第 5 列不是空格，防止回归为仅下移、不回车的裸 LF 行为。
#[test]
fn test_multiline_text_input_restarts_each_line_at_terminal_left_edge() {
    let factory: PtyFactory = Box::new(
        |_cmd: PtyCommand, _cols: u16, _rows: u16, tx: Sender<PtyEvent>| {
            Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
        },
    );
    let mut live = TermLive::spawn_with_factory(46, PtyCommand::new("/bin/sh"), 20, 4, &factory)
        .expect("多行粘贴测试的 echo TermLive 应创建成功");

    live.input(NodeInput::Paste("first\nsecond".to_string()))
        .expect("含 LF 的多行粘贴应成功写入 Agent 终端");
    let drained = live.drain();
    let Some(NodeView::Term { snapshot, .. }) = live.view() else {
        panic!("写入后的 Agent 终端必须提供可渲染的 NodeView::Term");
    };
    let first_row: String = snapshot.rows[0].iter().take(6).map(|cell| cell.c).collect();
    let second_row: String = snapshot.rows[1].iter().take(6).map(|cell| cell.c).collect();

    assert!(drained, "echo PTY 回灌多行文本后 drain 必须标记为 dirty");
    assert_eq!(
        first_row, "first ",
        "第一行应完整显示 first，实际首行前六格为 {first_row:?}"
    );
    assert_eq!(
        second_row, "second",
        "第二行必须从第 0 列显示 second，实际第二行前六格为 {second_row:?}"
    );
    assert_ne!(
        snapshot.rows[1][5].c, ' ',
        "第二行第 5 列必须是 second 的末字母 d，不能因裸 LF 留空而被横向裁出"
    );

    live.kill();
}

/// TNV-7：多行粘贴必须把常见的三种换行格式收敛为恰好一个终端 CRLF。
///
/// 数据构造（含关键数值的推导过程）：
///   input = "a\\r\\nb\\rc\\nd"，包含 CRLF、CR、LF 三种行分隔符，共 8 bytes
///   logical line breaks = 3
///   canonical terminal separator = "\\r\\n"，每个 2 bytes
///   expected = "a\\r\\nb\\r\\nc\\r\\nd" = 10 bytes（原 CRLF 不得额外增加 CR）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立 recording PTY，只记录 `TermLive` 实际写入的输入字节。
///   2. 通过 `NodeInput::Paste` 写入同时含 CRLF、CR、LF 的粘贴文本。
///   3. 读取唯一一次 PTY 写入，比较其终端行分隔符是否已规范化。
///
/// 预期结果：
///   - 断言 A：写入精确等于 `a\\r\\nb\\r\\nc\\r\\nd`，所有逻辑换行均可正确显示为新行。
///   - 断言 B：写入次数为 1，避免在多行粘贴中拆成多次、导致 Agent TUI 观察到中间状态。
///   - 断言 C：结果不含 `\\r\\r\\n`，避免 CRLF 被重复转换而生成多余回车。
#[test]
fn test_multiline_text_input_normalizes_all_line_endings_once() {
    let writes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (tx, rx) = unbounded();
    let backend = TestPtyBackend::recording(tx, writes.clone());
    let session = TerminalSession::spawn_with_backend(47, 20, 4, Box::new(backend), rx);
    let mut live = TermLive::from_session(session).expect("recording TermLive 应创建成功");

    live.input(NodeInput::Paste("a\r\nb\rc\nd".to_string()))
        .expect("混合换行的多行粘贴应成功写入 Agent 终端");
    let recorded = writes.lock().expect("测试写入记录锁不应被污染").clone();

    assert_eq!(
        recorded,
        vec![b"a\r\nb\r\nc\r\nd".to_vec()],
        "CRLF、CR 和 LF 都必须各自规范化为一个 CRLF，实际写入为 {recorded:?}"
    );
    assert_eq!(recorded.len(), 1, "一次多行粘贴必须只触发一次 PTY write");
    assert!(
        !recorded[0].windows(3).any(|bytes| bytes == b"\r\r\n"),
        "规范化结果不得含 CRCRLF，避免已有 CRLF 被重复转换"
    );

    live.kill();
}

/// TNV-8：Codex 开启 bracketed paste 后，多行文本必须作为一个 paste 事件进入草稿。
///
/// 数据构造（含关键数值的推导过程）：
///   mode enable = ESC[?2004h = 8 bytes（终端应用声明支持 bracketed paste）
///   input       = "first\\nsecond" = 5 + 1 + 6 = 12 bytes
///   paste begin = ESC[200~ = 6 bytes；paste end = ESC[201~ = 6 bytes
///   expected    = 6 + 12 + 6 = 24 bytes，且只允许 1 次 PTY write
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立 recording PTY，并从应用输出侧注入 ESC[?2004h。
///   2. drain 输出，使真实 terminal 状态机记录 bracketed-paste mode。
///   3. 通过 `LiveNode::input(NodeInput::Paste)` 粘贴两行正文。
///   4. 检查 PTY 输入侧收到的完整协议字节。
///
/// 预期结果：
///   - 正断言：唯一写入精确为 ESC[200~first\\nsecondESC[201~。
///   - 正断言：正文的逻辑换行保留为 LF，供 Codex `handle_paste` 作为草稿换行处理。
///   - 负断言：不得写出裸 `first\\r\\nsecond`，否则 CR 会被 Codex 当成 Enter 提交。
#[test]
fn test_bracketed_paste_keeps_multiline_text_in_single_codex_draft() {
    let writes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (tx, rx) = unbounded();
    let backend = TestPtyBackend::recording(tx.clone(), writes.clone());
    let session = TerminalSession::spawn_with_backend(48, 20, 4, Box::new(backend), rx);
    let mut live = TermLive::from_session(session).expect("Codex bracketed-paste 会话应创建成功");

    tx.send(PtyEvent::Output(b"\x1b[?2004h".to_vec()))
        .expect("测试 PTY 应能注入 bracketed-paste enable 序列");
    assert!(
        live.drain(),
        "bracketed-paste enable 输出必须被 terminal 状态机 drain"
    );

    live.input(NodeInput::Paste("first\nsecond".to_string()))
        .expect("Codex 多行文本应成功写入 PTY");
    let recorded = writes.lock().expect("测试写入记录锁不应被污染").clone();

    assert_eq!(recorded.len(), 1, "一次 Cmd+V 只能产生一次 PTY write");
    assert_eq!(
        recorded[0], b"\x1b[200~first\nsecond\x1b[201~",
        "开启 bracketed paste 后，多行正文必须由 begin/end 包装并保留 LF，实际写入为 {:?}",
        recorded[0]
    );
    assert_ne!(
        recorded[0], b"first\r\nsecond",
        "多行正文不得以裸 CRLF 写入 Codex，否则会被拆成多次提交"
    );

    live.kill();
}

/// TNV-9：普通 IME 文本输入不得因终端启用 bracketed paste 而变成粘贴事件。
///
/// 数据构造（含关键数值的推导过程）：
///   mode enable = ESC[?2004h = 8 bytes（zsh 声明支持 bracketed paste）
///   typed text  = "car" = 3 个 ASCII bytes
///   raw input   = "car" = 3 bytes
///   forbidden paste = ESC[200~(6) + "car"(3) + ESC[201~(6) = 15 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立 recording PTY，并从输出侧注入 ESC[?2004h。
///   2. drain 输出，使真实 terminal 状态机记录 bracketed-paste mode。
///   3. 通过 `LiveNode::input(NodeInput::Text)` 模拟 GPUI IME commit 普通字符 `car`。
///   4. 检查 PTY 输入侧收到的完整字节，区分打字与 Cmd+V 粘贴语义。
///
/// 预期结果：
///   - 正断言：唯一写入精确为裸 `car`，使 zsh ZLE 按普通 self-insert 刷新 autosuggestion。
///   - 正断言：写入次数为 1，整段 IME commit 不应被拆散或重复。
///   - 负断言：不得出现 bracketed-paste begin/end，否则建议只能等下一次 Tab widget 才刷新。
#[test]
fn test_plain_text_input_bypasses_bracketed_paste_markers() {
    let writes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (tx, rx) = unbounded();
    let backend = TestPtyBackend::recording(tx.clone(), writes.clone());
    let session = TerminalSession::spawn_with_backend(49, 20, 4, Box::new(backend), rx);
    let mut live = TermLive::from_session(session).expect("普通文本输入 TermLive 应创建成功");

    tx.send(PtyEvent::Output(b"\x1b[?2004h".to_vec()))
        .expect("测试 PTY 应能注入 bracketed-paste enable 序列");
    assert!(
        live.drain(),
        "bracketed-paste enable 输出必须被 terminal 状态机 drain"
    );

    live.input(NodeInput::Text("car".to_string()))
        .expect("普通 IME 文本应成功写入 PTY");
    let recorded = writes.lock().expect("测试写入记录锁不应被污染").clone();

    assert_eq!(
        recorded,
        vec![b"car".to_vec()],
        "普通文本必须作为裸 UTF-8 写入，使 zsh 立即刷新建议；实际写入为 {recorded:?}"
    );
    assert_eq!(
        recorded.len(),
        1,
        "一次 IME commit 必须只产生一次 PTY write，实际写入为 {recorded:?}"
    );
    assert!(
        !recorded[0].starts_with(b"\x1b[200~") && !recorded[0].ends_with(b"\x1b[201~"),
        "普通文本不得携带 bracketed-paste begin/end，实际写入为 {:?}",
        recorded[0]
    );

    live.kill();
}
