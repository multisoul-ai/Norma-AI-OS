use norma::cli::{
    parse_invocation_from, parse_process_invocation_from, render_protocol_error, OutputFormat,
};
use norma::protocol::{CardCommand, CardCommandError};

/// 诊断入口：doctor 与 --version 必须在 App 不可达前提供真实版本信息。
///
/// 数据构造（含关键数值的推导过程）：
///   local-only commands = doctor parse(1) + version render(1) = 2 条
///   socket connections  = 0 次（本测试只验证 CLI 前置契约）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 `norma doctor --format json` 为 Doctor 命令。
///   2. 通过 process parser 处理 `norma --version` 的 Clap DisplayVersion。
///   3. 检查 stdout/stderr 与退出码。
///
/// 预期结果：
///   - 正断言：doctor 使用独立协议命令，version 在 stdout 输出包版本且退出 0。
///   - 负断言：--version 不得进入 socket 请求，也不得作为错误写入 stderr。
#[test]
fn doctor_and_version_expose_local_diagnostic_contract() {
    let doctor = parse_invocation_from(["norma", "doctor", "--format", "json"])
        .expect("doctor 应在连接 App 前解析成功");
    let version = parse_process_invocation_from(["norma", "--version"])
        .expect_err("--version 应作为终止型本地输出返回给 binary");

    assert_eq!(
        doctor.command,
        CardCommand::Doctor,
        "doctor 必须使用独立诊断命令，不能伪装成 agent list"
    );
    assert_eq!(
        version.exit_code, 0,
        "--version 是成功的本地输出，退出码必须为 0"
    );
    assert!(
        version.stdout.contains(env!("CARGO_PKG_VERSION")),
        "--version stdout 必须包含真实 CLI 包版本"
    );
    assert!(
        version.stderr.is_empty(),
        "--version 不得作为错误写入 stderr"
    );
}

/// 副作用未知诊断：错误 JSON 必须携带原 operation ID 和显式 unknown 标记。
///
/// 数据构造（含关键数值的推导过程）：
///   operation IDs = 1 个固定 UUID
///   outcome_unknown flags = 1 个 true
///   新 operation IDs = 0 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用原 operation ID 构造副作用结果未知错误。
///   2. 将错误渲染为 Agent 默认 JSON envelope。
///   3. 解析 error 对象并读取 code、operation_id、outcome_unknown 与 retryable。
///
/// 预期结果：
///   - 正断言：错误码为 outcome_unknown，且返回原 operation ID 和 true 标记。
///   - 正断言：错误可重试，但重试身份没有变化。
///   - 负断言：输出不得生成或暗示第二个 operation ID。
#[test]
fn outcome_unknown_json_preserves_the_original_operation_identity() {
    let operation_id = "11111111-2222-4333-8444-555555555555";
    let error = CardCommandError::outcome_unknown(operation_id, "连接在收到任务回执前断开");

    let output = render_protocol_error(OutputFormat::Json, "request-unknown".to_string(), &error)
        .expect("outcome_unknown 应渲染为合法 JSON");
    let value: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("错误 stdout 应是单个 JSON 对象");
    let rendered = value
        .get("error")
        .expect("错误 envelope 必须包含 error 对象");

    assert_eq!(
        rendered.get("code").and_then(serde_json::Value::as_str),
        Some("outcome_unknown"),
        "副作用不确定必须使用稳定 outcome_unknown 错误码"
    );
    assert_eq!(
        rendered
            .get("operation_id")
            .and_then(serde_json::Value::as_str),
        Some(operation_id),
        "错误必须原样返回首次调用的 operation ID"
    );
    assert_eq!(
        rendered
            .get("outcome_unknown")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "错误必须显式声明副作用结果未知"
    );
    assert_eq!(
        rendered
            .get("retryable")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "调用方可使用相同 operation ID 查询或重放原请求"
    );
    assert_eq!(
        output.stdout.matches(operation_id).count(),
        1,
        "错误输出只能包含一个原 operation ID，不得生成第二个身份"
    );
}
