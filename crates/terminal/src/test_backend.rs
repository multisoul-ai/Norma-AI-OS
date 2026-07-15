//! 测试用 PTY 后端：不启动真实子进程，按 echo / scripted 规则回吐输出，
//! 供 e2e 测试桩注入 `TerminalSession` 驱动上层逻辑。域层无 GPUI 依赖。

use crate::message::PtyEvent;
use crate::pty::{PtyBackend, PtyError};
use crossbeam_channel::Sender;

/// 剧本规则：trigger 子串 → 回吐 output。`write` 的字节若包含某 trigger 子串，
/// 则吐对应 output。
pub type ScriptRule = (Vec<u8>, Vec<u8>);

/// 测试后端的回吐模式。
enum EchoMode {
    /// 原样回显：`write(bytes)` → 吐 `bytes`。
    Echo,
    /// 剧本：`write` 含某 trigger 子串 → 吐对应 output；无匹配则静默。
    Scripted(Vec<ScriptRule>),
}

/// 测试用 PTY 后端。实现 `PtyBackend`，不启动任何子进程；
/// `write` 时按 echo / scripted 规则通过 `tx` 同步投递 `PtyEvent::Output`，
/// 模拟真实 PTY 读线程回吐。`resize`/`kill` 为 no-op（`kill` 仅记录 flag）。
pub struct TestPtyBackend {
    tx: Sender<PtyEvent>,
    mode: EchoMode,
    killed: bool,
}

impl TestPtyBackend {
    /// 回显模式：`write(bytes)` 把 `bytes` 原样通过 `tx` 吐回。最简单、最确定性。
    pub fn echo(tx: Sender<PtyEvent>) -> Self {
        TestPtyBackend {
            tx,
            mode: EchoMode::Echo,
            killed: false,
        }
    }

    /// 剧本模式：`rules` 为 `(trigger, output)` 列表；`write` 的字节中
    /// 若包含某 trigger 子串，则吐对应 output（取首个匹配）。无匹配则静默不吐。
    pub fn scripted(tx: Sender<PtyEvent>, rules: Vec<ScriptRule>) -> Self {
        TestPtyBackend {
            tx,
            mode: EchoMode::Scripted(rules),
            killed: false,
        }
    }

    /// `kill` 是否曾被调用（测试可观察）。
    pub fn was_killed(&self) -> bool {
        self.killed
    }

    fn emit(&self, bytes: &[u8]) -> Result<(), PtyError> {
        self.tx
            .send(PtyEvent::Output(bytes.to_vec()))
            .map_err(|_| PtyError::Spawn("test backend channel closed".into()))
    }
}

impl PtyBackend for TestPtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        match &self.mode {
            EchoMode::Echo => self.emit(bytes),
            EchoMode::Scripted(rules) => {
                for (trigger, output) in rules {
                    if contains_subslice(bytes, trigger) {
                        return self.emit(output);
                    }
                }
                Ok(())
            }
        }
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {
        self.killed = true;
    }
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    /// echo 模式回显：write `b"hi"` → 从配对 rx 收到含 `hi` 的 `PtyEvent::Output`。
    ///
    /// 数据构造（含关键数值推导）：
    ///   输入 bytes = b"hi"  = 2 字节
    ///   echo 模式 → emit(bytes) → tx.send(Output(b"hi".to_vec()))
    ///   通道为 unbounded，send 同步成功，rx 立即可 recv
    ///
    /// 执行过程：
    ///   1. unbounded() 建立 (tx, rx) 通道
    ///   2. echo(tx) 构造回显后端，mode = Echo，killed = false
    ///   3. write(b"hi") → emit(b"hi") → 通道收到 Output(b"hi")
    ///   4. rx.recv() 取出事件
    ///
    /// 预期结果：
    ///   - 正断言：收到 PtyEvent::Output（而非 Exited）
    ///   - 正断言：output 字节含子串 "hi"
    ///   - 负断言：output 不应含未写入的 "bye"
    #[test]
    fn test_echo_mode_writes_back_input_bytes() {
        let (tx, rx) = unbounded();
        let mut backend = TestPtyBackend::echo(tx);

        backend
            .write(b"hi")
            .expect("echo write should succeed without channel error");

        let event = rx
            .recv()
            .expect("should receive an Output event from echo backend");
        match event {
            PtyEvent::Output(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                assert!(
                    text.contains("hi"),
                    "echo 模式 Output 应包含写入的 'hi'，实际: {:?}",
                    text
                );
                assert!(
                    !text.contains("bye"),
                    "echo 模式不应回吐未写入的 'bye'，实际: {:?}",
                    text
                );
            }
            PtyEvent::Exited { .. } => panic!("echo 模式 write 不应产生 Exited 事件"),
        }
    }

    /// scripted 模式按 trigger 回吐：rules 含 (b"echo hi", b"hi\r\n")，
    /// write `b"echo hi\r"` → 收到含 `hi` 的 Output。
    ///
    /// 数据构造（含关键数值推导）：
    ///   rule = (b"echo hi".to_vec(), b"hi\r\n".to_vec())
    ///   trigger "echo hi" = 7 字节
    ///   write 输入 b"echo hi\r" = 8 字节（含尾部 \r）
    ///   contains_subslice(b"echo hi\r", b"echo hi") → true（前 7 字节匹配）
    ///   → emit(b"hi\r\n") = 4 字节
    ///
    /// 执行过程：
    ///   1. unbounded() 建立 (tx, rx) 通道
    ///   2. scripted(tx, [rule]) 构造剧本后端，mode = Scripted
    ///   3. write(b"echo hi\r") → 遍历 rules 匹配 trigger "echo hi" → emit(b"hi\r\n")
    ///   4. rx.recv() 取出事件
    ///
    /// 预期结果：
    ///   - 正断言：收到 PtyEvent::Output（而非 Exited）
    ///   - 正断言：output 含 "hi"
    ///   - 正断言：output 含 "\r\n"（剧本指定的 CRLF 换行）
    ///   - 负断言：output 不含 "echo hi"（回吐的是 output 而非 trigger 文本）
    #[test]
    fn test_scripted_mode_emits_output_on_trigger_match() {
        let (tx, rx) = unbounded();
        let rules = vec![(b"echo hi".to_vec(), b"hi\r\n".to_vec())];
        let mut backend = TestPtyBackend::scripted(tx, rules);

        backend
            .write(b"echo hi\r")
            .expect("scripted write should succeed without channel error");

        let event = rx
            .recv()
            .expect("should receive an Output event from scripted backend");
        match event {
            PtyEvent::Output(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                assert!(
                    text.contains("hi"),
                    "scripted 模式 Output 应包含规则 output 的 'hi'，实际: {:?}",
                    text
                );
                assert!(
                    text.contains("\r\n"),
                    "scripted 模式 Output 应保留规则 output 的 CRLF，实际: {:?}",
                    text
                );
                assert!(
                    !text.contains("echo hi"),
                    "scripted 模式不应回吐 trigger 文本 'echo hi'，实际: {:?}",
                    text
                );
            }
            PtyEvent::Exited { .. } => panic!("scripted 模式 write 不应产生 Exited 事件"),
        }
    }

    /// scripted 模式无匹配时静默：write `b"ls\r"` 与 rule trigger `b"echo hi"`
    /// 不匹配 → 通道不应收到任何事件。同时验证 resize/kill 为 no-op 且 kill 置 flag。
    ///
    /// 数据构造（含关键数值推导）：
    ///   rule = (b"echo hi".to_vec(), b"hi\r\n".to_vec())
    ///   write 输入 b"ls\r" = 3 字节
    ///   contains_subslice(b"ls\r", b"echo hi") → false（长度 3 < 7，windows 为空）
    ///   → 不 emit，write 返回 Ok(())
    ///
    /// 执行过程：
    ///   1. scripted(tx, [rule]) 构造剧本后端
    ///   2. write(b"ls\r") → 无 trigger 匹配 → 静默 Ok
    ///   3. resize(80, 24) → no-op Ok
    ///   4. kill() → killed = true
    ///   5. rx.recv_timeout(50ms) → 应超时无事件
    ///
    /// 预期结果：
    ///   - 负断言：50ms 内通道不应收到任何事件（try_recv 为空）
    ///   - 正断言：was_killed() 为 true（kill 被调用并记录 flag）
    #[test]
    fn test_scripted_mode_silent_on_no_match_and_kill_records_flag() {
        let (tx, rx) = unbounded();
        let rules = vec![(b"echo hi".to_vec(), b"hi\r\n".to_vec())];
        let mut backend = TestPtyBackend::scripted(tx, rules);

        backend
            .write(b"ls\r")
            .expect("scripted write with no match should still return Ok");
        backend
            .resize(80, 24)
            .expect("test backend resize should be a no-op returning Ok");
        backend.kill();

        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Err(_) => { /* 期望：无事件 */ }
            Ok(other) => panic!("无 trigger 匹配时通道不应收到任何事件，但收到: {:?}", other),
        }
        assert!(
            backend.was_killed(),
            "kill() 后 was_killed 应为 true，说明 flag 被正确记录"
        );
    }
}
