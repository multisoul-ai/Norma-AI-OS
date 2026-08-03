use norma::client::{
    decode_request_line, decode_response_line, encode_request_line, encode_response_line,
};
use norma::protocol::{
    CardCommand, CardCommandResult, CardKind, Placement, WireRequest, WireResponse,
};

/// Norma client codec：request 必须编码为单行 JSON，并可恢复为原始 command。
///
/// 数据构造（含关键数值的推导过程）：
///   request_id = 11（固定 id，验证 codec 不改写）
///   command = Create Terminal(shell=None)
///   NDJSON 行尾 = 1 个 '\n'，socket 读取可按行分帧
///
/// 执行过程（逐步说明系统如何处理）：
///   1. encode_request_line(request)
///   2. 检查输出以换行结束且中间不包含额外换行
///   3. decode_request_line(encoded)
///
/// 预期结果：
///   - 正断言：decoded 等于 request
///   - 正断言：encoded 有且只有末尾换行
///   - 负断言：request_id 不应丢失或变成 0
#[test]
fn test_request_codec_uses_single_newline_delimited_json() {
    let request = WireRequest {
        protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
        client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        request_id: 11,
        operation_id: None,
        authorization: None,
        source_workspace_id: None,
        source_workspace_route_id: None,
        target_workspace_id: None,
        source_node_id: None,
        command: CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        },
    };

    let encoded = encode_request_line(&request).expect("request 应可编码");
    let decoded = decode_request_line(&encoded).expect("request line 应可解码");

    assert!(
        encoded.ends_with('\n'),
        "request codec 应追加一个换行作为 NDJSON 分隔符"
    );
    assert_eq!(
        encoded.matches('\n').count(),
        1,
        "request codec 不应在 JSON 内产生额外换行"
    );
    assert_eq!(decoded, request, "request codec 往返应保留原始 command");
    assert_ne!(decoded.request_id, 0, "request_id 不应丢失或变成默认 0");
}

/// Norma client codec：response 必须编码为单行 JSON，并可恢复结构化结果。
///
/// 数据构造（含关键数值的推导过程）：
///   request_id = 11（与 request 配对）
///   result = Created{node_id=4}
///   NDJSON 行尾 = 1 个 '\n'
///
/// 执行过程（逐步说明系统如何处理）：
///   1. encode_response_line(response)
///   2. decode_response_line(encoded)
///   3. 检查 request_id 与 Created node_id
///
/// 预期结果：
///   - 正断言：response 往返后仍为 Created{4}
///   - 负断言：response 不应变成 Listed 或错误
#[test]
fn test_response_codec_round_trips_created_result() {
    let response = WireResponse {
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
        request_id: 11,
        result: Ok(CardCommandResult::Created { node_id: 4 }),
    };

    let encoded = encode_response_line(&response).expect("response 应可编码");
    let decoded = decode_response_line(&encoded).expect("response line 应可解码");

    assert_eq!(
        decoded.request_id, 11,
        "response request_id 应保留 11 以匹配原 request"
    );
    assert_eq!(
        decoded.result,
        Ok(CardCommandResult::Created { node_id: 4 }),
        "response codec 往返后应仍为 Created node_id=4"
    );
    assert_ne!(
        decoded.result,
        Ok(CardCommandResult::Listed { cards: Vec::new() }),
        "Created response 不应被误解码为 Listed"
    );
}
