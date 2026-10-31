use norma::cli::{
    parse_invocation_from, render_protocol_error, render_protocol_success, OutputFormat,
};
use norma::protocol::{
    AgentErrorCode, AgentTaskReceiptStatus, AgentTaskResult, AgentTaskResultAvailability,
    AgentTaskStatus, CardCommand, CardCommandError, CardCommandResult,
};

/// Agent 任务结果查询：`result` 必须映射为独立的只读结果命令。
///
/// 数据构造（含关键数值的推导过程）：
///   task_ref = "runtime-a:7"，由 runtime_instance_id("runtime-a") + task_id(7) 组成
///   等待预算 = 0 秒：result 是单次查询，不进入 wait 轮询
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 `norma agent task result runtime-a:7 --format json`。
///   2. CLI 将不透明 task_ref 原样放入结果查询命令。
///   3. 检查调用没有等待预算，也没有产生取消或派发副作用。
///
/// 预期结果：
///   - 正断言：生成 `AgentTaskResult`，task_ref 精确等于 `runtime-a:7`。
///   - 正断言：输出格式为 JSON，且 wait timeout 为 None。
///   - 负断言：不得退化为 `AgentTaskStatus`，否则调用方仍无法表达结果语义。
#[test]
fn task_result_command_maps_to_distinct_result_query() {
    let invocation = parse_invocation_from([
        "norma",
        "agent",
        "task",
        "result",
        "runtime-a:7",
        "--format",
        "json",
    ])
    .expect("agent task result 应解析为公开 CLI 调用");

    assert_eq!(
        invocation.command,
        CardCommand::AgentTaskResult {
            task_ref: "runtime-a:7".to_string(),
        },
        "result 必须按不透明 task_ref 查询独立结果协议"
    );
    assert_eq!(
        invocation.wait_timeout_seconds, None,
        "result 是单次读取，不得隐式进入 wait 轮询"
    );
    assert_eq!(
        invocation.format,
        OutputFormat::Json,
        "显式 --format json 必须保留稳定机器可读输出"
    );
    assert_ne!(
        invocation.command,
        CardCommand::AgentTaskStatus {
            task_ref: "runtime-a:7".to_string(),
        },
        "result 不得退化为只返回状态的旧命令"
    );
}

/// wait 终态 JSON：完成状态必须直接携带与任务绑定的 Agent 最终回复。
///
/// 数据构造（含关键数值的推导过程）：
///   task_id = "runtime-a:7"
///   result.text = "测试通过，共 12 项"，字符数 11 < 结果上限 1200 → truncated=false
///   status = done，属于 4 个可信终态之一
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 wait 在终态收到的 `AgentTaskStatus`。
///   2. 通过 CLI 公共 JSON 输出 seam 渲染响应。
///   3. 调用方从 `result.task.result` 直接读取回复与截断标记。
///
/// 预期结果：
///   - 正断言：JSON 同时保留 done 状态和完整结果正文。
///   - 正断言：结果 availability=available 且 truncated=false。
///   - 负断言：结果不得退化为顶层自然语言字符串或缺失 result 字段。
#[test]
fn terminal_wait_json_includes_bound_agent_result() {
    let response = CardCommandResult::AgentTaskStatus {
        task: AgentTaskStatus {
            task_id: "runtime-a:7".to_string(),
            target_definition_id: 2,
            target_name: "Bella".to_string(),
            target_node_id: 102,
            status: AgentTaskReceiptStatus::Done,
            tracking: "managed".to_string(),
            parent_task_id: None,
            root_task_id: "runtime-a:7".to_string(),
            result: Some(AgentTaskResult {
                availability: AgentTaskResultAvailability::Available,
                text: Some("测试通过，共 12 项".to_string()),
                truncated: false,
                background_unsettled: false,
            }),
        },
    };

    let output = render_protocol_success(OutputFormat::Json, "request-wait".to_string(), &response)
        .expect("带任务结果的终态状态应可序列化");
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("wait 成功输出必须是合法 JSON");

    assert_eq!(
        json["result"]["task"]["status"], "done",
        "wait 必须保留可信 done 终态"
    );
    assert_eq!(
        json["result"]["task"]["result"]["text"], "测试通过，共 12 项",
        "wait 终态必须直接返回与 task_id 绑定的 Agent 回复"
    );
    assert_eq!(
        json["result"]["task"]["result"]["availability"], "available",
        "存在完整回复时 availability 必须为 available"
    );
    assert_eq!(
        json["result"]["task"]["result"]["truncated"], false,
        "短回复不得错误标记为已截断"
    );
    assert!(
        json["result"]["task"].get("result").is_some(),
        "终态 wait 不得继续只返回状态而缺失 result"
    );
    assert!(
        json.get("message").is_none(),
        "任务结果不得退化为协议外的顶层自然语言 message"
    );
}

/// result 成功 JSON：独立查询必须保留与 wait 相同的稳定 task/result 结构。
///
/// 数据构造（含关键数值的推导过程）：
///   task_id = "runtime-a:9"，status = done
///   result.text = "结果已生成"，5 个字符 < 1200 字符上限
///   result 字段数 = availability + text + truncated = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造独立 `AgentTaskResult` 协议响应。
///   2. 通过 CLI JSON 输出 seam 渲染成功 envelope。
///   3. 从 `result.task.result` 读取任务绑定结果。
///
/// 预期结果：
///   - 正断言：task_id、done、available、text 和 truncated 均稳定输出。
///   - 负断言：不得使用顶层 message，也不得丢失 task 包装层。
#[test]
fn task_result_json_uses_the_same_stable_task_result_shape() {
    let response = CardCommandResult::AgentTaskResult {
        task: AgentTaskStatus {
            task_id: "runtime-a:9".to_string(),
            target_definition_id: 3,
            target_name: "Cora".to_string(),
            target_node_id: 109,
            status: AgentTaskReceiptStatus::Done,
            tracking: "managed".to_string(),
            parent_task_id: None,
            root_task_id: "runtime-a:9".to_string(),
            result: Some(AgentTaskResult {
                availability: AgentTaskResultAvailability::Available,
                text: Some("结果已生成".to_string()),
                truncated: false,
                background_unsettled: false,
            }),
        },
    };

    let output = render_protocol_success(
        OutputFormat::Json,
        "request-result-9".to_string(),
        &response,
    )
    .expect("独立 result 响应应可序列化");
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("result 成功输出必须是合法 JSON");

    assert_eq!(
        json["result"]["task"]["task_id"], "runtime-a:9",
        "result 必须保留调用方查询的精确 task_id"
    );
    assert_eq!(
        json["result"]["task"]["status"], "done",
        "成功读取结果时仍必须保留任务终态"
    );
    assert_eq!(
        json["result"]["task"]["result"]["availability"], "available",
        "可信最终回复必须标记 available"
    );
    assert_eq!(
        json["result"]["task"]["result"]["text"], "结果已生成",
        "独立 result 必须输出任务绑定正文"
    );
    assert_eq!(
        json["result"]["task"]["result"]["truncated"], false,
        "短结果不得错误标记为截断"
    );
    assert!(
        json["result"].get("task").is_some(),
        "result 成功 envelope 不得丢失稳定 task 包装层"
    );
    assert!(
        json.get("message").is_none(),
        "独立结果不得退化为协议外顶层 message"
    );
}

/// result 未就绪错误：非终态查询必须提供稳定、可重试且带当前状态的 JSON。
///
/// 数据构造（含关键数值的推导过程）：
///   task status = submitted，终态集合大小 = 4（done/failed/cancelled/interrupted）
///   submitted ∉ 终态集合 → result 尚不可读
///   retryable = true，退出码使用一般领域错误码 5
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 `TaskResultNotReady`，并附带当前 submitted 状态。
///   2. 通过公开 JSON 错误输出 seam 渲染 CLI 响应。
///   3. 调用方读取稳定 code、retryable 与 current_status 决定稍后重试。
///
/// 预期结果：
///   - 正断言：code=task_result_not_ready、retryable=true、current_status=submitted。
///   - 正断言：退出码为 5 且 stderr 为空。
///   - 负断言：未就绪不得伪装成 task_not_found 或 wait_timeout。
#[test]
fn task_result_not_ready_is_explicit_and_retryable() {
    let error = CardCommandError::agent_with_status(
        AgentErrorCode::TaskResultNotReady,
        "任务尚未进入终态，结果暂不可用",
        true,
        AgentTaskReceiptStatus::Submitted,
    );

    let output = render_protocol_error(OutputFormat::Json, "request-result".to_string(), &error)
        .expect("未就绪错误应可序列化为稳定 JSON");
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("result 错误 stdout 必须是合法 JSON");

    assert_eq!(
        json["error"]["code"], "task_result_not_ready",
        "非终态 result 查询必须返回专用稳定错误码"
    );
    assert_eq!(
        json["error"]["retryable"], true,
        "任务仍在执行时调用方必须可以稍后重试 result"
    );
    assert_eq!(
        json["error"]["current_status"], "submitted",
        "错误必须携带真实当前状态，不能要求调用方另行猜测"
    );
    assert_eq!(
        output.exit_code, 5,
        "result 未就绪应使用一般领域错误退出码 5"
    );
    assert!(
        output.stderr.is_empty(),
        "JSON 错误不得把自然语言诊断重复写入 stderr"
    );
    assert_ne!(
        json["error"]["code"], "task_not_found",
        "已存在但未完成的任务不得误报为不存在"
    );
    assert_ne!(
        json["error"]["code"], "wait_timeout",
        "单次 result 查询没有等待预算，不得误报 wait timeout"
    );
}
