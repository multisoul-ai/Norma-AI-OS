use canvas::AgentProfile;
use storage::{AgentDefinitionStore, NewAgentDefinition};

/// Grok Agent 定义：Storage 必须以稳定 `grok` 值完成创建和读取往返。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions 初始数量 = 0
///   create 后数量 = 0 + 1 = 1
///   runtime_profile = Grok，对应数据库稳定值 `grok`
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建临时项目数据库和 `AgentDefinitionStore`
///   2. 创建名为 `GrokBuilder`、profile=Grok 的定义
///   3. 通过公开 `list` 接口重新读取定义
///
/// 预期结果：
///   - 正断言：恰好读回 1 条定义且 profile 仍为 Grok
///   - 正断言：定义名称仍为 `GrokBuilder`
///   - 负断言：不得错误解码为 Codex
#[test]
fn grok_agent_definition_round_trips_through_storage() {
    let temp = tempfile::tempdir().expect("测试应创建临时项目目录");
    let store = AgentDefinitionStore::new(temp.path().join("project.sqlite"));

    store
        .create(&NewAgentDefinition {
            name: "GrokBuilder".to_string(),
            runtime_profile: AgentProfile::Grok,
            workflow_description: "Use Grok Build for coding tasks.".to_string(),
        })
        .expect("Grok Agent 定义应成功写入 SQLite");
    let definitions = store.list().expect("Grok Agent 定义应能从 SQLite 读回");

    assert_eq!(
        definitions.len(),
        1,
        "创建 1 条 Grok Agent 定义后必须恰好读回 1 条"
    );
    assert_eq!(
        definitions[0].name, "GrokBuilder",
        "Grok Agent 定义应保留用户可见名称 GrokBuilder"
    );
    assert_eq!(
        definitions[0].runtime_profile,
        AgentProfile::Grok,
        "Grok Agent 定义的 runtime_profile 必须完整往返"
    );
    assert_ne!(
        definitions[0].runtime_profile,
        AgentProfile::Codex,
        "Grok Agent 定义不得错误解码为 Codex"
    );
}
