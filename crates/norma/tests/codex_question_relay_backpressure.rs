#![cfg(any(unix, windows))]
use norma::codex::relay::CodexQuestionRelay;
use norma::codex::{UnixListener, UnixStream};
#[path = "support/codex_relay_slow_cases.rs"]
mod cases;

/// 场景：任一端短暂停止读取，大帧必须受背压而非撑爆relay缓冲并断开原TUI。
/// 构造：48条各512KiB消息共24MiB，超过本机socket队列容量；接收端暂时不消费。
/// 执行：分别以native/server为来源，连续写后等待1秒确认尚无错误；恢复读取后逐条核对并回复。
/// 预期：内存有界、连接存活，全部帧保持顺序与内容，观察器不得因此拆掉原生会话。
#[test]
fn slow_server_applies_backpressure_without_closing_the_native_connection() {
    cases::slow_server_applies_backpressure_without_closing_the_native_connection(
        CodexQuestionRelay::connect,
    );
}
