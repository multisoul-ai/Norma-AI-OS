//! Agent 卡片队列 UI：标题栏等待数量、锚定浮层、详情、添加与撤回闭环。

use canvas::CanvasRect;
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// 卡片队列概览：标题栏只统计 waiting，点击后浮层展示 current、FIFO 与详情。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent = 1：Adam/Codex。
///   tasks = current(1) + waiting(2) = 3；标题栏 QUEUE = 3 - current(1) = 2。
///   waiting order = “补充失败路径测试” → “更新 API 文档”。
///   panel/card overlap = 0px：浮层必须完全位于卡片左侧或右侧。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建未激活 Adam，并连续派发 3 条 Norma 任务。
///   2. 等待真实 headless GPUI 渲染标题栏。
///   3. 点击 `QUEUE 2` 胶囊，读取浮层、current 与两条 waiting 的 debug bounds。
///   4. 点击第一条 waiting，读取展开的任务详情。
///
/// 预期结果：
///   - 标题栏显示 QUEUE 2，不显示 QUEUE 3，证明 current 未计入等待数量。
///   - 浮层打开前不存在，点击后出现且不遮挡 Agent 卡片。
///   - current、两条 waiting 与 Norma 来源均可见，FIFO 顺序稳定。
///   - 点击第一条 waiting 后只展开该任务详情。
#[test]
fn agent_queue_badge_opens_anchored_panel_and_waiting_task_detail() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent queue UI e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    create_inactive_agent(&app, cx, "Adam");
    let current = dispatch(&app, cx, "Adam", "重构用户认证流程");
    set_agent_card_bounds(&app, cx, current.node_id);
    dispatch(&app, cx, "Adam", "补充失败路径测试");
    dispatch(&app, cx, "Adam", "更新 API 文档");
    cx.run_until_parked();

    let queue_two = "agent-queue-badge-count-2";
    let queue_three = "agent-queue-badge-count-3";
    let panel = "agent-queue-panel";
    assert!(
        cx.debug_bounds(queue_two).is_some(),
        "3 个任务中的 1 个 current 必须被排除，标题栏应显示 QUEUE 2"
    );
    assert!(
        cx.debug_bounds(queue_three).is_none(),
        "标题栏不得显示 QUEUE 3，否则 current 被错误计入 waiting"
    );
    assert!(
        cx.debug_bounds(panel).is_none(),
        "用户点击 QUEUE 胶囊前不应永久占用画布显示浮层"
    );

    click_debug(cx, queue_two);

    assert!(
        cx.debug_bounds("agent-card").is_some(),
        "Adam Agent card 应存在于画布"
    );
    let panel_bounds = cx
        .debug_bounds(panel)
        .expect("点击 QUEUE 2 后应渲染锚定侧边浮层");
    let panel_left = f32::from(panel_bounds.origin.x);
    // card_right = card_left(48) + card_width(620) = 668；
    // panel_left = card_right(668) + sidecar_gap(22) = 690。
    let card_right = 48.0 + 620.0;

    assert!(
        panel_left >= card_right,
        "队列浮层不得遮挡 Agent 内容：card_right={card_right} panel_left={panel_left}"
    );
    assert!(
        cx.debug_bounds("agent-queue-current").is_some(),
        "浮层必须独立展示当前正在执行的任务"
    );
    assert!(
        cx.debug_bounds("agent-queue-row-0").is_some(),
        "浮层必须展示 FIFO 第一条等待任务"
    );
    assert!(
        cx.debug_bounds("agent-queue-row-1").is_some(),
        "浮层必须展示 FIFO 第二条等待任务"
    );
    assert!(
        cx.debug_bounds("agent-queue-source-0-norma").is_some(),
        "第一条等待任务应展示真实来源 Norma"
    );
    assert!(
        cx.debug_bounds("agent-queue-detail-0").is_none(),
        "点击任务前不应默认展开详情，避免列表信息过载"
    );

    click_debug(cx, "agent-queue-row-0");

    assert!(
        cx.debug_bounds("agent-queue-detail-0").is_some(),
        "点击第一条等待任务后必须展开该任务详情"
    );
    assert!(
        cx.debug_bounds("agent-queue-detail-1").is_none(),
        "查看第一条任务时不得错误展开第二条任务详情"
    );
}

/// 队列编辑闭环：从浮层手动添加任务，并精确撤回一条 waiting。
///
/// 数据构造（含关键数值的推导过程）：
///   initial tasks = current(1) + waiting(1) = 2，因此初始标题栏 QUEUE = 1。
///   add tasks = 1，因此添加后 waiting = 1 + 1 = 2。
///   withdraw tasks = 1，因此撤回后 waiting = 2 - 1 = 1。
///   被撤回目标 = 原 FIFO 第 1 条“补充失败路径测试”，保留目标 = “运行回归检查”。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Adam，派发 current 与一条 waiting，打开 QUEUE 1 浮层。
///   2. 点击“添加任务”，先空提交验证错误，再输入“运行回归检查”并提交。
///   3. 检查错误消失、QUEUE 2、第二条 waiting 及“手动”来源。
///   4. 点击第一条 waiting 的撤回按钮，再按原 task_ref 查询终态。
///
/// 预期结果：
///   - 正断言：添加任务进入 FIFO 尾部，等待数从 1 增至 2，来源为手动。
///   - 正断言：撤回后等待数回到 1，原任务状态为 cancelled。
///   - 负断言：撤回不得影响新添加的任务，且标题栏不得继续显示 QUEUE 2。
#[test]
fn agent_queue_adds_manual_task_and_withdraws_one_waiting_task() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent queue edit e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    create_inactive_agent(&app, cx, "Adam");
    let current = dispatch(&app, cx, "Adam", "重构用户认证流程");
    set_agent_card_bounds(&app, cx, current.node_id);
    let waiting = dispatch(&app, cx, "Adam", "补充失败路径测试");
    cx.run_until_parked();

    click_debug(cx, "agent-queue-badge-count-1");
    click_debug(cx, "agent-queue-add");

    assert!(
        cx.debug_bounds("agent-queue-add-input").is_some(),
        "点击添加任务后必须出现可聚焦的原生文本输入框"
    );
    click_debug(cx, "agent-queue-add-submit");
    assert_eq!(
        app.update(cx, |app, _cx| app.workspace_error_for_e2e()),
        Some("队列任务不能为空".to_string()),
        "空任务必须保留添加表单并显示明确错误"
    );
    click_debug(cx, "agent-queue-add-input");
    cx.simulate_input("运行回归检查");
    click_debug(cx, "agent-queue-add-submit");

    assert_eq!(
        app.update(cx, |app, _cx| app.workspace_error_for_e2e()),
        None,
        "有效任务加入队列后必须清除先前的空任务错误"
    );
    assert!(
        cx.debug_bounds("agent-queue-badge-count-2").is_some(),
        "添加一条任务后 waiting 必须从 1 增至 2"
    );
    assert!(
        cx.debug_bounds("agent-queue-row-1").is_some(),
        "手动添加的任务必须位于原 waiting 之后，保持 FIFO"
    );
    assert!(
        cx.debug_bounds("agent-queue-source-1-manual").is_some(),
        "从卡片浮层添加的任务必须展示来源“手动”"
    );

    click_debug(cx, "agent-queue-cancel-0");

    let cancelled = execute(
        &app,
        cx,
        CardCommand::AgentTaskStatus {
            task_ref: waiting.task_id.clone(),
        },
    )
    .expect("撤回后的原 waiting task_ref 应仍可查询终态");
    let CardCommandResult::AgentTaskStatus { task } = cancelled else {
        panic!("撤回后 status 必须返回 AgentTaskStatus");
    };
    assert_eq!(
        task.status,
        AgentTaskReceiptStatus::Cancelled,
        "点击撤回必须把指定 waiting 任务精确推进到 cancelled"
    );
    assert_ne!(
        task.status,
        AgentTaskReceiptStatus::CancelRequested,
        "waiting 任务从未提交，撤回不得误报为运行中取消请求"
    );
    assert!(
        cx.debug_bounds("agent-queue-badge-count-1").is_some(),
        "撤回一条 waiting 后标题栏数量必须从 2 回到 1"
    );
    assert!(
        cx.debug_bounds("agent-queue-row-0").is_some(),
        "撤回原第一条后，新添加任务必须保留并前移为 FIFO 第一条"
    );
}

fn create_inactive_agent(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut VisualTestContext,
    name: &str,
) {
    let result = execute(
        app,
        cx,
        CardCommand::CreateAgent {
            name: Some(name.to_string()),
            profile: AgentProfile::Codex,
            activate: false,
        },
    );
    assert!(
        matches!(
            result,
            Ok(CardCommandResult::AgentCreated { node_id: None, .. })
        ),
        "测试前置必须成功创建未激活 Agent，actual={result:?}"
    );
}

fn dispatch(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut VisualTestContext,
    name: &str,
    task: &str,
) -> norma::protocol::AgentTaskReceipt {
    let result = execute(
        app,
        cx,
        CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: name.to_string(),
                task: task.to_string(),
            }],
            force_untracked: false,
        },
    )
    .expect("测试任务派发应成功");
    let CardCommandResult::AgentTasksDispatched { mut receipts } = result else {
        panic!("任务派发必须返回 AgentTasksDispatched");
    };
    assert_eq!(
        receipts.len(),
        1,
        "单个 assignment 必须产生且只产生一条任务回执"
    );
    receipts.pop().expect("长度已断言为 1，任务回执必须存在")
}

fn execute(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut VisualTestContext,
    command: CardCommand,
) -> Result<CardCommandResult, String> {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let result = app.execute_agent_card_command_for_e2e(window, command);
            cx.notify();
            result
        })
    })
}

fn set_agent_card_bounds(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut VisualTestContext,
    node_id: u64,
) {
    app.update(cx, |app, cx| {
        app.runtime
            .document
            .set_node_bounds(node_id, CanvasRect::new(48.0, 48.0, 620.0, 420.0));
        cx.notify();
    });
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 应在点击前存在 debug bounds"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
