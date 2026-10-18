use canvas::AgentProfile;
use storage::{AgentDefinitionStore, NewAgentDefinition};

/// Kimi、ZCode、Pi、Hermes Agent 定义：Storage 必须以稳定小写值完成创建和读取往返。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions 初始数量 = 0
///   依次创建 4 条定义（Kimi / ZCode / Pi / Hermes），数量 = 0 + 4 = 4
///   数据库稳定值：`kimi`、`zcode`、`pi`、`hermes`
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建临时项目数据库和 `AgentDefinitionStore`
///   2. 为每个新增 profile 创建一条定义
///   3. 通过公开 `list` 接口重新读取全部定义
///
/// 预期结果：
///   - 正断言：恰好读回 4 条，且每条 profile 与创建时一致
///   - 负断言：任何一条都不得被解码为 Grok 或 Codex
#[test]
fn new_cli_agent_definitions_round_trip_through_storage() {
    let temp = tempfile::tempdir().expect("测试应创建临时项目目录");
    let store = AgentDefinitionStore::new(temp.path().join("project.sqlite"));
    let profiles = [
        ("KimiBuilder", AgentProfile::Kimi),
        ("ZCodeBuilder", AgentProfile::ZCode),
        ("PiBuilder", AgentProfile::Pi),
        ("HermesBuilder", AgentProfile::Hermes),
    ];

    for (name, profile) in profiles {
        store
            .create(&NewAgentDefinition {
                name: name.to_string(),
                runtime_profile: profile,
                workflow_description: format!("Use {name} for coding tasks."),
            })
            .unwrap_or_else(|error| panic!("{name} 定义应成功写入 SQLite: {error}"));
    }
    let definitions = store.list().expect("新增 Agent 定义应能从 SQLite 读回");

    assert_eq!(
        definitions.len(),
        4,
        "创建 4 条新增 Agent 定义后必须恰好读回 4 条"
    );
    for (name, profile) in profiles {
        let found = definitions
            .iter()
            .find(|definition| definition.name == name)
            .unwrap_or_else(|| panic!("读回结果应包含定义 {name}"));
        assert_eq!(
            found.runtime_profile, profile,
            "{name} 的 runtime_profile 必须完整往返"
        );
        assert_ne!(
            found.runtime_profile,
            AgentProfile::Grok,
            "{name} 不得错误解码为 Grok"
        );
        assert_ne!(
            found.runtime_profile,
            AgentProfile::Codex,
            "{name} 不得错误解码为 Codex"
        );
    }
}
