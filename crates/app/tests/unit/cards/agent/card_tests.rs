use super::*;
use canvas::{AgentProfile, AgentSpec, CanvasRect};

/// DWS-05：Agent 卡片标题栏应同时显示项目和所选 worktree。
///
/// 数据构造（含关键数值的推导过程）：
///   项目目录 1 个、worktree 目录 1 个、Agent 卡片 1 张；显示位置 = 项目名 + worktree 名。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以项目目录创建 Runtime，注入可控 PTY。
///   2. 指定另一目录启动一张 Codex Agent 卡片并读取 chrome。
///
/// 预期结果：
///   - 正断言：标题详情同时包含 project 与 linked。
///   - 负断言：不会误显示为默认项目目录 worktree。
#[test]
fn agent_card_chrome_shows_project_and_selected_worktree() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let project = temp.path().join("project");
    let linked = temp.path().join("linked");
    std::fs::create_dir(&project).expect("应创建项目目录");
    std::fs::create_dir(&linked).expect("应创建 worktree 目录");
    let mut runtime = Runtime::new_for_workspace(project);
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    }));
    let id = runtime
        .spawn_agent_with_worktree(
            AgentProfile::Codex,
            None,
            CanvasRect::new(0.0, 0.0, 400.0, 300.0),
            80,
            24,
            &linked,
        )
        .expect("应启动可控 Agent 卡片");
    let node = runtime.document.node(id).expect("卡片应存在");
    let chrome = CARD.chrome(node, &runtime);
    assert_eq!(
        chrome.title_detail.as_deref(),
        Some("project / linked"),
        "卡片应标出项目和所选 worktree"
    );
    assert_ne!(
        chrome.title_detail.as_deref(),
        Some("project / project"),
        "不能误显示默认 worktree"
    );
}
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
        card_instance_id: canvas::new_card_instance_id(),
        id: 1,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        z_index: 1,
        selected: false,
    };
    let codex = CanvasNode {
        card_instance_id: canvas::new_card_instance_id(),
        id: 2,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        z_index: 2,
        selected: false,
    };
    let opencode = CanvasNode {
        card_instance_id: canvas::new_card_instance_id(),
        id: 3,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::OpenCode,
            alias: None,
            definition_id: None,
            mobile_control: false,
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
        card_instance_id: canvas::new_card_instance_id(),
        id: 3,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("Alan".to_string()),
            definition_id: None,
            mobile_control: false,
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
            mobile_control: false,
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

/// AgentCard chrome：task_title 存在时稳定名称与任务标题必须分开提供给布局层。
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
///   - 正断言：chrome.title = "Claude"，作为不可被任务正文挤掉的稳定名称。
///   - 正断言：chrome.title_detail = "帮我重构 user_service.rs"，作为可截断任务标题。
///   - 负断言：稳定名称不包含任务正文，避免两者作为一个整体收缩。
#[test]
fn test_agent_card_chrome_uses_task_title_when_set() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: None,
            definition_id: None,
            mobile_control: false,
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
        chrome.title, "Claude",
        "task_title 存在时 chrome.title 仍应只承载稳定 Agent 名称"
    );
    assert_eq!(
        chrome.title_detail.as_deref(),
        Some("帮我重构 user_service.rs"),
        "task_title 应进入独立 title_detail，交给布局层单独截断"
    );
    assert!(
        !chrome.title.contains("user_service.rs"),
        "稳定 Agent 名称不得混入任务正文，否则长正文仍会挤压名称"
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
        card_instance_id: canvas::new_card_instance_id(),
        id: 99,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        z_index: 1,
        selected: false,
    };

    let chrome = CARD.chrome(&node, &runtime);

    assert_eq!(chrome.title, "Codex", "无 task_title 时应回退到 profile 名");
    assert_eq!(
        chrome.title_detail, None,
        "无 task_title 时不应渲染空的任务标题分区"
    );
}

/// AgentCard chrome：alias 保持稳定名称，task_title 进入独立的可收缩详情。
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
///   - 正断言：chrome.title = "Alan"，alias 不参与任务标题截断。
///   - 正断言：chrome.title_detail = "重构任务"。
///   - 负断言：chrome.title 不包含 "重构任务"，避免名称和任务共同收缩。
#[test]
fn test_agent_card_chrome_keeps_alias_separate_from_task_title() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: Some("Alan".to_string()),
            definition_id: None,
            mobile_control: false,
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
        chrome.title, "Alan",
        "alias 应保持为独立、不可被任务正文挤掉的 Agent 名称"
    );
    assert_eq!(
        chrome.title_detail.as_deref(),
        Some("重构任务"),
        "task_title 应作为独立 title_detail 提供给标题栏"
    );
    assert!(
        !chrome.title.contains("重构任务"),
        "稳定 Agent 名称不得拼接任务正文"
    );
}

/// AgentCard chrome：长且多行的用户任务不能覆盖 Agent 名称，任务标题必须折叠为单行。
///
/// 数据构造（含关键数值推导）：
///   node_id = 1；alias = "Alan"（4 个 ASCII 字符，作为稳定 Agent 身份）
///   task_title = 第一行 20 个汉字 + LF(1) + 第二行 16 个汉字 = 37 个 Unicode scalar
///   logical lines = 2；渲染标题允许的换行数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 runtime 写入包含显式 LF 的长 task_title，并进入 Thinking。
///   2. 调用 AgentCard.chrome() 构造标题栏展示模型。
///   3. 分别读取稳定名称 title 与可收缩任务 title_detail。
///
/// 预期结果：
///   - 正断言：title 精确为 "Alan"，长任务正文不能挤掉 Agent 名称。
///   - 正断言：title_detail 把 LF 折叠成一个空格并保留两段任务语义。
///   - 负断言：title_detail 不包含 LF，不允许 Agent 标题产生第二行。
#[test]
fn test_agent_card_chrome_keeps_name_and_collapses_multiline_task_title() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: Some("Alan".to_string()),
            definition_id: None,
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 320.0, 220.0),
    );
    let multiline_task =
        "请分析这个非常长的用户输入并保留代理名称\n然后继续处理第二行且标题不要换行";
    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: runtime::AgentTaskState::Thinking,
        task_title: Some(multiline_task.to_string()),
        session_id: None,
    });
    let node = runtime
        .document
        .node(node_id)
        .expect("多行标题测试的 Agent node 应存在")
        .clone();

    let chrome = CARD.chrome(&node, &runtime);
    let title_detail = chrome
        .title_detail
        .expect("存在 task_title 时应生成独立的可收缩 title_detail");

    assert_eq!(
        chrome.title, "Alan",
        "长 task_title 不得与 Agent 名称拼成同一个可收缩标题"
    );
    assert_eq!(
        title_detail, "请分析这个非常长的用户输入并保留代理名称 然后继续处理第二行且标题不要换行",
        "多行 task_title 应折叠为保留任务语义的单行标题"
    );
    assert!(
        !title_detail.contains('\n'),
        "Agent title_detail 不得保留 LF，否则标题栏可能渲染成多行"
    );
}
