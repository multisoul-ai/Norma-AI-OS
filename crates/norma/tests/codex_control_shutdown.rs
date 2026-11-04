#![cfg(any(unix, windows))]
use norma::codex::relay::CodexQuestionRelay;
use norma::codex::{UnixListener, UnixStream};
#[path = "support/codex_relay_shutdown_cases.rs"]
mod cases;

/// 场景：任一方向因慢读端施加背压时，写端或读端退出都必须结束原 relay。
/// 构造：真实双向 AF_UNIX WebSocket，48 × 512KiB = 24MiB 超过 socket 缓冲，读端暂停。
/// 执行：预序列化正文，确认真实 send/flush 返回 WouldBlock 后，分别关闭两端并回收线程。
/// 预期：两个方向各两种关闭均在 3s 内结束，发送方收到错误，不能忙等或遗留后台 IO。
#[test]
fn backpressured_relay_closes_when_either_peer_exits_in_both_directions() {
    cases::backpressured_relay_closes_when_either_peer_exits_in_both_directions(
        CodexQuestionRelay::connect,
    );
}
