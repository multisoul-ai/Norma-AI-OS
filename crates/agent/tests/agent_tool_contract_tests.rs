use agent::{card_subagent_tool_specs, parse_card_subagent_tool_call};
use norma::protocol::{AgentProfile, CardCommand};

/// AT-1：CardTask ProviderRequest 应暴露四个高层 Agent 工具，并保留普通卡片工具。
///
/// 数据构造（含关键数值的推导过程）：
///   high-level Agent tools = 4：list_agents/create_agent/activate_agent/dispatch_agent_tasks
///   generic terminal/card tools 至少保留 list_cards/create_card/focus_card/send_text/delete_card
///   工具名总数下限 = 4 + 5 = 9
///
/// 执行过程（逐步说明系统如何处理）：
///   1. card_subagent_tool_specs() → 生成完整工具 schema 列表
///   2. 提取所有工具名 → 分别检查四个高层 Agent 入口
///   3. 读取 create_card schema → 确认通用创建不再宣称支持 agent kind
///
/// 预期结果：
///   - 正断言：四个高层 Agent 工具全部存在
///   - 正断言：工具总数至少为 9，普通卡片能力未被误删
///   - 负断言：create_card schema 不应包含 agent 枚举旁路
#[test]
fn card_task_exposes_high_level_agent_tools_without_generic_agent_creation() {
    let tools = card_subagent_tool_specs();
    let names = tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>();
    let create_card = tools
        .iter()
        .find(|tool| tool.name == "create_card")
        .expect("普通 terminal 创建工具仍应存在");

    assert!(
        names.contains(&"list_agents"),
        "必须暴露 list_agents 高层工具"
    );
    assert!(
        names.contains(&"create_agent"),
        "必须暴露 create_agent 高层工具"
    );
    assert!(
        names.contains(&"activate_agent"),
        "必须暴露 activate_agent 高层工具"
    );
    assert!(
        names.contains(&"dispatch_agent_tasks"),
        "必须暴露 dispatch_agent_tasks 原子委派工具"
    );
    assert!(
        tools.len() >= 9,
        "新增 4 个高层工具后仍应保留至少 5 个普通卡片工具"
    );
    assert!(
        !create_card
            .parameters_json_schema
            .to_string()
            .contains("agent"),
        "通用 create_card schema 不应继续暴露匿名 agent 创建旁路"
    );
}

/// AT-2：高层 Agent 工具参数应解析为结构化命令，且创建缺少运行时不得默认 Codex。
///
/// 数据构造（含关键数值的推导过程）：
///   create args = name Alan + profile claude + activate true，共 3 个业务字段
///   dispatch assignments = 2 项：Alan/修后端、Bella/写测试
///   missing-profile create = 仅 name 1 个字段，缺少必填 profile
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 create_agent → CardCommand::CreateAgent
///   2. 解析 dispatch_agent_tasks → 原样保留 2 项分工与 force=false
///   3. 解析缺少 profile 的 create_agent → 返回 InvalidArguments
///
/// 预期结果：
///   - 正断言：创建命令保留 Alan、Claude、activate=true
///   - 正断言：委派命令恰好保留 2 项明确分工
///   - 负断言：缺少 profile 时不能静默产生 Codex 创建命令
#[test]
fn high_level_agent_tool_calls_parse_structured_commands_and_require_profile() {
    let create = parse_card_subagent_tool_call(
        "create_agent",
        r#"{"name":"Alan","profile":"claude","activate":true}"#,
    )
    .expect("完整 create_agent 参数应解析成功");
    let dispatch = parse_card_subagent_tool_call(
        "dispatch_agent_tasks",
        r#"{"assignments":[{"name":"Alan","task":"修后端"},{"name":"Bella","task":"写测试"}]}"#,
    )
    .expect("两项明确分工应解析为原子委派命令");
    let missing_profile = parse_card_subagent_tool_call("create_agent", r#"{"name":"Alan"}"#);

    assert_eq!(
        create,
        CardCommand::CreateAgent {
            name: Some("Alan".to_string()),
            profile: AgentProfile::Claude,
            activate: true,
        },
        "create_agent 应完整保留名称、Claude profile 与立即激活意图"
    );
    let CardCommand::DispatchAgentTasks {
        assignments,
        force_untracked,
    } = dispatch
    else {
        panic!("dispatch_agent_tasks 必须解析为 DispatchAgentTasks 命令");
    };
    assert_eq!(
        assignments.len(),
        2,
        "原子委派命令应包含 Alan 与 Bella 两项明确分工"
    );
    assert_eq!(assignments[0].name, "Alan", "第一项目标必须是 Alan");
    assert_eq!(assignments[0].task, "修后端", "第一项正文必须是修后端");
    assert_eq!(assignments[1].name, "Bella", "第二项目标必须是 Bella");
    assert_eq!(assignments[1].task, "写测试", "第二项正文必须是写测试");
    assert!(!force_untracked, "未显式 force 时必须保持 false");
    assert!(
        missing_profile.is_err(),
        "缺少 runtime profile 的创建请求必须拒绝，不能默认 Codex"
    );
}
