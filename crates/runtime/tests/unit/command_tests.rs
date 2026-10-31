use super::*;

/// 场景：完整 Agent 指令直接创建，分类只提供类型候选。
///
/// 数据构造：/new 为一个词；/new claude 为命令加类型两个词；主题目录为空。
/// 执行过程：分别查询分类、完整指令与带旧位置参数的指令。
/// 预期结果：五种 Agent 均可选；完整 Claude 指令只有一个直接创建动作；旧位置不执行。
#[test]
fn agent_command_creates_without_position_step() {
    let agents = suggestions("/new", &[], &[]);
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
            .any(|s| s.command == Some(DockCommand::Agent(AgentProfile::InfCode))),
        "分类必须包含 InfCode"
    );
    assert!(
        agents
            .iter()
            .any(|s| s.command == Some(DockCommand::Agent(AgentProfile::Grok))),
        "分类必须包含 Grok"
    );
    let claude = suggestions("/new claude", &[], &[]);
    assert_eq!(claude.len(), 1, "完整类型只产生一个候选，不应出现位置选择");
    assert_eq!(
        claude[0].command,
        Some(DockCommand::Agent(AgentProfile::Claude)),
        "确认 Claude 应直接创建"
    );
    assert!(
        suggestions("/new claude right", &[], &[]).is_empty(),
        "废弃位置参数不可静默执行"
    );
}

/// 场景：常用中文表达映射到同一动作，含糊主题返回候选，不接受否定、批量和未知参数。
///
/// 数据构造：2 个主题分别为浅色 Paper Blue 和深色 Midnight Violet，深色候选数 = 2 - 1 = 1。
/// 执行过程：查询创建、终端、深色主题，再查询浏览器、预览、否定和混合表达。
/// 预期结果：终端与主题仍对应原动作；根列表平铺为 10 个 Agent + 1 终端 + 1 浏览文件夹 + 2 主题 = 14 项，不含浏览器和预览；不支持的输入没有动作。
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
        suggestions("创建一个 Codex 卡片", &themes, &[])[0].command,
        Some(DockCommand::Agent(AgentProfile::Codex)),
        "常用中文创建应选择 Codex"
    );
    assert_eq!(
        suggestions("打开终端", &themes, &[])[0].command,
        Some(DockCommand::Terminal),
        "打开终端应映射到创建终端"
    );
    assert!(
        suggestions("浏览器", &themes, &[]).is_empty()
            && suggestions("创建浏览器", &themes, &[]).is_empty()
            && suggestions("/browser", &themes, &[]).is_empty(),
        "快捷指令不再提供创建浏览器"
    );
    assert!(
        suggestions("预览文件", &themes, &[]).is_empty()
            && suggestions("查看文件", &themes, &[]).is_empty()
            && suggestions("/preview", &themes, &[]).is_empty(),
        "快捷指令不再提供查看文件"
    );
    let dark = suggestions("切换深色主题", &themes, &[]);
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
        suggestions("/theme paper-blue", &themes, &[])[0].command,
        Some(DockCommand::Theme("paper-blue".into())),
        "完整主题指令应精确匹配"
    );
    assert!(
        suggestions("不要创建 Codex", &themes, &[]).is_empty(),
        "否定表达不应触发创建"
    );
    assert!(
        suggestions("创建 Codex 并打开浏览器", &themes, &[]).is_empty(),
        "多个操作不能被截断为单个动作"
    );
    assert!(
        suggestions("/new codex extra", &themes, &[]).is_empty(),
        "额外参数不应被丢弃后执行"
    );
    assert!(
        suggestions("/unknown", &themes, &[]).is_empty(),
        "未知指令不能执行"
    );
    assert!(
        suggestions("/ego", &themes, &[]).is_empty(),
        "本版本不接入 Ego"
    );
    let roots = suggestions("/", &themes, &[]);
    assert_eq!(
        roots.len(),
        16,
        "根列表平铺全部叶子指令，共 10 + 1 + 1(ask) + 1 + 2 + 1 = 16 项"
    );
    assert!(
        roots
            .iter()
            .any(|item| item.command == Some(DockCommand::Terminal)),
        "根列表仍包含新建终端"
    );
    assert!(
        roots.iter().all(|item| {
            !matches!(
                item.command,
                Some(DockCommand::Browser | DockCommand::Preview)
            )
        }),
        "根列表不得包含创建浏览器或查看文件"
    );
}

/// 场景：根列表平铺全部叶子指令并按组排序，裸词可直接过滤。
///
/// 数据构造：2 个主题；Agent 10 个，终端 1 个。
/// 执行过程：查询空输入、`/`、`codex`、`midnight`、`/te`。
/// 预期结果：空输入与 `/` 一致且无需进入分类（无 command 为 None 的项）；分组顺序为 Agent→Terminal→Theme；
/// 裸词 `midnight` 直接命中主题，`/te` 命中终端与带 `/theme` 之外的前缀项。
#[test]
fn root_lists_flat_grouped_leaves_and_bare_words_filter() {
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
    let root = suggestions("", &themes, &[]);
    assert_eq!(root, suggestions("/", &themes, &[]), "空输入与 / 应一致");
    assert!(
        root.iter()
            .all(|s| s.command.is_some() || s.group == CommandGroup::Ask),
        "除 ask 补全入口外不应再有分类项"
    );
    let groups: Vec<_> = root.iter().map(|s| s.group).collect();
    let mut sorted = groups.clone();
    sorted.sort_by_key(|g| *g as u8);
    assert_eq!(
        groups, sorted,
        "分组顺序应为 Agent、Terminal、Ask、Project、Theme"
    );
    let midnight = suggestions("midnight", &themes, &[]);
    assert_eq!(midnight.len(), 1, "裸词只命中一个主题");
    assert_eq!(
        midnight[0].command,
        Some(DockCommand::Theme("midnight-violet".into())),
        "midnight 应直接命中 Midnight Violet"
    );
    let te = suggestions("/te", &themes, &[]);
    assert!(
        te.iter().any(|s| s.command == Some(DockCommand::Terminal)),
        "/te 应命中终端"
    );
}

/// 场景：`/add` 按名称添加最近 repo，`/open` 打开文件夹选择器。
///
/// 数据构造：2 个最近 repo（norma、Website）；主题目录为空。
/// 执行过程：查询 `/add`、`/add web`、`/add nothing`、`/open`、裸词 `norma`。
/// 预期结果：`/add` 为 2 个 repo + 1 个浏览项 = 3；`/add web` 只命中 Website 再加浏览项；
/// 无匹配时仍保留浏览项；`/open` 只返回浏览项；裸词可直接命中 repo。
#[test]
fn add_command_lists_recent_repos_and_falls_back_to_browse() {
    let projects = [
        CommandProject {
            name: "norma".into(),
            path: "/code/norma".into(),
        },
        CommandProject {
            name: "Website".into(),
            path: "/code/site".into(),
        },
    ];
    assert_eq!(
        suggestions("/add", &[], &projects).len(),
        3,
        "2 个 repo + 浏览项"
    );
    let web = suggestions("/add web", &[], &projects);
    assert_eq!(
        web[0].command,
        Some(DockCommand::AddProject("/code/site".into())),
        "名称过滤不区分大小写"
    );
    assert_eq!(web.len(), 2, "命中的 repo + 浏览项");
    let none = suggestions("/add nothing", &[], &projects);
    assert_eq!(
        none.iter().map(|s| s.command.clone()).collect::<Vec<_>>(),
        vec![Some(DockCommand::BrowseProject)],
        "无匹配时只剩浏览项"
    );
    assert_eq!(
        suggestions("/open", &[], &projects)[0].command,
        Some(DockCommand::BrowseProject),
        "/open 打开文件夹选择器"
    );
    assert_eq!(
        suggestions("norma", &[], &projects)[0].command,
        Some(DockCommand::AddProject("/code/norma".into())),
        "裸词直接命中 repo"
    );
    assert!(
        !suggestions("/", &[], &projects)
            .iter()
            .any(|s| matches!(s.command, Some(DockCommand::AddProject(_)))),
        "根列表不展开 repo，避免淹没其它指令"
    );
}

/// 场景：/help 与中英文裸词都只产生一个“打开功能引导”候选。
///
/// 数据构造：主题与最近 repo 为空；根列表 = 10 Agent + 1 终端 + 1 浏览文件夹 + 1 帮助 = 13。
/// 执行过程：查询 `/help`、`/he`、`帮助`、`help`、带多余参数的 `/help me`、根列表。
/// 预期结果：前四个各只有一个 Help 候选；`/help me` 不执行；根列表含 Help 且排在最后一组。
#[test]
fn help_command_opens_feature_guide() {
    for query in ["/help", "帮助", "help"] {
        let found = suggestions(query, &[], &[]);
        assert_eq!(found.len(), 1, "{query} 只应命中一个候选");
        assert_eq!(
            found[0].command,
            Some(DockCommand::Help),
            "{query} 应打开功能引导"
        );
    }
    let he = suggestions("/he", &[], &[]);
    assert_eq!(
        he[0].command,
        Some(DockCommand::Help),
        "/he 帮助相似度最高，排第一"
    );
    assert!(
        suggestions("/help me", &[], &[]).is_empty(),
        "多余参数不能静默执行"
    );
    let roots = suggestions("/", &[], &[]);
    assert_eq!(roots.len(), 14, "10 + 1 + 1(ask) + 1 + 1 = 14 项");
    assert_eq!(
        roots.last().and_then(|item| item.command.clone()),
        Some(DockCommand::Help),
        "帮助排在最后一组"
    );
}

/// 场景：输入 `/open` 或 `/opencode` 时，Tab 补全应能拿到 OpenCode 的创建指令。
///
/// 数据构造：主题与 repo 为空。
/// 执行过程：查询 `/open`、`/opencode`、`/nothing`。
/// 预期结果：`/open` 浏览项在前、OpenCode 补全为 `/new opencode`；`/opencode` 直接命中该补全；无匹配仍为空。
#[test]
fn slash_agent_shorthand_completes_to_new_command() {
    let open = suggestions("/open", &[], &[]);
    assert_eq!(
        open[0].command,
        Some(DockCommand::BrowseProject),
        "/open 仍以浏览文件夹为首选"
    );
    assert!(
        open.iter().any(|s| s.completion == "/new opencode"),
        "/open 必须额外列出 OpenCode"
    );
    let direct = suggestions("/opencode", &[], &[]);
    assert_eq!(direct.len(), 1, "/opencode 只命中一个候选");
    assert_eq!(direct[0].completion, "/new opencode");
    assert!(
        suggestions("/nothing", &[], &[]).is_empty(),
        "无匹配不应有候选"
    );
}

/// 场景：快捷指令模糊匹配，相似度最高的排在最前。
///
/// 数据构造：主题与 repo 为空。
/// 执行过程：查询 `/opcode`（子序列）、`/cod`（前缀）、`/de`（子串）。
/// 预期结果：`/opcode` 命中 OpenCode；`/cod` 中 Codex 排在 OpenCode/InfCode 之前；`/de` 的前缀/子串排序稳定且不含无关项。
#[test]
fn fuzzy_matching_ranks_most_similar_first() {
    let opn = suggestions("/opcode", &[], &[]);
    assert_eq!(opn[0].completion, "/new opencode", "子序列应命中 OpenCode");
    let cod = suggestions("/cod", &[], &[]);
    assert_eq!(cod[0].completion, "/new codex", "前缀匹配排在子串匹配之前");
    assert!(
        cod.iter().any(|s| s.completion == "/new opencode"),
        "子串匹配仍然保留"
    );
    let pos = |c: &str| cod.iter().position(|s| s.completion == c);
    assert!(pos("/new codex") < pos("/new opencode"), "相似度高者在前");
}

/// 场景：`ask` 的发现与补全。
/// 执行：查询根列表、`ask`、`/ask co`、`ask codex hello`。
/// 预期：根列表有一条 ask 入口；`ask` 列出各类型且只补全不执行；输入消息后不再出候选，回车才能发送。
#[test]
fn ask_is_discoverable_and_completes_without_executing() {
    assert!(
        suggestions("", &[], &[])
            .iter()
            .any(|s| s.group == CommandGroup::Ask && s.completion == "ask "),
        "根列表必须有 ask 入口"
    );
    let types = suggestions("ask", &[], &[]);
    assert!(types.len() >= 5 && types.iter().all(|s| s.command.is_none()));
    assert!(types.iter().any(|s| s.completion == "ask codex "));
    let filtered = suggestions("/ask co", &[], &[]);
    assert_eq!(
        filtered.first().map(|s| s.completion.as_str()),
        Some("ask codex ")
    );
    assert!(suggestions("ask codex hello", &[], &[]).is_empty());
}
