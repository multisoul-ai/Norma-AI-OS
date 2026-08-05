//! Headless GPUI E2E：验证 Agent Settings 的 Agent Library、统一 Inspector 与 CRUD 流程。

use canvas::CanvasNodeKind;
use gpui::{
    point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext,
};

use crate::harness::{echo_factory, enter_canvas, make_app, project_store, read_runtime};

/// E2E-A-SETTINGS-MASTER-DETAIL-1：Agent Settings 默认显示统一 Inspector，并按 dirty 状态展示保存栏。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions = 2 条：Alan/Codex + Bella/OpenCode
///   theme switches     = 2 次：Paper Blue → Forest Mint → Paper Blue，确保真实经过深浅切换
///   initial Agent cards = 0 张
///   Name edit           = Alan → Alana，增加 1 个字符后进入 dirty
///   Discard click       = 1 次，恢复 Alan 并退出 dirty
///   Add to Canvas click = 1 次，最终 Agent cards = 0 + 1 = 1 张
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向临时 workspace 写入 Alan 与 Bella，再启动 headless App 并进入 Canvas。
///   2. 打开 Settings → Themes，先切到 Forest Mint，再切回 Paper Blue。
///   3. 进入 Agents，检查双栏、Alan/Codex 列表标签和统一 Inspector。
///   4. 确认 Name、Workflow、Add to Canvas 与 Delete Agent 常驻，Edit action 不再存在。
///   5. 修改 Name，检查 dirty 保存栏出现；点击 Discard 后检查保存栏消失。
///   6. 点击 Add to Canvas。
///   7. 从 Runtime 读取 Agent 卡片，核对它绑定 Alan definition。
///
/// 预期结果：
///   - 正断言：进入 Agents 时仍是 Paper Blue 浅色主题，且 Codex 标签有真实渲染 bounds。
///   - 负断言：切回 Paper Blue 后不得残留 dark mode。
///   - 正断言：默认存在 master-detail、左侧列表、统一 Inspector、表单、Add to Canvas 与 Delete Agent。
///   - 负断言：不得继续渲染只读详情或显式 Edit action。
///   - 正断言：修改后出现 Save Changes 与 Discard，Discard 后恢复干净状态。
///   - 正断言：Add to Canvas 创建恰好 1 张绑定 Alan 的卡片。
///   - 负断言：Add to Canvas 不得创建未绑定 definition 的匿名 Agent 卡片。
#[test]
fn agent_settings_uses_unified_inspector_and_can_discard_then_add_to_canvas() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent Settings 双栏 e2e 应创建临时 workspace");
    let store = project_store(&app, cx, workspace.path()).agent_definition_store();
    let alan = store
        .create(&storage::NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: "负责工程实现与验证。".to_string(),
        })
        .expect("测试应创建 Alan/Codex definition");
    store
        .create(&storage::NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: canvas::AgentProfile::OpenCode,
            workflow_description: "负责代码审查。".to_string(),
        })
        .expect("测试应创建 Bella/OpenCode definition");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-themes");
    click_debug(cx, "settings-theme-forest-mint");
    click_debug(cx, "settings-theme-paper-blue");
    let light_theme = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        light_theme.active_theme_id, "paper-blue",
        "进入 Agent Settings 前应明确切回 Paper Blue 浅色主题"
    );
    assert!(
        !light_theme.theme_is_dark,
        "切回 Paper Blue 后不得残留 Forest Mint 的 dark mode"
    );
    click_debug(cx, "settings-section-agents");

    assert!(
        cx.debug_bounds("settings-agents-master-detail").is_some(),
        "Agents 页面默认应渲染主从双栏容器"
    );
    assert!(
        cx.debug_bounds("settings-agents-list").is_some(),
        "主从双栏左侧应渲染可选择的 Agent 列表"
    );
    assert!(
        cx.debug_bounds("settings-agent-list-runtime-profile")
            .is_some(),
        "Paper Blue 下 Alan 行必须渲染可见的 Codex Runtime Profile 标签"
    );
    assert!(
        cx.debug_bounds("settings-agent-inspector").is_some(),
        "主从双栏右侧默认应渲染统一 Agent Inspector"
    );
    assert!(
        cx.debug_bounds("settings-agent-add-to-canvas").is_some(),
        "统一 Inspector 应保留 Add to Canvas action"
    );
    assert!(
        cx.debug_bounds("settings-agent-delete").is_some(),
        "统一 Inspector 底部应显式提供 Delete Agent action"
    );
    assert!(
        cx.debug_bounds("settings-agent-name-input").is_some(),
        "统一 Inspector 默认应直接显示 Name 编辑区域"
    );
    assert!(
        cx.debug_bounds("settings-agent-workflow-input").is_some(),
        "统一 Inspector 默认应直接显示 Workflow 编辑区域"
    );
    assert!(
        cx.debug_bounds("settings-agent-edit").is_none(),
        "统一 Inspector 不应继续要求用户先点击 Edit"
    );
    assert!(
        cx.debug_bounds("settings-agent-save").is_none(),
        "未修改 Agent 时不应显示 Save Changes action"
    );

    click_debug(cx, "settings-agent-name-input");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("Alana");
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("settings-agent-save").is_some(),
        "修改 Name 后应显示 Save Changes action"
    );
    assert!(
        cx.debug_bounds("settings-agent-discard").is_some(),
        "修改 Name 后应同时显示 Discard action"
    );

    click_debug(cx, "settings-agent-discard");
    let discarded = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert_eq!(
        discarded.draft_name, "Alan",
        "Discard 应恢复 Alan，而不是保留临时名称 Alana"
    );
    assert_ne!(
        discarded.draft_name, "Alana",
        "Discard 后临时名称 Alana 不应继续留在 draft"
    );
    assert!(
        !discarded.dirty,
        "Discard 后 Agent draft 应恢复干净状态，不能继续保留 dirty"
    );

    click_debug(cx, "settings-agent-add-to-canvas");

    let agent_specs = read_runtime(&app, cx, |runtime| {
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
        agent_specs.len(),
        1,
        "一次 Add to Canvas 应创建恰好 1 张 Agent 卡片"
    );
    assert_eq!(
        agent_specs[0].definition_id,
        Some(alan.id),
        "Add to Canvas 应创建绑定当前选中 Alan definition 的卡片"
    );
    assert_eq!(
        agent_specs[0].alias.as_deref(),
        Some("Alan"),
        "Add to Canvas 创建的卡片应显示当前选中 Agent 名称 Alan"
    );
    assert!(
        agent_specs[0].definition_id.is_some(),
        "Add to Canvas 不得创建 definition_id=None 的匿名 Agent 卡片"
    );
}

/// E2E-A-SETTINGS-MASTER-DETAIL-2：左侧 Agent 列表独立滚动，右侧详情保持固定。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions = 18 条
///   row minimum height  = 68 px
///   row gaps            = 17 × 3 px = 51 px
///   rows total          ≥ 18 × 68 + 51 = 1,275 px，稳定超过 Settings 内容区高度
///   scroll delta        = -20,000 px，远大于 1,275 px，足以滚到列表末尾
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向临时 workspace 写入 18 条 Agent definition，再打开 Settings → Agents。
///   2. 记录左侧列表 viewport、右侧详情与首末行的初始 bounds。
///   3. 在左侧列表中心向下滚动 20,000 px，并等待 GPUI 完成重绘。
///   4. 再次读取左右区域与首末行 bounds，比较滚动前后的空间位置。
///
/// 预期结果：
///   - 正断言：左侧列表 viewport 高度受主从容器约束，末行滚动后进入 viewport。
///   - 正断言：列表 viewport 与右侧详情 bounds 在滚动前后完全不变。
///   - 负断言：首行滚动后不应仍完整位于 viewport 内。
///   - 负断言：右侧详情不得跟随左侧列表发生任何位移。
#[test]
fn agent_settings_scrolls_the_agent_list_without_moving_the_detail_pane() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent Settings 独立滚动 e2e 应创建临时 workspace");
    let store = project_store(&app, cx, workspace.path()).agent_definition_store();
    let names = [
        "Alan", "Bella", "Cora", "Della", "Eileen", "Flora", "Greta", "Hazel", "Iris", "Julia",
        "Kira", "Luna", "Mabel", "Nora", "Olive", "Piper", "Quinn", "Ruby",
    ];
    let mut definition_ids = Vec::new();
    for (index, name) in names.into_iter().enumerate() {
        let definition = store
            .create(&storage::NewAgentDefinition {
                name: name.to_string(),
                runtime_profile: canvas::AgentProfile::Codex,
                workflow_description: format!("负责第 {index:02} 项工程任务。"),
            })
            .expect("测试应创建用于撑开左侧列表的 Agent definition");
        definition_ids.push(definition.id);
    }
    enter_canvas(&app, cx, workspace.path());
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-agents");

    let list_before = cx
        .debug_bounds("settings-agents-list")
        .expect("Agents 页面必须渲染左侧列表 viewport");
    let master_detail = cx
        .debug_bounds("settings-agents-master-detail")
        .expect("Agents 页面必须渲染主从双栏容器");
    let detail_before = cx
        .debug_bounds("settings-agent-detail")
        .expect("Agents 页面必须渲染右侧详情区域");
    let first_selector: &'static str =
        Box::leak(format!("settings-agent-list-row-{}", definition_ids[0]).into_boxed_str());
    let last_selector: &'static str =
        Box::leak(format!("settings-agent-list-row-{}", definition_ids[17]).into_boxed_str());
    let first_before = cx
        .debug_bounds(first_selector)
        .expect("滚动前应渲染第 1 条 Agent 行");
    let last_before = cx
        .debug_bounds(last_selector)
        .expect("滚动前应渲染第 18 条 Agent 行");

    assert!(
        list_before.size.height <= master_detail.size.height,
        "左侧列表 viewport 高度必须受主从容器约束；list={list_before:?}, master={master_detail:?}"
    );
    assert!(
        first_before.top() >= list_before.top() && first_before.bottom() <= list_before.bottom(),
        "滚动前第 1 条 Agent 行应完整位于列表 viewport；first={first_before:?}, list={list_before:?}"
    );
    assert!(
        last_before.bottom() > list_before.bottom(),
        "滚动前第 18 条 Agent 行不应已完整进入首屏 viewport；last={last_before:?}, list={list_before:?}"
    );

    cx.simulate_event(ScrollWheelEvent {
        position: list_before.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-20_000.0))),
        ..Default::default()
    });
    cx.run_until_parked();

    let list_after = cx
        .debug_bounds("settings-agents-list")
        .expect("滚动后左侧列表 viewport 必须继续存在");
    let detail_after = cx
        .debug_bounds("settings-agent-detail")
        .expect("滚动后右侧详情区域必须继续存在");
    let first_after = cx
        .debug_bounds(first_selector)
        .expect("滚动后第 1 条 Agent 行仍应保留在列表内容中");
    let last_after = cx
        .debug_bounds(last_selector)
        .expect("滚动后应能定位第 18 条 Agent 行");

    assert_eq!(
        list_after, list_before,
        "左侧内部滚动不得移动或拉伸列表 viewport"
    );
    assert_eq!(
        detail_after, detail_before,
        "左侧列表滚动不得带动右侧详情区域移动"
    );
    assert!(
        last_after.top() >= list_after.top() && last_after.bottom() <= list_after.bottom(),
        "滚到底后第 18 条 Agent 行必须完整位于列表 viewport；last={last_after:?}, list={list_after:?}"
    );
    assert!(
        first_after.bottom() < list_after.top() || first_after.top() > list_after.bottom(),
        "滚到底后第 1 条 Agent 行不应仍位于列表 viewport；first={first_after:?}, list={list_after:?}"
    );
}

/// E2E-A-SETTINGS-MASTER-DETAIL-3：左侧搜索与状态筛选只过滤 Agent 列表，并可清空恢复。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions = 3 条：Alan(enabled)、Bella(disabled)、Quinn(enabled)
///   search query       = "quinn"（小写输入，验证大小写不敏感）
///   expected matches   = 1 条：Quinn
///   clear shortcut     = Cmd+A + Backspace，共 2 个键盘动作
///   disabled matches   = 1 条：Bella
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向临时 workspace 写入 3 条 Agent definition，再打开 Settings → Agents。
///   2. 聚焦左侧搜索框并输入小写 quinn，等待 Input Change 驱动列表重绘。
///   3. 读取 Agent Settings e2e 快照，并检查匹配行仍有真实渲染 bounds。
///   4. 在搜索框执行 Cmd+A + Backspace，检查查询清空且三条 Agent 全部恢复。
///   5. 点击 Disabled segmented filter，检查只保留 Bella。
///
/// 预期结果：
///   - 正断言：搜索框存在，quinn 只保留 Quinn，且 Quinn 行继续可见。
///   - 负断言：过滤结果不得包含 Alan 或 Bella。
///   - 正断言：清空搜索后 Alan、Bella、Quinn 全部恢复。
///   - 负断言：清空搜索不得删除任何已保存 Agent definition。
///   - 正断言：All / Enabled / Disabled 三个筛选入口存在，Disabled 只显示 Bella。
///   - 负断言：Disabled 结果不得包含 enabled 的 Alan 或 Quinn。
#[test]
fn agent_settings_search_filters_only_the_left_agent_list_and_can_be_cleared() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent Settings 搜索 e2e 应创建临时 workspace");
    let store = project_store(&app, cx, workspace.path()).agent_definition_store();
    let alan = store
        .create(&storage::NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: "负责工程实现。".to_string(),
        })
        .expect("测试应创建 Alan definition");
    let bella = store
        .create(&storage::NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: canvas::AgentProfile::OpenCode,
            workflow_description: "负责代码审查。".to_string(),
        })
        .expect("测试应创建 Bella definition");
    store
        .update(&storage::AgentDefinitionUpdate {
            id: bella.id,
            name: bella.name,
            runtime_profile: bella.runtime_profile,
            workflow_description: bella.workflow_description,
            enabled: false,
            sort_order: bella.sort_order,
        })
        .expect("测试应把 Bella 更新为 disabled");
    let quinn = store
        .create(&storage::NewAgentDefinition {
            name: "Quinn".to_string(),
            runtime_profile: canvas::AgentProfile::Cursor,
            workflow_description: "负责交互验证。".to_string(),
        })
        .expect("测试应创建 Quinn definition");
    enter_canvas(&app, cx, workspace.path());
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-agents");

    click_debug(cx, "settings-agent-search-input");
    cx.simulate_input("quinn");
    cx.run_until_parked();

    let filtered = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert_eq!(
        filtered.search_query, "quinn",
        "搜索快照应保留用户输入的小写 quinn"
    );
    assert_eq!(
        filtered.visible_definition_names.as_slice(),
        ["Quinn"],
        "大小写不敏感搜索 quinn 应只保留 Quinn"
    );
    assert!(
        !filtered
            .visible_definition_names
            .iter()
            .any(|name| name == "Alan" || name == "Bella"),
        "搜索 quinn 后左侧列表不得继续包含 Alan 或 Bella"
    );
    let quinn_selector: &'static str =
        Box::leak(format!("settings-agent-list-row-{}", quinn.id).into_boxed_str());
    assert!(
        cx.debug_bounds(quinn_selector).is_some(),
        "搜索 quinn 后匹配的 Quinn 行必须继续渲染"
    );

    click_debug(cx, "settings-agent-search-input");
    cx.simulate_keystrokes("cmd-a backspace");
    cx.run_until_parked();

    let restored = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert!(
        restored.search_query.is_empty(),
        "Cmd+A + Backspace 后搜索查询必须为空"
    );
    assert_eq!(
        restored.visible_definition_names.as_slice(),
        ["Alan", "Bella", "Quinn"],
        "清空搜索后必须恢复全部 3 条 Agent definition"
    );
    assert_eq!(
        restored.definitions.len(),
        3,
        "清空搜索不得删除任何已保存 Agent definition"
    );
    assert!(
        restored
            .definitions
            .iter()
            .any(|definition| definition.id == alan.id),
        "清空搜索后已保存的 Alan definition 必须仍然存在"
    );

    assert!(
        cx.debug_bounds("settings-agent-filter-all").is_some()
            && cx.debug_bounds("settings-agent-filter-enabled").is_some()
            && cx.debug_bounds("settings-agent-filter-disabled").is_some(),
        "Agent Library 必须同时显示 All、Enabled、Disabled 三个状态筛选入口"
    );
    click_debug(cx, "settings-agent-filter-disabled");
    let disabled = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert_eq!(
        disabled.visible_definition_names.as_slice(),
        ["Bella"],
        "点击 Disabled filter 后左侧列表应只显示 Bella"
    );
    assert!(
        !disabled
            .visible_definition_names
            .iter()
            .any(|name| name == "Alan" || name == "Quinn"),
        "Disabled filter 不得显示 enabled 的 Alan 或 Quinn"
    );
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
