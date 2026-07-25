use norma::cli::{
    parse_invocation_from, render_protocol_error, render_protocol_success, OutputFormat,
};
use norma::protocol::{
    AgentErrorCode, AgentProfile, AgentTaskReceipt, AgentTaskReceiptStatus, CardCommand,
    CardCommandError, CardCommandResult,
};

/// Agent CLI 命令树：list/assign/status/wait/cancel 应映射为窄命令且默认 JSON。
///
/// 数据构造（含关键数值的推导过程）：
///   命令数 = 5：list、assign、status、wait、cancel
///   assign 目标 = Bella，任务正文 = "运行测试"
///   wait timeout = 未显式传入 → 默认 60 秒
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别解析五条 `norma agent ...` argv。
///   2. 检查每条 argv 只生成一个对应 CardCommand。
///   3. 检查 Agent 命令未指定 `--format` 时统一选择 JSON。
///
/// 预期结果：
///   - 正断言：五条命令映射到 ListAgents/Dispatch/Status/Status/Cancel。
///   - 正断言：wait 保留默认 60 秒的本次调用等待策略。
///   - 负断言：assign 不得生成多目标 assignments，也不得隐式 force。
#[test]
fn agent_command_tree_parses_narrow_commands_with_json_default() {
    let list = parse_invocation_from(["norma", "agent", "list"])
        .expect("agent list 应解析为公开 CLI 调用");
    let assign = parse_invocation_from([
        "norma",
        "agent",
        "task",
        "assign",
        "Bella",
        "--task",
        "运行测试",
    ])
    .expect("agent task assign 应解析单目标与单任务");
    let status = parse_invocation_from(["norma", "agent", "task", "status", "runtime-a:7"])
        .expect("agent task status 应解析 task_ref");
    let wait = parse_invocation_from(["norma", "agent", "task", "wait", "runtime-a:7"])
        .expect("agent task wait 应使用默认 timeout");
    let cancel = parse_invocation_from(["norma", "agent", "task", "cancel", "runtime-a:7"])
        .expect("agent task cancel 应解析 task_ref");

    assert_eq!(
        list.command,
        CardCommand::ListAgents,
        "agent list 必须映射到现有高层 ListAgents 命令"
    );
    let CardCommand::DispatchAgentTasks {
        assignments,
        force_untracked,
    } = assign.command
    else {
        panic!("agent task assign 必须映射到 DispatchAgentTasks");
    };
    assert_eq!(
        assignments.len(),
        1,
        "一次 assign 只能携带一个目标和一个任务"
    );
    assert_eq!(
        assignments[0].name, "Bella",
        "assign 必须保留用户明确给出的目标呼叫名"
    );
    assert_eq!(
        assignments[0].task, "运行测试",
        "assign 必须保留用户明确给出的任务正文"
    );
    assert!(
        !force_untracked,
        "未取得额外授权时 CLI 不得隐式启用 --force-untracked"
    );
    assert_eq!(
        status.command,
        CardCommand::AgentTaskStatus {
            task_ref: "runtime-a:7".to_string(),
        },
        "status 必须按不透明 task_ref 查询精确任务"
    );
    assert_eq!(
        wait.command, status.command,
        "wait 的服务端查询命令应复用精确 status，等待策略留在 CLI 客户端"
    );
    assert_eq!(
        wait.wait_timeout_seconds,
        Some(60),
        "未指定 --timeout 时 wait 必须仅为本次调用使用默认 60 秒"
    );
    assert_eq!(
        cancel.command,
        CardCommand::AgentTaskCancel {
            task_ref: "runtime-a:7".to_string(),
        },
        "cancel 必须按单个不透明 task_ref 精确取消"
    );
    assert_eq!(
        list.format,
        OutputFormat::Json,
        "所有 Agent 命令默认输出必须是 JSON"
    );
    assert_eq!(
        cancel.format,
        OutputFormat::Json,
        "cancel 未显式指定 format 时不得回退到自然语言输出"
    );
}

/// Agent CLI 参数边界：一次 assign 不得接收第二个目标或空任务正文。
///
/// 数据构造（含关键数值的推导过程）：
///   非法目标数 = 2：Bella + Cora；协议上限 = 1
///   空白任务 = "   "，trim 后长度 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析带两个位置目标的 assign。
///   2. 解析只含空白字符的 `--task`。
///   3. 在连接 App 之前由 CLI 参数层拒绝两者。
///
/// 预期结果：
///   - 正断言：两个非法调用均返回 clap 错误。
///   - 负断言：不得产生 DispatchAgentTasks 命令或依赖服务端猜测。
#[test]
fn assign_rejects_multiple_targets_and_blank_task_before_transport() {
    let multiple = parse_invocation_from([
        "norma",
        "agent",
        "task",
        "assign",
        "Bella",
        "Cora",
        "--task",
        "运行测试",
    ]);
    let blank =
        parse_invocation_from(["norma", "agent", "task", "assign", "Bella", "--task", "   "]);

    assert!(
        multiple.is_err(),
        "assign 出现第二个目标时必须在 CLI 解析层拒绝"
    );
    assert!(
        blank.is_err(),
        "assign 的任务正文 trim 后为空时必须在 CLI 解析层拒绝"
    );
}

/// JSON 成功输出契约：stdout 只含 schema/request/result，且任务回执不泄露 Token。
///
/// 数据构造（含关键数值的推导过程）：
///   schema_version = 1
///   request_id = "request-42"
///   task_id = "runtime-a:7"，status=queued
///   敏感哨兵 = "secret-provider-token"（不得出现）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一条真实 AgentTasksDispatched 回执。
///   2. 以 JSON format 渲染 CLI 成功结果。
///   3. 解析 stdout 并检查 schema、ok、request_id、result。
///
/// 预期结果：
///   - 正断言：stdout 是单个合法 JSON 对象，stderr 为空，exit_code=0。
///   - 正断言：result 中保留 task_id、target_name、target_node_id 与 queued 状态。
///   - 负断言：不得输出含糊的 name/node_id 旧键；stdout/stderr 均不得出现 Token 哨兵。
#[test]
fn json_success_output_has_stable_envelope_and_no_diagnostics_or_token() {
    let result = CardCommandResult::AgentTasksDispatched {
        receipts: vec![AgentTaskReceipt {
            task_id: "runtime-a:7".to_string(),
            target_definition_id: 2,
            name: "Bella".to_string(),
            node_id: 102,
            task: "运行测试".to_string(),
            status: AgentTaskReceiptStatus::Queued,
            tracking: "managed".to_string(),
            parent_task_id: Some("runtime-a:1".to_string()),
            root_task_id: "runtime-a:1".to_string(),
        }],
    };

    let output = render_protocol_success(OutputFormat::Json, "request-42".to_string(), &result)
        .expect("协议结果应可序列化为 JSON");
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("stdout 必须只包含合法 JSON");

    assert_eq!(
        json["schema_version"], 1,
        "Agent CLI JSON schema_version 首版必须固定为 1"
    );
    assert_eq!(json["ok"], true, "成功输出必须显式标记 ok=true");
    assert_eq!(
        json["request_id"], "request-42",
        "成功输出必须保留本次 request_id"
    );
    assert_eq!(
        json["result"]["receipts"][0]["task_id"], "runtime-a:7",
        "成功派发必须返回可继续查询的唯一 task_id"
    );
    assert_eq!(
        json["result"]["receipts"][0]["status"], "queued",
        "忙碌目标的真实接收状态必须输出 queued"
    );
    assert_eq!(
        json["result"]["receipts"][0]["target_name"], "Bella",
        "assign JSON 必须使用稳定字段 target_name 标识目标 Agent"
    );
    assert_eq!(
        json["result"]["receipts"][0]["target_node_id"], 102,
        "assign JSON 必须使用稳定字段 target_node_id 标识目标卡片"
    );
    assert!(
        json["result"]["receipts"][0].get("name").is_none(),
        "assign JSON 不得输出语义含糊的旧键 name"
    );
    assert!(
        json["result"]["receipts"][0].get("node_id").is_none(),
        "assign JSON 不得输出语义含糊的旧键 node_id"
    );
    assert_eq!(output.exit_code, 0, "成功协议输出的进程退出码必须为 0");
    assert!(
        output.stderr.is_empty(),
        "JSON 协议成功时 stderr 不应混入普通诊断"
    );
    assert!(
        !output.stdout.contains("secret-provider-token"),
        "JSON stdout 不得暴露 Provider Token"
    );
    assert!(
        !output.stderr.contains("secret-provider-token"),
        "诊断 stderr 不得暴露 Provider Token"
    );
}

/// JSON 错误输出契约：稳定错误码进入 stdout，退出码按错误类别稳定映射。
///
/// 数据构造（含关键数值的推导过程）：
///   error.code = unauthorized
///   retryable = false
///   request_id = "request-unauthorized"
///   Provider = Codex（只用于构造错误消息，不输出 Token）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造未授权 Provider 的结构化 CardCommandError。
///   2. 以 JSON format 渲染错误结果。
///   3. 解析 stdout 并检查稳定 code/message/retryable 与退出码。
///
/// 预期结果：
///   - 正断言：stdout 为 ok=false 且 code=unauthorized。
///   - 正断言：授权错误退出码固定为 4。
///   - 负断言：stderr 不得重复自然语言错误，code 不得退化为 internal_error。
#[test]
fn json_error_output_preserves_stable_error_code_and_exit_code() {
    let error = CardCommandError::agent(
        AgentErrorCode::Unauthorized,
        format!("{:?} 编排授权无效", AgentProfile::Codex),
        false,
    );

    let output = render_protocol_error(
        OutputFormat::Json,
        "request-unauthorized".to_string(),
        &error,
    )
    .expect("结构化错误应可序列化为 JSON");
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("错误 stdout 必须只包含合法 JSON");

    assert_eq!(json["ok"], false, "错误协议输出必须显式标记 ok=false");
    assert_eq!(
        json["error"]["code"], "unauthorized",
        "未授权错误必须保留稳定 code=unauthorized"
    );
    assert_eq!(
        json["error"]["retryable"], false,
        "长期 Token 不匹配不是瞬时错误，不应标记 retryable"
    );
    assert_eq!(output.exit_code, 4, "授权错误必须映射到稳定的进程退出码 4");
    assert!(
        output.stderr.is_empty(),
        "JSON 协议错误不得在 stderr 重复输出自然语言错误"
    );
    assert_ne!(
        json["error"]["code"], "internal_error",
        "已知授权失败不得退化为 internal_error"
    );
}
