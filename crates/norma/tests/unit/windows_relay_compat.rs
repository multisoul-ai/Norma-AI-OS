use super::{
    partial_tests::ControlledStream, transport::Readiness, CodexQuestionRelay, UnixStream,
};
use crate::codex::UnixListener;
use std::path::Path;
#[path = "../support/codex_relay_shutdown_cases.rs"]
mod shutdown;
#[path = "../support/codex_relay_slow_cases.rs"]
mod slow;

fn connect_without_api(native: UnixStream, server: &Path) -> Result<CodexQuestionRelay, String> {
    CodexQuestionRelay::connect_with(
        native,
        || UnixStream::connect(server).map(ControlledStream::from),
        false,
        true,
        false,
        Readiness::without_socket_notifications,
    )
}

/// 场景：缺API回退须保持双向背压下四种关闭契约。
/// 构造：复用原24MiB真实AF_UNIX fixture，仅将连接工厂强制置于缺API路径。
/// 执行：分别关闭两方向的写端/读端，仍使用原真实WouldBlock见证和3秒预算。
/// 预期：全部组合收口且发送者收到错误，不修改原断言或超时。
#[test]
fn missing_socket_api_preserves_both_direction_shutdown_contracts() {
    shutdown::backpressured_relay_closes_when_either_peer_exits_in_both_directions(
        connect_without_api,
    );
}

/// 场景：缺API时反向Ping不能破坏慢端存活与正文完整性。
/// 构造：复用原48×512KiB=24MiB双向fixture，读端暂停1秒后恢复。
/// 执行：强制缺API连接，逐帧核对原顺序/长度，并确认真实来源收到Ping。
/// 预期：暂停期间连接存活，全部帧与原确认正确，沿用原断言。
#[test]
fn missing_socket_api_preserves_slow_peer_payload_and_liveness() {
    slow::slow_server_applies_backpressure_without_closing_the_native_connection(
        connect_without_api,
    );
}
