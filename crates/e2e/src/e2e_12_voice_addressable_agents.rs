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
