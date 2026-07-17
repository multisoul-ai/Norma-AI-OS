pub mod support;

use agent::{
    AgentCommand, AgentProvider, AgentSystem, CanvasCardContext, CanvasCardKind, CardTaskBudget,
    CurrentCanvasContext, MemoryJournal, ProviderError, ProviderRequest, ProviderResponse,
};
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

/// CC-1：Dialogue 请求应注入当前项目和所有卡片的轻量摘要。
///
/// 数据构造（含关键数值的推导过程）：
///   project       = norma-ai-os，workspace root=/workspace/norma-ai-os
///   card count    = 2 = Terminal(11) + Browser(12)
///   focused count = 1 = Terminal(11)
///   content fields = 0，输入契约只接受身份、类型、标题与交互状态
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过 AgentCommand 更新当前画布上下文
///   2. 提交普通用户回合，触发一次 Dialogue Provider 请求
///   3. 读取公开 ProviderRequest 的首条 system message
///
/// 预期结果：
///   - 正断言：Prompt 包含当前项目、workspace root 和两张卡片摘要
///   - 正断言：Terminal(11) 是唯一聚焦卡片，Browser(12) 不聚焦
///   - 负断言：Prompt 不得声明终端正文或网页正文字段
#[test]
fn dialogue_request_injects_bounded_current_canvas_context_without_card_contents() {
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
        .submit(AgentCommand::UpdateCurrentCanvasContext(
            CurrentCanvasContext {
                project_name: "norma-ai-os".to_string(),
                workspace_root: "/workspace/norma-ai-os".to_string(),
                total_cards: 2,
                cards: vec![
                    CanvasCardContext {
                        node_id: 11,
                        kind: CanvasCardKind::Terminal,
                        title: "Main Terminal".to_string(),
                        selected: true,
                        focused: true,
                    },
                    CanvasCardContext {
                        node_id: 12,
                        kind: CanvasCardKind::Browser,
                        title: "Docs".to_string(),
                        selected: false,
                        focused: false,
                    },
                ],
            },
        ))
        .expect("合法当前画布上下文应可更新");

    system
        .submit(AgentCommand::UserTurn("当前画布上有什么？".to_string()))
        .expect("普通画布查询应生成 Dialogue 请求");

    let recorded = requests
        .lock()
        .expect("request recording lock should not be poisoned");
    let prompt = &recorded[0].messages[0].content;
    assert!(
        prompt.contains("<current-canvas-context>"),
        "当前画布上下文应有明确开始边界"
    );
    assert!(
        prompt.contains("project_name=\"norma-ai-os\"")
            && prompt.contains("workspace_root=\"/workspace/norma-ai-os\""),
        "Prompt 应包含当前项目名和 workspace root"
    );
    assert!(
        prompt.contains(
            "node_id=11 kind=terminal title=\"Main Terminal\" selected=true focused=true"
        ),
        "Prompt 应包含聚焦 Terminal(11) 的精确摘要"
    );
    assert!(
        prompt.contains("node_id=12 kind=browser title=\"Docs\" selected=false focused=false"),
        "Prompt 应包含非聚焦 Browser(12) 的精确摘要"
    );
    assert!(
        !prompt.contains("terminal_content") && !prompt.contains("page_content"),
        "当前画布上下文不得暴露终端正文或网页正文字段"
    );
}

/// CC-2：大画布上下文应优先保留聚焦卡片和 Agent，并明确报告截断。
///
/// 数据构造（含关键数值的推导过程）：
///   context card limit = 64
///   ordinary terminals = 64，node_id=1..64
///   Agent              = 1，node_id=65
///   focused Browser    = 1，node_id=66
///   Total              = 64 + 1 + 1 = 66
///   omitted            = 66 - 64 = 2，应为优先级最低的 Terminal(63/64)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 更新包含 66 张卡片的当前画布上下文
///   2. 提交普通 Dialogue 回合
///   3. 读取 system message 中的包含/省略计数与具体卡片行
///
/// 预期结果：
///   - 正断言：总数 66、包含 64、省略 2
///   - 正断言：聚焦 Browser(66)、Agent(65) 和普通 Terminal(1) 存在
///   - 负断言：Terminal(63/64) 不存在，证明截断没有丢掉高优先级卡片
#[test]
fn current_canvas_context_prioritizes_focused_and_agent_cards_when_truncated() {
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
    let mut cards = (1..=64)
        .map(|node_id| CanvasCardContext {
            node_id,
            kind: CanvasCardKind::Terminal,
            title: format!("Terminal {node_id}"),
            selected: false,
            focused: false,
        })
        .collect::<Vec<_>>();
    cards.push(CanvasCardContext {
        node_id: 65,
        kind: CanvasCardKind::Agent,
        title: "Alan".to_string(),
        selected: false,
        focused: false,
    });
    cards.push(CanvasCardContext {
        node_id: 66,
        kind: CanvasCardKind::Browser,
        title: "Focused Docs".to_string(),
        selected: true,
        focused: true,
    });
    system
        .submit(AgentCommand::UpdateCurrentCanvasContext(
            CurrentCanvasContext {
                project_name: "norma-ai-os".to_string(),
                workspace_root: "/workspace/norma-ai-os".to_string(),
                total_cards: 66,
                cards,
            },
        ))
        .expect("大画布上下文应可以受限渲染");

    system
        .submit(AgentCommand::UserTurn("当前画布上有什么？".to_string()))
        .expect("大画布查询应生成 Dialogue 请求");

    let recorded = requests
        .lock()
        .expect("request recording lock should not be poisoned");
    let prompt = &recorded[0].messages[0].content;
    assert!(
        prompt.contains("total_cards=66 included_cards=64 omitted_cards=2"),
        "大画布 Prompt 应明确报告总数、包含数和省略数"
    );
    assert!(
        prompt
            .contains("node_id=66 kind=browser title=\"Focused Docs\" selected=true focused=true"),
        "聚焦 Browser(66) 必须在截断后保留"
    );
    assert!(
        prompt.contains("node_id=65 kind=agent title=\"Alan\" selected=false focused=false"),
        "Agent(65) 必须在截断后保留"
    );
    assert!(
        prompt
            .contains("node_id=1 kind=terminal title=\"Terminal 1\" selected=false focused=false"),
        "未超出剩余预算的 Terminal(1) 应保留"
    );
    assert!(
        !prompt.contains("node_id=63 kind=terminal")
            && !prompt.contains("node_id=64 kind=terminal"),
        "低优先级 Terminal(63/64) 应被省略，不得挤掉聚焦卡片或 Agent"
    );
}
