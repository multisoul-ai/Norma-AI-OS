//! E2E-12：Agent 卡片标题必须保留稳定身份，并把长任务限制在单行详情区域。

use gpui::{px, TestAppContext};
use runtime::{AgentTaskState, AgentTaskUpdate};

use crate::harness::{create_codex_agent, echo_factory, enter_canvas, make_app};

/// E2E-12：短任务切换成长且多行的任务后，Codex 名称尺寸必须保持不变且详情留在标题栏内。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards = 1；稳定名称 = "Codex"（5 个 ASCII 字符）
///   baseline task_title = "短任务"（3 个汉字）
///   long task_title = 第一行 20 个汉字 + LF(1) + 第二行 16 个汉字 = 37 个 Unicode scalar
///   identity size delta = 0px × 0px（任务正文只能占用独立的剩余宽度）
///   title detail logical lines = 1（生产 chrome 在渲染前折叠 LF）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 装配 headless GPUI App，进入 Canvas 并通过公开 action 创建 Codex Agent。
///   2. 注入短任务并从真实渲染树读取 card-title 与 card-titlebar bounds。
///   3. 先用 Ready 开启新 session，再注入包含 LF 的长 UserPromptSubmit 并通知 GPUI 重绘。
///   4. 再次读取 card-title、card-title-detail、card-titlebar 与关闭按钮 bounds。
///
/// 预期结果：
///   - 正断言：加入长任务前后 Codex 名称 bounds.size 完全相同，身份没有被挤压。
///   - 正断言：任务详情位于名称右侧，且其高度不超过固定标题栏。
///   - 正断言：任务详情右边界不越过关闭按钮，溢出只能在自身区域内截断。
///   - 负断言：任务详情与名称不能重叠，也不能获得 0px 宽度。
#[test]
fn e2e_12_agent_name_keeps_its_size_with_long_multiline_task_title() {
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Agent 标题 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);

    app.update(cx, |app, cx| {
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "UserPromptSubmit".to_string(),
            to: AgentTaskState::Thinking,
            task_title: Some("短任务".to_string()),
            session_id: None,
        });
        assert!(changed, "短任务应建立 Agent 名称固有宽度的渲染基准");
        cx.notify();
    });
    cx.run_until_parked();

    let baseline_name = cx
        .debug_bounds("card-title")
        .expect("短任务时 Codex Agent 应渲染稳定 card-title");
    let baseline_titlebar = cx
        .debug_bounds("card-titlebar")
        .expect("Codex Agent 应渲染固定 card-titlebar");

    let multiline_task =
        "请分析这个非常长的用户输入并保留代理名称\n然后继续处理第二行且标题不要换行";
    app.update(cx, |app, cx| {
        let ready_changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "SessionStart".to_string(),
            to: AgentTaskState::Ready,
            task_title: None,
            session_id: None,
        });
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "UserPromptSubmit".to_string(),
            to: AgentTaskState::Thinking,
            task_title: Some(multiline_task.to_string()),
            session_id: None,
        });
        assert!(
            ready_changed,
            "Ready 应结束短任务 session，使长任务成为新的锁定标题"
        );
        assert!(changed, "长多行 task_title 应更新 Codex Agent runtime 状态");
        cx.notify();
    });
    cx.run_until_parked();

    let name = cx
        .debug_bounds("card-title")
        .expect("长任务存在时 Codex 名称仍应渲染");
    let detail = cx
        .debug_bounds("card-title-detail")
        .expect("长任务应渲染独立 card-title-detail");
    let titlebar = cx
        .debug_bounds("card-titlebar")
        .expect("长任务存在时固定 card-titlebar 仍应渲染");
    let close = cx
        .debug_bounds("node-close")
        .expect("Agent 标题栏关闭按钮应保留固定操作区域");

    assert_eq!(
        name.size, baseline_name.size,
        "长 task_title 前后 Codex 名称尺寸必须保持 0px × 0px 变化"
    );
    assert_eq!(
        titlebar.size.height, baseline_titlebar.size.height,
        "显式 LF 不得把固定标题栏撑高"
    );
    assert!(
        detail.origin.x >= name.origin.x + name.size.width,
        "任务详情必须从稳定 Agent 名称右侧开始，不能覆盖名称"
    );
    assert!(
        detail.size.width > px(0.0),
        "任务详情应获得正宽度的独立截断区域，不能被控件完全挤没"
    );
    assert!(
        detail.size.height <= titlebar.size.height,
        "任务详情必须保持在单行标题栏高度内"
    );
    assert!(
        detail.origin.x + detail.size.width <= close.origin.x,
        "任务详情右边界不得越过关闭按钮，长文本应在自身区域截断"
    );
    assert_ne!(
        detail.origin, name.origin,
        "任务详情与 Agent 名称必须是两个不重叠的布局区域"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
