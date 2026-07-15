use super::*;
use canvas::{AgentProfile, AgentSpec, CanvasRect};
/// 数据构造（含关键数值推导）：
///   Claude node: id=1, kind=Agent(profile=Claude)
///   Codex node:  id=2, kind=Agent(profile=Codex)
///   OpenCode node: id=3, kind=Agent(profile=OpenCode)
///   profile display title 分别为 "Claude" / "Codex" / "OpenCode"
///
/// 执行过程：
///   1. 构造三个 CanvasNode
///   2. 调用 AgentCard.chrome
///
/// 预期结果：
///   - 正断言：Claude/Codex/OpenCode title 分别正确
///   - 负断言：Claude title 不应退化为 "Agent" 或 "Terminal"
#[test]
fn test_agent_card_chrome_uses_profile_title() {
    let runtime = Runtime::new();
    let claude = CanvasNode {
        id: 1,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: None,
            definition_id: None,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        z_index: 1,
        selected: false,
    };
    let codex = CanvasNode {
        id: 2,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
            definition_id: None,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        z_index: 2,
        selected: false,
    };
    let opencode = CanvasNode {
        id: 3,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::OpenCode,
            alias: None,
            definition_id: None,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        z_index: 3,
        selected: false,
    };

    let claude_chrome = CARD.chrome(&claude, &runtime);
    let codex_chrome = CARD.chrome(&codex, &runtime);
    let opencode_chrome = CARD.chrome(&opencode, &runtime);

    assert_eq!(
        claude_chrome.title, "Claude",
        "Claude Agent card 标题应显示 Claude"
    );
    assert_eq!(
        codex_chrome.title, "Codex",
        "Codex Agent card 标题应显示 Codex"
    );
    assert_eq!(
        opencode_chrome.title, "OpenCode",
        "OpenCode Agent card 标题应显示 OpenCode"
    );
    assert_ne!(
        claude_chrome.title, "Agent",
        "Claude Agent card 不应退化为泛化 Agent 标题"
    );
    assert_ne!(
        claude_chrome.title, "Terminal",
        "Claude Agent card 不应退化为 Terminal 标题"
    );
}

/// AgentCard chrome 应优先使用 alias，缺失 alias 时回退 profile。
///
/// 数据构造（含关键数值推导）：
///   node_id=3，profile=Codex，alias=Alan
///   runtime 为空；chrome 标题只依赖 CanvasNodeKind::Agent
///
/// 执行过程：
///   1. 构造带 alias 的 Agent node
///   2. 调用 AgentCard.chrome
///   3. 检查 title
///
/// 预期结果：
///   - 正断言：title 为 Alan
///   - 负断言：title 不应是 Codex 或泛化 Agent
#[test]
fn test_agent_card_chrome_prefers_alias_over_profile_title() {
    let runtime = Runtime::new();
    let node = CanvasNode {
        id: 3,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("Alan".to_string()),
            definition_id: None,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        z_index: 3,
        selected: false,
    };

    let chrome = CARD.chrome(&node, &runtime);

    assert_eq!(chrome.title, "Alan", "Agent alias 应优先作为 card 标题");
    assert_ne!(chrome.title, "Codex", "带 alias 时不应显示 profile title");
    assert_ne!(chrome.title, "Agent", "带 alias 时不应退化为泛化 Agent");
}

/// AgentCard chrome 会把 runtime task state 映射成短 badge。
///
/// 数据构造（含关键数值推导）：
///   runtime 添加 node_id=1 的 Codex Agent
///   task update: PreToolUse 等价状态 RunningTool(tool_name=shell)
///   badge 文案 V1 固定为短标签 "Tool"，避免标题栏溢出
///
/// 执行过程：
///   1. 在 runtime.document 中添加 Agent node
///   2. apply_agent_task_update 写入 RunningTool
///   3. 调用 AgentCard.chrome 读取 badge
///
/// 预期结果：
///   - 正断言：badge 存在且 label 为 Tool
///   - 负断言：badge 不应显示 Terminal，也不应退化为 None
#[test]
fn test_agent_card_chrome_shows_task_badge() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
            definition_id: None,
        }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let changed = runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Codex,
        provider_event: "PreToolUse".to_string(),
        to: AgentTaskState::RunningTool {
            tool_name: Some("shell".to_string()),
        },
        task_title: None,
        session_id: None,
    });
    let node = runtime
        .document
        .node(node_id)
        .expect("Agent node 应存在于 runtime document")
        .clone();

    let chrome = CARD.chrome(&node, &runtime);
    let badge = chrome
        .task_badge
        .expect("RunningTool 状态应渲染 task badge");

    assert!(changed, "Agent task update 应改变 runtime task state");
    assert_eq!(badge.label, "Tool", "RunningTool 应显示短标签 Tool");
    assert_ne!(
        badge.label, "Terminal",
        "Agent task badge 不应显示 Terminal 文案"
    );
}

/// AgentCard chrome: task_title 存在时标题格式为 "<profile> — <task_title>"。
///
/// 数据构造：
///   node_id=1, profile=Claude, alias=None
///   runtime 写入 task_title = "帮我重构 user_service.rs"（首次 UserPromptSubmit 触发）
///
/// 执行过程：
///   1. apply_agent_task_update task_title=Some("帮我重构 user_service.rs"), to=Thinking
///   2. 调用 AgentCard.chrome()
///
/// 预期结果：
///   - 正断言：chrome.title = "Claude — 帮我重构 user_service.rs"（profile 前缀 + task_title）
///   - 负断言：chrome.title ≠ "Claude"（有 task_title 时不应只显示 profile 名）
#[test]
fn test_agent_card_chrome_uses_task_title_when_set() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: None,
            definition_id: None,
        }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: runtime::AgentTaskState::Thinking,
        task_title: Some("帮我重构 user_service.rs".to_string()),
        session_id: None,
    });
    let node = runtime.document.node(node_id).unwrap().clone();

    let chrome = CARD.chrome(&node, &runtime);

    assert_eq!(
        chrome.title, "Claude — 帮我重构 user_service.rs",
        "task_title 存在时 chrome.title 应为 '<profile> — <task_title>'"
    );
    assert_ne!(
        chrome.title, "Claude",
        "task_title 存在时不应只显示 profile 名"
    );
}

/// AgentCard chrome: task_title 未设置时，标题回退到 alias（若有）或 profile 名。
///
/// 数据构造：
///   node_id=99, profile=Codex, alias=None；未发过 UserPromptSubmit
///
/// 执行过程：
///   1. 不调用 apply_agent_task_update（无 task_title）
///   2. 调用 AgentCard.chrome()
///
/// 预期结果：
///   - 正断言：chrome.title = "Codex"（回退到 profile 名）
#[test]
fn test_agent_card_chrome_falls_back_to_profile_when_no_task_title() {
    let runtime = Runtime::new();
    let node = CanvasNode {
        id: 99,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
            definition_id: None,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        z_index: 1,
        selected: false,
    };

    let chrome = CARD.chrome(&node, &runtime);

    assert_eq!(chrome.title, "Codex", "无 task_title 时应回退到 profile 名");
}

/// AgentCard chrome: task_title 优先于 alias（前缀用 alias，后缀用 task_title）。
///
/// 数据构造：
///   node_id=1, profile=Claude, alias=Some("Alan")
///   runtime 写入 task_title = "重构任务"
///
/// 执行过程：
///   1. apply_agent_task_update task_title=Some("重构任务"), to=Thinking
///   2. 调用 AgentCard.chrome()
///
/// 预期结果：
///   - 正断言：chrome.title = "Alan — 重构任务"（alias 作前缀，task_title 作后缀）
///   - 负断言：chrome.title ≠ "Alan"（有 task_title 时不应只显示 alias）
#[test]
fn test_agent_card_chrome_task_title_beats_alias() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: Some("Alan".to_string()),
            definition_id: None,
        }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: runtime::AgentTaskState::Thinking,
        task_title: Some("重构任务".to_string()),
        session_id: None,
    });
    let node = runtime.document.node(node_id).unwrap().clone();

    let chrome = CARD.chrome(&node, &runtime);

    assert_eq!(
        chrome.title, "Alan — 重构任务",
        "alias 作前缀、task_title 作后缀，格式应为 '<alias> — <task_title>'"
    );
    assert_ne!(chrome.title, "Alan", "有 task_title 时不应只显示 alias");
}
