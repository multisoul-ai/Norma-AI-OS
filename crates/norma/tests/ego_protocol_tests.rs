use norma::cli::{render_protocol_error, render_protocol_success, OutputFormat};
use norma::protocol::{
    CardCommandError, CardCommandResult, EgoCaptureMode, EgoCardStatus, EgoErrorCode, EgoOwnership,
    EgoPhase, EgoPresentationDisposition,
};

fn ready_status() -> EgoCardStatus {
    EgoCardStatus {
        node_id: 42,
        task_space_id: Some("space-7".to_string()),
        task_space_name: Some("Norma · Ego".to_string()),
        phase: EgoPhase::ReadyForUser,
        ownership: EgoOwnership::Inactive,
        bound_agent_node_id: None,
        capture_mode: EgoCaptureMode::ExternalWindow,
        message: Some("当前为外部窗口模式".to_string()),
        master: false,
    }
}

/// Ego 协议成功结果：展示响应必须包含卡片、Task Space、所有权和降级模式。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id=42，task_space_id="space-7"
///   master=false（创建 Ego 不得自动进入 Master）
///   capture_mode=external_window（POC 未通过专用窗口定向闸门）
///   schema_version=1（沿用现有 Norma 协议版本）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 EgoPresented(created) 结果
///   2. 通过 JSON 输出协议渲染
///   3. 解析 envelope 并读取嵌套 status
///
/// 预期结果：
///   - 正断言：node_id、Task Space ID、phase、capture mode 均可机器读取
///   - 正断言：disposition 为 created 且 master=false
///   - 负断言：响应不得误报 live_mirror 或 Master
#[test]
fn test_ego_presented_json_exposes_external_mode_and_non_master_state() {
    let output = render_protocol_success(
        OutputFormat::Json,
        "request-1".to_string(),
        &CardCommandResult::EgoPresented {
            status: ready_status(),
            disposition: EgoPresentationDisposition::Created,
        },
    )
    .expect("EgoPresented 应可渲染为 JSON");
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("EgoPresented 输出应是有效 JSON");

    assert_eq!(
        json["result"]["status"]["node_id"], 42,
        "EgoPresented 必须返回精确 node_id=42"
    );
    assert_eq!(
        json["result"]["status"]["task_space_id"], "space-7",
        "EgoPresented 必须返回精确 Task Space ID"
    );
    assert_eq!(
        json["result"]["status"]["phase"], "ready_for_user",
        "EgoPresented 必须返回 ready_for_user 阶段"
    );
    assert_eq!(
        json["result"]["status"]["capture_mode"], "external_window",
        "POC 闸门失败后的实现必须明确报告 external_window"
    );
    assert_eq!(
        json["result"]["disposition"], "created",
        "首次展示 Ego 卡片必须报告 created"
    );
    assert_eq!(
        json["result"]["status"]["master"], false,
        "创建 Ego 卡片不得自动设为 Master"
    );
    assert_ne!(
        json["result"]["status"]["capture_mode"], "live_mirror",
        "外部窗口降级模式不得谎报实时镜像"
    );
}

/// Ego 协议错误：忙碌冲突必须稳定表达当前所有者并使用业务退出码。
///
/// 数据构造（含关键数值的推导过程）：
///   owner node_id=10，challenger node_id=20
///   retryable=false（第一版无排队、抢占或自动重试）
///   业务校验错误退出码=5（沿用现有协议错误分类）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 EgoErrorCode::Busy
///   2. 渲染 JSON 错误 envelope
///   3. 检查结构化 code、消息和退出码
///
/// 预期结果：
///   - 正断言：code=busy、exit_code=5、消息包含 owner 10
///   - 负断言：错误不得标记 retryable，也不得降级为 internal_error
#[test]
fn test_ego_busy_error_is_structured_and_non_retryable() {
    let output = render_protocol_error(
        OutputFormat::Json,
        "request-2".to_string(),
        &CardCommandError::ego(
            EgoErrorCode::Busy,
            "Ego card is busy with agent 10; agent 20 cannot take over",
            false,
        ),
    )
    .expect("Ego busy 错误应可渲染为 JSON");
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("Ego busy 输出应是有效 JSON");

    assert_eq!(
        json["error"]["code"], "busy",
        "Ego 忙碌冲突必须返回稳定业务码 busy"
    );
    assert_eq!(
        output.exit_code, 5,
        "Ego 状态或授权类业务错误必须使用退出码 5"
    );
    assert!(
        json["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("agent 10")),
        "忙碌错误必须包含当前所有者 agent 10；实际：{json}"
    );
    assert_eq!(
        json["error"]["retryable"], false,
        "第一版没有排队或抢占，忙碌错误不得暗示自动重试"
    );
    assert_ne!(
        json["error"]["code"], "internal_error",
        "Ego 忙碌冲突不应被吞成 internal_error"
    );
}
