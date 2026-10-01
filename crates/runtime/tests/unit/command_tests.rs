use super::*;

/// 场景：完整 Agent 指令直接创建，分类只提供类型候选。
///
/// 数据构造：/new 为一个词；/new claude 为命令加类型两个词；主题目录为空。
/// 执行过程：分别查询分类、完整指令与带旧位置参数的指令。
/// 预期结果：五种 Agent 均可选；完整 Claude 指令只有一个直接创建动作；旧位置不执行。
#[test]
fn agent_command_creates_without_position_step() {
    let agents = suggestions("/new", &[]);
    assert!(
        agents
            .iter()
            .any(|s| s.command == Some(DockCommand::Agent(AgentProfile::Codex))),
        "分类必须包含 Codex"
    );
    assert!(
        agents
            .iter()
            .any(|s| s.command == Some(DockCommand::Agent(AgentProfile::Claude))),
        "分类必须包含 Claude"
    );
    assert!(
        agents
            .iter()
            .any(|s| s.command == Some(DockCommand::Agent(AgentProfile::Cursor))),
        "分类必须包含 Cursor"
    );
    assert!(
        agents
            .iter()
            .any(|s| s.command == Some(DockCommand::Agent(AgentProfile::OpenCode))),
        "分类必须包含 OpenCode"
    );
    assert!(
        agents
            .iter()
            .any(|s| s.command == Some(DockCommand::Agent(AgentProfile::Grok))),
        "分类必须包含 Grok"
    );
    let claude = suggestions("/new claude", &[]);
    assert_eq!(claude.len(), 1, "完整类型只产生一个候选，不应出现位置选择");
    assert_eq!(
        claude[0].command,
        Some(DockCommand::Agent(AgentProfile::Claude)),
        "确认 Claude 应直接创建"
    );
    assert!(
        suggestions("/new claude right", &[]).is_empty(),
        "废弃位置参数不可静默执行"
    );
}

/// 场景：常用中文表达映射到同一动作，含糊主题返回候选，不接受否定、批量和未知参数。
///
/// 数据构造：2 个主题分别为浅色 Paper Blue 和深色 Midnight Violet，深色候选数 = 2 - 1 = 1。
/// 执行过程：查询创建、终端、浏览器、预览、深色主题，再查询否定和混合表达。
/// 预期结果：每个明确表达对应同一个已支持动作；深色不包含浅色；不支持的输入没有动作。
#[test]
fn common_phrases_and_theme_catalog_are_bounded() {
    let themes = [
        CommandTheme {
            id: "paper-blue",
            title: "Paper Blue",
            dark: false,
        },
        CommandTheme {
            id: "midnight-violet",
            title: "Midnight Violet",
            dark: true,
        },
    ];
    assert_eq!(
        suggestions("创建一个 Codex 卡片", &themes)[0].command,
        Some(DockCommand::Agent(AgentProfile::Codex)),
        "常用中文创建应选择 Codex"
    );
    assert_eq!(
        suggestions("打开终端", &themes)[0].command,
        Some(DockCommand::Terminal),
        "打开终端应映射到创建终端"
    );
    assert_eq!(
        suggestions("浏览器", &themes)[0].command,
        Some(DockCommand::Browser),
        "浏览器关键词应找到创建浏览器"
    );
    assert_eq!(
        suggestions("预览文件", &themes)[0].command,
        Some(DockCommand::Preview),
        "预览文件应调用文件选择器"
    );
    let dark = suggestions("切换深色主题", &themes);
    assert_eq!(dark.len(), 1, "目录仅有一个深色主题");
    assert_eq!(
        dark[0].command,
        Some(DockCommand::Theme("midnight-violet".into())),
        "深色主题应为 Midnight Violet"
    );
    assert!(
        !dark
            .iter()
            .any(|s| s.command == Some(DockCommand::Theme("paper-blue".into()))),
        "深色候选不得包含浅色 Paper Blue"
    );
    assert_eq!(
        suggestions("/theme paper-blue", &themes)[0].command,
        Some(DockCommand::Theme("paper-blue".into())),
        "完整主题指令应精确匹配"
    );
    assert!(
        suggestions("不要创建 Codex", &themes).is_empty(),
        "否定表达不应触发创建"
    );
    assert!(
        suggestions("创建 Codex 并打开浏览器", &themes).is_empty(),
        "多个操作不能被截断为单个动作"
    );
    assert!(
        suggestions("/new codex extra", &themes).is_empty(),
        "额外参数不应被丢弃后执行"
    );
    assert!(
        suggestions("/unknown", &themes).is_empty(),
        "未知指令不能执行"
    );
    assert!(suggestions("/ego", &themes).is_empty(), "本版本不接入 Ego");
    assert_eq!(
        suggestions("/", &themes).len(),
        5,
        "根列表应保留已约定的五类指令"
    );
}
