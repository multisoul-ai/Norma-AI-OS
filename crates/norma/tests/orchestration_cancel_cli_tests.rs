use norma::cli::parse_invocation_from;
use norma::protocol::CardCommand;

/// 取消幂等参数：调用方必须能复用首次取消的 operation ID，空白值要在传输前拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   cancel commands = valid(1) + blank operation ID(1) = 2 条
///   explicit operation IDs = 1 个固定 UUID
///   transport attempts = 0 次（本测试只解析 argv）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析带固定 `--operation-id` 的 cancel。
///   2. 检查调用元数据保留原 ID，供 outcome_unknown 后重放。
///   3. 解析只含空白字符的 operation ID，并在 CLI 参数层拒绝。
///
/// 预期结果：
///   - 正断言：合法 cancel 保留固定 operation ID 和精确 task ref。
///   - 负断言：空白 operation ID 不得生成新 ID，也不得进入 App 传输。
#[test]
fn cancel_accepts_replayable_operation_id_and_rejects_blank_identity() {
    let cancel = parse_invocation_from([
        "norma",
        "agent",
        "task",
        "cancel",
        "task-7",
        "--operation-id",
        "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
    ])
    .expect("显式 operation ID 的 cancel 应解析成功");
    let blank = parse_invocation_from([
        "norma",
        "agent",
        "task",
        "cancel",
        "task-7",
        "--operation-id",
        "   ",
    ]);

    assert_eq!(
        cancel.command,
        CardCommand::AgentTaskCancel {
            task_ref: "task-7".to_string(),
        },
        "cancel 必须继续精确作用于原 global task ref"
    );
    assert_eq!(
        cancel.operation_id.as_deref(),
        Some("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"),
        "取消结果未知时必须能用调用方保留的 operation ID 安全重放"
    );
    assert!(
        blank.is_err(),
        "空白 operation ID 必须在 CLI 参数层拒绝，不能自动换号后执行取消"
    );
}
