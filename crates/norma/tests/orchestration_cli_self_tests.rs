use norma::cli::{parse_invocation_from, render_agent_self_success, OutputFormat};
use norma::cli_runtime::{project_self, AgentSelfInfo, ProjectSelfError};
use norma::protocol::{AgentProfile, AgentSummary, CardCommand};

fn agent_summary(definition_id: i64, name: &str, active_node_id: Option<u64>) -> AgentSummary {
    AgentSummary {
        definition_id,
        name: name.to_string(),
        speech_alias: None,
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id,
        busy: false,
        tracking: "managed".to_string(),
    }
}

/// CLI-SELF-1：`agent self` 必须复用 ListAgents 并标记 self 投影，不接受跨 Workspace 目标。
///
/// 数据构造（含关键数值的推导过程）：
///   argv 数 = 3（norma agent self）
///   默认格式 = JSON
///   target_workspace_id = 无
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 `norma agent self` argv。
///   2. 检查生成的 command 与 project_self 标记。
///
/// 预期结果：
///   - 正断言：command 为 ListAgents（不新增 wire command）。
///   - 正断言：project_self=true 且默认 JSON。
///   - 负断言：target_workspace_id 为 None（self 不做跨 Workspace 名称比较）。
#[test]
fn agent_self_parses_to_list_agents_with_self_projection() {
    let invocation = parse_invocation_from(["norma", "agent", "self"])
        .expect("agent self 应解析为公开 CLI 调用");
    assert_eq!(
        invocation.command,
        CardCommand::ListAgents,
        "agent self 必须复用现有 ListAgents，不新增 wire command"
    );
    assert!(
        invocation.project_self,
        "agent self 必须标记 self 投影，区别于普通 agent list"
    );
    assert_eq!(
        invocation.format,
        OutputFormat::Json,
        "agent 子命令未指定 --format 时必须默认 JSON"
    );
    assert!(
        invocation.target_workspace_id.is_none(),
        "agent self 不接受跨 Workspace 目标，只能识别当前 Session"
    );
}

/// CLI-SELF-2：roster 中恰好 1 个活跃节点命中时，必须投影出该 Agent 的完整身份。
///
/// 数据构造（含关键数值的推导过程）：
///   roster agents = 2（浩宇 active_node=7、老乔 无卡片）
///   命中 node_id = 7
///   workspace_id = "workspace-a"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造含唯一 active_node_id=7 的 roster。
///   2. 调用纯投影函数 project_self。
///   3. 校验返回的自我身份字段。
///
/// 预期结果：
///   - 正断言：definition_id、node_id、name 与 workspace_id 与输入一致。
///   - 负断言：不得把无卡片的第二个 Agent 误判为自己。
#[test]
fn project_self_matches_unique_active_node() {
    let agents = vec![
        agent_summary(10, "浩宇", Some(7)),
        agent_summary(11, "老乔", None),
    ];
    let info =
        project_self(&agents, 7, "workspace-a").expect("唯一 active_node_id=7 必须投影出自我身份");
    assert_eq!(
        info.definition_id, 10,
        "自我身份 definition_id 必须来自命中的 Agent"
    );
    assert_eq!(info.node_id, 7, "自我身份 node_id 必须与进程 node 一致");
    assert_eq!(info.name, "浩宇", "自我身份必须使用命中的规范名称");
    assert_eq!(
        info.workspace_id, "workspace-a",
        "自我身份必须携带 Workspace 作用域"
    );
}

/// CLI-SELF-3：roster 找不到当前 node 时不得按名称猜测，必须返回确定性 NotFound。
///
/// 数据构造（含关键数值的推导过程）：
///   roster agents = 1（浩宇 active_node=7）
///   查询 node_id = 999（不存在）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用不存在 node 调用 project_self。
///   2. 校验返回错误。
///
/// 预期结果：
///   - 负断言：返回 ProjectSelfError::NotFound，绝不猜测名称。
#[test]
fn project_self_rejects_unknown_node() {
    let agents = vec![agent_summary(10, "浩宇", Some(7))];
    assert_eq!(
        project_self(&agents, 999, "workspace-a"),
        Err(ProjectSelfError::NotFound),
        "未找到当前 node 时不得按名称猜测身份"
    );
}

/// CLI-SELF-4：两个活跃节点命中同一 node 时身份无法唯一确定，必须返回 Ambiguous。
///
/// 数据构造（含关键数值的推导过程）：
///   roster agents = 2（浩宇 active_node=7、老乔 active_node=7）
///   查询 node_id = 7
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用重复 active_node 的 roster 调用 project_self。
///   2. 校验返回错误。
///
/// 预期结果：
///   - 负断言：返回 ProjectSelfError::Ambiguous，不得选择任意一个。
#[test]
fn project_self_rejects_ambiguous_active_nodes() {
    let agents = vec![
        agent_summary(10, "浩宇", Some(7)),
        agent_summary(11, "老乔", Some(7)),
    ];
    assert_eq!(
        project_self(&agents, 7, "workspace-a"),
        Err(ProjectSelfError::Ambiguous),
        "多个活跃节点命中同一 node 必须报内部一致性错误"
    );
}

/// CLI-SELF-5：self JSON 输出契约：只含单一 self，不输出整个 roster，不泄露 token。
///
/// 数据构造（含关键数值的推导过程）：
///   AgentSelfInfo 字段 = 6 项
///   profile = Codex → JSON 序列化为 "codex"
///   workspace_id = "workspace-a"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 AgentSelfInfo。
///   2. 以 JSON 格式渲染 self 成功输出。
///   3. 解析 stdout JSON 并校验字段。
///
/// 预期结果：
///   - 正断言：ok=true，result.self 字段完整且 profile 为 "codex"。
///   - 负断言：输出不得包含 agents 列表或 token 字段。
#[test]
fn agent_self_json_contract_has_single_self_and_no_roster_or_token() {
    let info = AgentSelfInfo {
        workspace_id: "workspace-a".to_string(),
        definition_id: 10,
        node_id: 7,
        name: "浩宇".to_string(),
        speech_alias: Some("浩宇".to_string()),
        profile: AgentProfile::Codex,
    };
    let output = render_agent_self_success(OutputFormat::Json, "req-1".to_string(), &info)
        .expect("self JSON 渲染应成功");
    let value: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("self JSON 必须可解析");
    assert_eq!(value["ok"], true, "self 成功输出 ok 必须为 true");
    assert_eq!(
        value["result"]["self"]["name"], "浩宇",
        "result.self 必须包含规范名"
    );
    assert_eq!(
        value["result"]["self"]["definition_id"], 10,
        "result.self 必须包含 definition_id"
    );
    assert_eq!(
        value["result"]["self"]["profile"], "codex",
        "result.self profile 必须序列化为 snake_case 标签"
    );
    assert!(
        !output.stdout.contains("\"agents\""),
        "self 输出不得附带整个 roster 列表"
    );
    assert!(
        !output.stdout.to_lowercase().contains("token"),
        "self 输出不得泄露任何 token"
    );
}
