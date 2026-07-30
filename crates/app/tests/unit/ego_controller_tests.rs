use super::*;
use std::os::unix::fs::PermissionsExt;

/// Ego helper 成功回执：即使官方 `cliLog` 把带前缀结果写到 stderr，也必须完成 Task Space 绑定。
///
/// 数据构造（含关键数值的推导过程）：
///   helper 退出码 = 0（成功）
///   stdout 行数 = 1：无前缀干扰 JSON，task_space_id="99"
///   stderr 行数 = 1：可信前缀结果，task_space_id="5"
///   候选结果 = stdout(1) + stderr(1) = 2 行，但只有 stderr 行带可信前缀
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动真实临时 helper 进程，并向其 stdin 写入占位脚本。
///   2. helper 成功退出；普通 JSON 写 stdout，`NORMA_EGO_RESULT` 回执写 stderr。
///   3. 将 helper 成功输出交给 Task Space 结果解析器。
///
/// 预期结果：
///   - 正断言：解析器读取 stderr 中的精确 ID=5 和名称 `Norma · Ego`。
///   - 负断言：stdout 中无前缀的 ID=99 不得成为绑定结果。
#[test]
fn test_successful_helper_result_on_stderr_is_parsed() {
    let fixture_dir = tempfile::tempdir().expect("测试必须创建隔离的 helper fixture 目录");
    let helper = fixture_dir.path().join("ego-browser-fixture");
    std::fs::write(
        &helper,
        "#!/bin/sh\n\
         /bin/cat >/dev/null\n\
         printf '%s\\n' '{\"task_space_id\":\"99\",\"task_space_name\":\"普通日志\"}'\n\
         printf '%s\\n' 'NORMA_EGO_RESULT={\"task_space_id\":\"5\",\"task_space_name\":\"Norma · Ego\"}' >&2\n",
    )
    .expect("测试必须写入可执行的 helper fixture");
    let mut permissions = std::fs::metadata(&helper)
        .expect("测试必须读取 helper fixture 权限")
        .permissions();
    // executable mode = owner(7) + group(5) + other(5) = 0o755。
    permissions.set_mode(0o755);
    std::fs::set_permissions(&helper, permissions).expect("测试必须把 helper fixture 标记为可执行");

    let output =
        run_helper(&helper, "fixture script").expect("退出码为 0 的 helper 必须返回可解析输出");
    let parsed =
        parse_task_space_result(&output).expect("stderr 中带可信前缀的成功回执必须可被解析");

    assert_eq!(
        parsed.task_space_id, "5",
        "卡片必须绑定 stderr 可信回执中的精确 Task Space ID=5"
    );
    assert_eq!(
        parsed.task_space_name, "Norma · Ego",
        "卡片必须保留 stderr 可信回执中的完整 Task Space 名称"
    );
    assert_ne!(
        parsed.task_space_id, "99",
        "stdout 中无前缀的普通 JSON 不得覆盖可信 Task Space 回执"
    );
}

/// Ego helper 输出解析：只接受带 Norma 前缀的精确 Task Space 结果。
///
/// 数据构造（含关键数值的推导过程）：
///   输出行数=3：helper 日志 1 行 + 结果 1 行 + 尾日志 1 行
///   task_space_id="17"，task_space_name="Norma · Ego · 42"
///   无前缀 JSON 1 行作为负例
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析包含 NORMA_EGO_RESULT 前缀的多行输出
///   2. 读取精确 ID 和名称
///   3. 解析无前缀输出并确认拒绝
///
/// 预期结果：
///   - 正断言：结果保留 ID=17 和完整名称
///   - 负断言：不得把普通日志中的任意 JSON 当作 Task Space
#[test]
fn test_parse_task_space_result_requires_exact_norma_prefix() {
    let stdout = "ego helper started\nNORMA_EGO_RESULT={\"task_space_id\":\"17\",\"task_space_name\":\"Norma · Ego · 42\"}\ndone\n";
    let parsed = parse_task_space_result(stdout).expect("带 Norma 前缀的结果应解析成功");
    let untrusted = parse_task_space_result(
        "{\"task_space_id\":\"99\",\"task_space_name\":\"用户日常空间\"}\n",
    );

    assert_eq!(
        parsed.task_space_id, "17",
        "解析器必须保留精确 Task Space ID=17"
    );
    assert_eq!(
        parsed.task_space_name, "Norma · Ego · 42",
        "解析器必须保留完整专用空间名称"
    );
    assert!(
        untrusted.is_err(),
        "无 Norma 前缀的任意 JSON 不得被当成专用 Task Space"
    );
    assert_ne!(
        parsed.task_space_id, "99",
        "解析结果不得来自无前缀的用户日常空间"
    );
}

/// Ego Task Space 清理守卫：只有仍属于 Agent 的已完成空间可被替换，用户接管空间必须保留。
///
/// 数据构造（含关键数值的推导过程）：
///   所有权结果 = agent(1) + user(1) + missing(1) = 3
///   可关闭结果 = agent(1)；拒绝结果 = user(1) + missing(1) = 2
///   目标 Task Space ID = "space-9"，三种结果均指向同一 ID 以隔离所有权变量
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将 helper 返回的 agent 所有权传入清理前验证。
///   2. 将 user 所有权和不存在结果分别传入相同验证。
///   3. 检查只有 agent 结果可继续到独占的 completeTaskSpace 调用。
///
/// 预期结果：
///   - 正断言：agent 所有权允许替换。
///   - 正断言：user/missing 错误都保留具体 Task Space ID，便于安全提示。
///   - 负断言：user 所有权不得被当成 agent；missing 不得被当成可关闭空间。
#[test]
fn test_task_space_cleanup_rejects_user_owned_or_missing_space() {
    let agent = EgoTaskSpaceOwnershipResult {
        found: true,
        ownership: Some("agent".to_string()),
    };
    let user = EgoTaskSpaceOwnershipResult {
        found: true,
        ownership: Some("user".to_string()),
    };
    let missing = EgoTaskSpaceOwnershipResult {
        found: false,
        ownership: None,
    };

    let agent_result = validate_agent_owned_task_space("space-9", &agent);
    let user_error = validate_agent_owned_task_space("space-9", &user)
        .expect_err("用户接管的 Task Space 必须拒绝自动关闭");
    let missing_error = validate_agent_owned_task_space("space-9", &missing)
        .expect_err("不存在的 Task Space 必须拒绝自动关闭");

    assert_eq!(
        agent_result,
        Ok(()),
        "仅 ownership=agent 的 Task Space 才能进入替换清理"
    );
    assert!(
        user_error.contains("user") && user_error.contains("space-9"),
        "用户所有权错误必须同时说明 user 与目标 Task Space ID，实际：{user_error}"
    );
    assert!(
        missing_error.contains("space-9"),
        "不存在错误必须指出无法关闭的精确 Task Space ID，实际：{missing_error}"
    );
    assert_ne!(
        agent_result,
        Err(user_error.clone()),
        "agent 所有权不得与 user 所有权走同一拒绝路径"
    );
    assert_ne!(
        agent_result,
        Err(missing_error.clone()),
        "agent 所有权不得与 missing Task Space 走同一拒绝路径"
    );
}
