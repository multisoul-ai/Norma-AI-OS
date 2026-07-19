//! Headless GPUI E2E：验证 Agent Settings 通过平台文本输入协议接收并持久化中文。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// E2E-A-IME-1：Agent Settings 的 Name 与 Workflow 必须接收 macOS 平台文本输入并保存中文。
///
/// 数据构造（含关键数值的推导过程）：
///   initial definitions = 0 条
///   default name        = "Alan"（4 个 ASCII 字符）
///   Chinese name        = "小诺"（2 个汉字，满足中文呼叫名 2–8 字限制）
///   Chinese workflow    = "负责中文需求"（6 个汉字，长度 6 <= 2000）
///   final definitions   = 1 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless GPUI app，进入临时 workspace，并打开 Settings → Agents。
///   2. 点击 New 创建默认名为 Alan 的草稿，再点击 Name 输入区域。
///   3. 发送 Cmd+A 后调用 `simulate_input("小诺")`，经当前焦点的 GPUI 平台文本输入协议上屏。
///   4. 按 Enter 提交名称；点击 Workflow 后用同一平台文本输入协议写入中文工作流。
///   5. 点击 Save，并从 app 快照与项目 SQLite 读取保存结果。
///
/// 预期结果：
///   - 正断言：app 与 SQLite 都精确保存中文名称“小诺”和中文 Workflow“负责中文需求”。
///   - 负断言：默认名称 Alan 不应残留，Workflow 不应保持为空或丢失中文。
#[test]
fn agent_settings_name_and_workflow_accept_chinese_platform_text_input() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("中文 Agent 输入 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-agents");
    click_debug(cx, "settings-agent-new");
    click_debug(cx, "settings-agent-name-input");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("小诺");
    cx.simulate_keystrokes("enter");
    click_debug(cx, "settings-agent-workflow-input");
    cx.simulate_input("负责中文需求");
    click_debug(cx, "settings-agent-save");

    let saved = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    let persisted = storage::ProjectStore::for_workspace(workspace.path())
        .expect("中文 Agent 输入 e2e 应重新打开 ProjectStore")
        .agent_definition_store()
        .list()
        .expect("中文 Agent 保存后应读取定义");

    assert_eq!(
        saved.definitions.len(),
        1,
        "保存中文 Agent 后 app 名册应恰好包含 1 条定义，snapshot: {saved:?}"
    );
    assert_eq!(
        saved.definitions[0].name, "小诺",
        "Name 应通过平台文本输入保存中文，而不是保留默认名 Alan，snapshot: {saved:?}"
    );
    assert_ne!(
        saved.definitions[0].name, "Alan",
        "中文输入提交后不应继续保存默认名称 Alan，snapshot: {saved:?}"
    );
    assert_eq!(
        saved.draft_workflow, "负责中文需求",
        "Workflow 应通过平台文本输入保留完整中文，snapshot: {saved:?}"
    );
    assert_ne!(
        saved.draft_workflow, "",
        "Workflow 输入中文并保存后不应仍为空，snapshot: {saved:?}"
    );
    assert_eq!(
        persisted.len(),
        1,
        "保存中文 Agent 后 SQLite 应恰好包含 1 条定义"
    );
    assert_eq!(
        persisted[0].name, "小诺",
        "SQLite 应精确持久化中文 Agent 名称"
    );
    assert_eq!(
        persisted[0].workflow_description, "负责中文需求",
        "SQLite 应精确持久化中文 Workflow"
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
