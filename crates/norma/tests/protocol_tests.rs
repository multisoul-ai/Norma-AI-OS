use norma::protocol::{
    AgentProfile, CardCommand, CardCommandError, CardCommandResult, CardKind, CardSummary,
    CardSummaryKind, Placement, PreviewType, WireRequest, WireResponse,
};
use std::path::PathBuf;

/// Norma 协议：创建 Preview 的命令应可 JSON 往返，并保留路径、显式类型和布局意图。
///
/// 数据构造（含关键数值的推导过程）：
///   request_id = 42（固定请求编号，用于匹配 response，不参与业务逻辑）
///   path = "/tmp/report.md"（绝对路径，CLI 传给 app 后由 app 读取）
///   explicit_type = Markdown（显式类型优先于扩展名）
///   placement = VisibleArea（由 app 按当前窗口可视区域计算 bounds）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 WireRequest{request_id=42, CardCommand::Create(Preview)}
///   2. serde_json::to_string 序列化为一行 JSON payload
///   3. serde_json::from_str 反序列化回 WireRequest
///
/// 预期结果：
///   - 正断言：request_id、path、explicit_type、placement 完整保留
///   - 负断言：Preview 命令不应退化为 Terminal 或 Agent
#[test]
fn test_preview_create_command_round_trips_through_json() {
    let request = WireRequest {
        request_id: 42,
        command: CardCommand::Create {
            kind: CardKind::Preview {
                path: PathBuf::from("/tmp/report.md"),
                explicit_type: Some(PreviewType::Markdown),
            },
            placement: Placement::VisibleArea,
        },
    };

    let json = serde_json::to_string(&request).expect("WireRequest 应可序列化为 JSON");
    let restored: WireRequest = serde_json::from_str(&json).expect("WireRequest JSON 应可反序列化");

    assert_eq!(
        restored.request_id, 42,
        "request_id 应保留 42，用于 client 匹配响应"
    );
    let CardCommand::Create { kind, placement } = restored.command else {
        panic!("Preview create JSON 往返后应仍是 CardCommand::Create");
    };
    assert_eq!(
        placement,
        Placement::VisibleArea,
        "placement 应保留 VisibleArea，让 app 使用当前窗口可视区域"
    );
    let CardKind::Preview {
        path,
        explicit_type,
    } = kind
    else {
        panic!("Preview create JSON 往返后不应退化为 Terminal 或 Agent");
    };
    assert_eq!(
        path,
        PathBuf::from("/tmp/report.md"),
        "Preview path 应完整保留传入的绝对路径"
    );
    assert_eq!(
        explicit_type,
        Some(PreviewType::Markdown),
        "Preview explicit_type 应保留 Markdown"
    );
}

/// Norma 协议：成功和失败 response 都应携带同一个 request_id 与结构化结果。
///
/// 数据构造（含关键数值的推导过程）：
///   request_id = 7（固定值，用于验证 response/request 配对）
///   success node_id = 3（模拟刚创建的卡片）
///   missing node_id = 99（模拟删除不存在卡片）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 success WireResponse::Ok(Created{3})
///   2. 构造 error WireResponse::Err(NotFound{99})
///   3. 分别 JSON 往返
///
/// 预期结果：
///   - 正断言：success 保留 Created node_id=3
///   - 正断言：error 保留 NotFound node_id=99
///   - 负断言：error 不应被误判为 success
#[test]
fn test_wire_response_preserves_success_and_error_results() {
    let success = WireResponse {
        request_id: 7,
        result: Ok(CardCommandResult::Created { node_id: 3 }),
    };
    let error = WireResponse {
        request_id: 7,
        result: Err(CardCommandError::NotFound { node_id: 99 }),
    };

    let restored_success: WireResponse =
        serde_json::from_str(&serde_json::to_string(&success).expect("success JSON"))
            .expect("success response 应可反序列化");
    let restored_error: WireResponse =
        serde_json::from_str(&serde_json::to_string(&error).expect("error JSON"))
            .expect("error response 应可反序列化");

    assert_eq!(
        restored_success.result,
        Ok(CardCommandResult::Created { node_id: 3 }),
        "success response 应保留 Created node_id=3"
    );
    assert_eq!(
        restored_error.result,
        Err(CardCommandError::NotFound { node_id: 99 }),
        "error response 应保留 NotFound node_id=99"
    );
    assert!(
        restored_error.result.is_err(),
        "error response 不应被误判为 Ok，否则 CLI 会报告成功"
    );
}

/// Norma 协议：List 结果应用中性摘要描述卡片，不暴露 app/runtime/gpui 类型。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id=1 Terminal，focused=true，selected=true
///   node_id=2 Agent(Codex)，focused=false，selected=false
///   summary count = 2，用于验证列表不会丢卡片
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 Listed response，包含 Terminal 与 Codex Agent
///   2. JSON 往返后读取 cards
///
/// 预期结果：
///   - 正断言：两张卡片摘要都保留
///   - 正断言：Terminal focused/selected 状态保留
///   - 负断言：Codex Agent 不应被误标为 Terminal
#[test]
fn test_list_result_uses_neutral_card_summary() {
    let response = WireResponse {
        request_id: 5,
        result: Ok(CardCommandResult::Listed {
            cards: vec![
                CardSummary {
                    node_id: 1,
                    title: "Terminal".to_string(),
                    kind: CardSummaryKind::Terminal,
                    selected: true,
                    focused: true,
                },
                CardSummary {
                    node_id: 2,
                    title: "Codex".to_string(),
                    kind: CardSummaryKind::Agent {
                        profile: AgentProfile::Codex,
                    },
                    selected: false,
                    focused: false,
                },
            ],
        }),
    };

    let restored: WireResponse =
        serde_json::from_str(&serde_json::to_string(&response).expect("list response JSON"))
            .expect("list response 应可反序列化");

    let Ok(CardCommandResult::Listed { cards }) = restored.result else {
        panic!("List response 应恢复为 Listed 结果");
    };
    assert_eq!(cards.len(), 2, "List 应保留两张卡片摘要");
    assert_eq!(
        cards[0].kind,
        CardSummaryKind::Terminal,
        "第一张卡片应保持 Terminal 摘要"
    );
    assert!(
        cards[0].selected && cards[0].focused,
        "Terminal selected/focused 状态应保留"
    );
    assert_ne!(
        cards[1].kind,
        CardSummaryKind::Terminal,
        "Codex Agent 不应被误标为 Terminal"
    );
}
