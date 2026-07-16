//! Headless GPUI E2E：验证 Settings 与快速创建入口共享持久 Agent 定义身份。
//!
//! 这些测试驱动真实渲染树与公开 app action，但使用注入 PTY，不声称启动真实 `.app`
//! 或真实 Codex CLI；真实 CLI/Hook 冒烟由本文件末尾的 ignored L3 runner 覆盖。

use canvas::CanvasNodeKind;
use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{create_codex_agent, echo_factory, enter_canvas, make_app, read_runtime};

/// E2E-A-1：Settings 的 Add Agent 应先创建未保存草稿，Save 只保存定义而不打开卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   initial definitions=0，initial Agent cards=0
///   auto name=Alan（固定名称池第 1 项）
///   profile 从默认 Codex 切换为 Claude
///   workflow=""（可选）
///   Save 后 definitions=1，Agent cards 仍为 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless app、进入临时 workspace，并从工具栏打开 Settings → Agents
///   2. 真实点击 Add Agent，读取未保存草稿快照
///   3. 真实点击 Claude profile 与 Save，再读取 app、Runtime 和 SQLite
///
/// 预期结果：
///   - 正断言：保存前 creating_new=true、Alan/Codex/空 workflow，SQLite 仍为空
///   - 正断言：保存后定义为 Alan/Claude，SQLite 恰好 1 条
///   - 负断言：保存前后 Runtime 都不得自动出现 Agent 卡片
#[test]
fn settings_add_agent_is_unsaved_until_save_and_does_not_open_card() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-agents");
    let agents_page = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        agents_page.active_section, "Agents",
        "点击 Agents 后应进入真实 Agent Settings 页面"
    );
    assert!(
        cx.debug_bounds("settings-agents-list").is_some(),
        "已绑定项目的 Agents 页面应渲染 Agent 列表，而不是空项目提示"
    );
    assert!(
        cx.debug_bounds("settings-agent-empty-project").is_none(),
        "进入 workspace 后 Agents 页面不应显示未打开项目提示"
    );
    click_debug(cx, "settings-agent-new");
    let draft = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    let before_persisted = storage::ProjectStore::for_workspace(workspace.path())
        .expect("e2e project store 应打开")
        .agent_definition_store()
        .list()
        .expect("保存前应能读取空定义列表");
    let before_cards = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });

    assert!(draft.creating_new, "点击 Add Agent 后应进入未保存草稿状态");
    assert_eq!(draft.draft_name, "Alan", "空项目应预填名称池首项 Alan");
    assert_eq!(draft.draft_profile, "codex", "新草稿应可见地默认选择 Codex");
    assert_eq!(
        draft.draft_workflow, "",
        "Workflow Description 可选且默认应为空"
    );
    assert!(
        before_persisted.is_empty(),
        "点击 Add 后保存前不得提前占用名称或写库"
    );
    assert_eq!(before_cards, 0, "Settings 草稿保存前不得创建 Agent 卡片");

    click_debug(cx, "settings-agent-profile-claude");
    click_debug(cx, "settings-agent-save");
    let saved = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    let persisted = storage::ProjectStore::for_workspace(workspace.path())
        .expect("e2e project store 应重新打开")
        .agent_definition_store()
        .list()
        .expect("保存后应能读取 Agent 定义");
    let after_cards = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });

    assert!(!saved.creating_new, "Save 后未保存草稿状态应结束");
    assert_eq!(
        saved.definitions.len(),
        1,
        "Save 后 app 名册应恰好包含 Alan"
    );
    assert_eq!(
        saved.definitions[0].name, "Alan",
        "保存定义应保留自动名称 Alan"
    );
    assert_eq!(
        saved.definitions[0].profile, "claude",
        "保存定义应采用用户选择的 Claude"
    );
    assert_eq!(
        persisted.len(),
        1,
        "Settings Save 应向项目 SQLite 写入恰好 1 条定义"
    );
    assert_eq!(persisted[0].name, "Alan", "SQLite 中的正式名称应为 Alan");
    assert_eq!(
        after_cards, 0,
        "Settings Save 只保存定义，不得隐式打开 Agent 卡片"
    );
}

/// E2E-A-2：工具栏快速创建 Codex 应自动命名、持久化，并打开绑定同一 definition 的卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   initial definitions=0，Agent cards=0
///   action=NewCodexAgent 1 次
///   expected definitions=1，Agent cards=1，二者通过同一非空 definition_id 绑定
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless app 并进入已绑定 ProjectStore 的临时 workspace
///   2. dispatch NewCodexAgent，注入 echo PTY 代替真实 Codex CLI
///   3. 读取 Runtime AgentSpec 与项目 SQLite 定义
///
/// 预期结果：
///   - 正断言：创建 Alan/Codex 定义并打开 1 张绑定卡片
///   - 正断言：快速入口创建的定义来源为 QuickCard，删除 Card 时可安全清理
///   - 正断言：卡片 definition_id 等于 SQLite definition id
///   - 负断言：不得产生 definition_id=None 的匿名 Agent 卡片
#[test]
fn quick_codex_create_persists_named_definition_and_bound_card() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let node_id = create_codex_agent(&app, cx);
    let spec = read_runtime(&app, cx, |runtime| {
        let node = runtime
            .document
            .node(node_id)
            .expect("NewCodexAgent 创建的 node_id 应存在");
        let CanvasNodeKind::Agent(spec) = &node.kind else {
            panic!("NewCodexAgent 应创建 Agent kind，实际是 {:?}", node.kind);
        };
        spec.clone()
    });
    let definitions = storage::ProjectStore::for_workspace(workspace.path())
        .expect("e2e project store 应打开")
        .agent_definition_store()
        .list()
        .expect("快速创建后应能读取 Agent 定义");
    let card_count = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });

    assert_eq!(
        definitions.len(),
        1,
        "一次快速创建应持久化恰好 1 条 Agent 定义"
    );
    assert_eq!(
        definitions[0].name, "Alan",
        "空项目快速创建应分配名称池首项 Alan"
    );
    assert_eq!(
        definitions[0].runtime_profile,
        canvas::AgentProfile::Codex,
        "NewCodexAgent 应保存 Codex runtime profile"
    );
    assert_eq!(
        definitions[0].origin,
        storage::AgentDefinitionOrigin::QuickCard,
        "NewCodexAgent 快速入口创建的 Definition 必须标记为 QuickCard"
    );
    assert_ne!(
        definitions[0].origin,
        storage::AgentDefinitionOrigin::Explicit,
        "快速创建不得伪装成 Settings 显式创建的长期角色"
    );
    assert_eq!(card_count, 1, "一次快速创建应打开恰好 1 张 Agent 卡片");
    assert_eq!(
        spec.alias.as_deref(),
        Some("Alan"),
        "卡片标题缓存应同步正式名称 Alan"
    );
    assert_eq!(
        spec.definition_id,
        Some(definitions[0].id),
        "卡片必须绑定刚持久化的 Agent definition id"
    );
    assert!(
        spec.definition_id.is_some(),
        "快速创建不得再产生 definition_id=None 的匿名 Agent 卡片"
    );
}

/// E2E-AGENT-LIFECYCLE-01：删除快速创建的 Agent Card 必须同步清理其临时 Definition。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 Agent Card       = 0 张
///   初始 Definition       = 0 条
///   Cmd+3 快速创建        = 1 张 Codex Card + 1 条 QuickCard Definition
///   Cmd+D 删除            = 删除刚创建且当前聚焦的 1 张 Card
///   最终期望              = 0 张 Card + 0 条 Definition
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless GPUI app 并进入空临时 workspace。
///   2. 模拟 Cmd+3，经过快捷键 registry、Agent action、Definition store 与 Runtime 创建链路。
///   3. 确认创建阶段确实存在 1 张 Agent Card 和 1 条 QuickCard Definition。
///   4. 模拟 Cmd+D，经过统一 Card 关闭路由删除当前聚焦节点。
///   5. 从 Runtime 与 ProjectStore 公共接口重新读取最终状态。
///
/// 预期结果：
///   - 正断言：删除前 Card 与 QuickCard Definition 各恰好 1 个。
///   - 正断言：删除后 Runtime 不再包含该 Agent Card。
///   - 正断言：删除后临时 Definition 同步清理，不再占用自动名称。
///   - 负断言：不得出现“Canvas 已无 Card、SQLite 仍残留 Definition”的隐形累积。
#[test]
fn deleting_quick_agent_card_removes_its_temporary_definition() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("生命周期 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    cx.simulate_keystrokes("cmd-3");
    cx.run_until_parked();

    let store = storage::ProjectStore::for_workspace(workspace.path())
        .expect("生命周期 e2e 应打开 ProjectStore")
        .agent_definition_store();
    let definitions_before = store.list().expect("删除前应读取 QuickCard Definition");
    let cards_before = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });

    assert_eq!(cards_before, 1, "Cmd+3 后应恰好存在 1 张 Codex Agent Card");
    assert_eq!(
        definitions_before.len(),
        1,
        "Cmd+3 后应恰好存在 1 条 QuickCard Definition"
    );
    assert_eq!(
        definitions_before[0].origin,
        storage::AgentDefinitionOrigin::QuickCard,
        "删除前置 Definition 必须确认为 QuickCard，避免误测长期角色"
    );

    cx.simulate_keystrokes("cmd-d");
    cx.run_until_parked();

    let cards_after = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });
    let definitions_after = store.list().expect("删除后应重新读取 Definition 列表");

    assert_eq!(
        cards_after, 0,
        "Cmd+D 后快速创建的 Agent Card 应从 Canvas 删除"
    );
    assert_eq!(
        definitions_after.len(),
        0,
        "删除快速 Agent Card 后其 QuickCard Definition 必须同步清理"
    );
    assert!(
        definitions_after
            .iter()
            .all(|definition| definition.id != definitions_before[0].id),
        "删除后不得残留原 QuickCard Definition，否则名称会继续被隐形占用"
    );
}

/// E2E-AGENT-LIFECYCLE-02：已有 30 个内置名称 Definition 时仍能快速创建第 31 个 Agent。
///
/// 数据构造（含关键数值的推导过程）：
///   builtin definitions = 30 条（Alan..Rowan，每个内置名称各 1 条）
///   初始 Agent Card     = 0 张
///   Cmd+2 创建          = 1 条 Claude QuickCard Definition + 1 张 Agent Card
///   最终 Definition     = 30 + 1 = 31 条
///   首个后备名称         = AgentA
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过 ProjectStore 公共接口显式创建 30 个长期 Definition，占满内置名称池。
///   2. 启动 headless GPUI app 并进入该 workspace，加载已有定义。
///   3. 模拟 Cmd+2，走真实快捷键、自动命名、Definition store 与 Runtime 创建链路。
///   4. 重新读取 SQLite 与 Runtime document，核对第 31 条记录及绑定 Card。
///
/// 预期结果：
///   - 正断言：最终恰好有 31 条 Definition，原 30 条长期角色全部保留。
///   - 正断言：新增 Definition 名称为 AgentA、profile 为 Claude、来源为 QuickCard。
///   - 正断言：Canvas 出现恰好 1 张绑定 AgentA 的 Agent Card。
///   - 负断言：不得因内置名称池耗尽而保持 30 条或返回空名称。
#[test]
fn quick_agent_creation_continues_after_thirty_builtin_definitions() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("第 31 个 Agent e2e 应创建临时 workspace");
    let store = storage::ProjectStore::for_workspace(workspace.path())
        .expect("第 31 个 Agent e2e 应打开 ProjectStore")
        .agent_definition_store();
    for builtin in agent::builtin_agent_names() {
        store
            .create(&storage::NewAgentDefinition {
                name: builtin.name().to_string(),
                runtime_profile: canvas::AgentProfile::Codex,
                workflow_description: "Long-lived role".to_string(),
            })
            .expect("30 个互异内置名称都应成功创建为长期 Definition");
    }
    enter_canvas(&app, cx, workspace.path());

    cx.simulate_keystrokes("cmd-2");
    cx.run_until_parked();

    let definitions = store.list().expect("第 31 次创建后应读取全部 Definition");
    let generated = definitions
        .iter()
        .find(|definition| definition.name == "AgentA")
        .expect("内置名称耗尽后应生成 AgentA");
    let cards = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| {
                let CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                Some(spec.clone())
            })
            .collect::<Vec<_>>()
    });

    assert_eq!(
        definitions.len(),
        31,
        "30 个长期角色后快速创建应得到第 31 条 Definition"
    );
    assert_eq!(
        generated.runtime_profile,
        canvas::AgentProfile::Claude,
        "Cmd+2 生成的 AgentA 必须使用 Claude profile"
    );
    assert_eq!(
        generated.origin,
        storage::AgentDefinitionOrigin::QuickCard,
        "第 31 条后备名称 Definition 必须标记为 QuickCard"
    );
    assert_eq!(cards.len(), 1, "Cmd+2 后 Canvas 应恰好出现 1 张 Agent Card");
    assert_eq!(
        cards[0].alias.as_deref(),
        Some("AgentA"),
        "第 31 张快速 Card 标题应使用后备名称 AgentA"
    );
    assert_eq!(
        cards[0].definition_id,
        Some(generated.id),
        "AgentA Card 必须绑定第 31 条 QuickCard Definition"
    );
    assert!(
        definitions
            .iter()
            .all(|definition| !definition.name.is_empty()),
        "内置名称耗尽后不得写入空名称"
    );
}

/// L3-A-1：真实 Agent CLI/Hook 冒烟入口，仅在显式配置环境后运行。
///
/// 数据构造（含关键数值的推导过程）：
///   required env=1 个：NORMA_E2E_AGENT_CLI=codex|claude|opencode
///   本默认测试环境不假设已安装 CLI、有效凭据或 Provider Hook
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取显式环境变量；缺失时用清楚原因跳过真实外部流程
///   2. 环境存在时验证值属于三个受支持 profile，作为外部 runner 前置检查
///
/// 预期结果：
///   - 正断言：显式配置值必须是受支持 CLI
///   - 负断言：默认 `cargo test -p e2e` 不得误启动真实 CLI 或消费用户凭据
#[test]
#[ignore = "L3 需要真实 Codex/Claude/OpenCode CLI、凭据与已安装 Agent Hook"]
fn real_agent_cli_and_hook_environment_preflight() {
    let profile = std::env::var("NORMA_E2E_AGENT_CLI")
        .expect("运行 ignored L3 前必须设置 NORMA_E2E_AGENT_CLI=codex|claude|opencode");
    assert!(
        matches!(profile.as_str(), "codex" | "claude" | "opencode"),
        "NORMA_E2E_AGENT_CLI 必须选择 codex、claude 或 opencode"
    );
    assert_ne!(profile, "", "L3 profile 不得为空，否则无法选择真实 CLI");
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` should have debug bounds before click"));
    let position = bounds.center();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}
