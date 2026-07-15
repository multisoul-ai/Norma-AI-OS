pub mod support;

use agent::{
    AgentCommand, AgentProvider, AgentRosterEntry, AgentSystem, AgentTrackingMode, CardTaskBudget,
    MemoryJournal, ProviderError, ProviderRequest, ProviderResponse,
};
use norma::protocol::AgentProfile;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use support::{text_response, RecordingHost, ScriptedProvider};

struct RecordingDialogueProvider {
    responses: VecDeque<ProviderResponse>,
    requests: Arc<Mutex<Vec<ProviderRequest>>>,
}

impl AgentProvider for RecordingDialogueProvider {
    fn complete(&mut self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.requests
            .lock()
            .expect("request recording lock should not be poisoned")
            .push(request.clone());
        self.responses
            .pop_front()
            .ok_or_else(|| ProviderError::Message("recording provider exhausted".to_string()))
    }
}

/// A-P-8：每轮 Dialogue 请求应注入紧凑动态 Agent 名册，且不包含自由文本 Workflow。
///
/// 数据构造（含关键数值的推导过程）：
///   roster = 1 个 Alan：alias=艾伦、profile=Codex、enabled=true、active node=77
///   允许动态字段数 = 5：name/alias/profile/enabled/active
///   workflow marker = "SECRET_WORKFLOW"，不属于 roster 数据且不得进入 prompt
///
/// 执行过程（逐步说明系统如何处理）：
///   1. set_agent_roster([Alan]) → 更新 AgentSystem 当前项目名册
///   2. submit(UserTurn) → 组装本轮公开 ProviderRequest
///   3. 读取首条 system message → 检查紧凑字段和隐私边界
///
/// 预期结果：
///   - 正断言：system prompt 包含 Alan、艾伦、Codex、enabled、active
///   - 正断言：名册使用明确的 agent-roster 边界
///   - 负断言：prompt 不得包含 SECRET_WORKFLOW 或 workflow_description 字段
#[test]
fn dialogue_request_injects_compact_dynamic_agent_roster() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let dialogue = RecordingDialogueProvider {
        responses: VecDeque::from([text_response("收到")]),
        requests: requests.clone(),
    };
    let mut system = AgentSystem::new(
        "project-a",
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .set_agent_roster(vec![AgentRosterEntry {
            definition_id: 7,
            name: "Alan".to_string(),
            speech_alias: Some("艾伦".to_string()),
            profile: AgentProfile::Codex,
            enabled: true,
            active_node_id: Some(77),
            tracking: AgentTrackingMode::Managed,
            busy: false,
        }])
        .expect("合法唯一的 Alan 名册应成功写入 AgentSystem");

    system
        .submit(AgentCommand::UserTurn("Alan 在吗".to_string()))
        .expect("普通提及应生成 Dialogue 请求但不执行 Agent 任务");

    let recorded = requests
        .lock()
        .expect("request recording lock should not be poisoned");
    let prompt = &recorded[0].messages[0].content;
    assert!(
        prompt.contains("<agent-roster>"),
        "动态名册应有明确开始边界"
    );
    assert!(prompt.contains("name=Alan"), "动态名册应包含正式名称 Alan");
    assert!(
        prompt.contains("alias=艾伦"),
        "动态名册应包含预设语音别名艾伦"
    );
    assert!(
        prompt.contains("profile=Codex"),
        "动态名册应包含 Codex 类型"
    );
    assert!(prompt.contains("enabled=true"), "动态名册应包含启用状态");
    assert!(prompt.contains("active=true"), "动态名册应包含活跃卡片状态");
    assert!(
        !prompt.contains("SECRET_WORKFLOW") && !prompt.contains("workflow_description"),
        "动态名册不得注入 Workflow Description 或其他自由文本"
    );
}
