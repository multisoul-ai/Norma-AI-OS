//! 桌面 E2E 控制通道专用：在宿主窗口摆出六张处于不同运行状态的 Agent 卡片。

use crate::app::{AppScreen, MultisoulApp};
use runtime::{AgentTaskState, AgentTaskUpdate};

/// 六张 Agent 卡片依次处于 准备 / 思考 / 工具调用 / 等待审批 / 完成（未查看）/ 失败，
/// 运行特效风格取自设置 `effects.run_style`。
pub(crate) fn install(app: &mut MultisoulApp) {
    app.screen = AppScreen::Canvas;
    app.settings_panel.close();
    // 1280 × 800 窗口下 3 × 2 排布，每张 360 × 230，留出磁流体的外扩空间。
    app.last_reflow_window_size = Some((1280.0, 800.0));
    let states = [
        None,
        Some(AgentTaskState::Thinking),
        Some(AgentTaskState::RunningTool {
            tool_name: Some("run_tests".to_string()),
        }),
        Some(AgentTaskState::WaitingApproval {
            tool_name: Some("Bash".to_string()),
        }),
        Some(AgentTaskState::Done),
        Some(AgentTaskState::Failed {
            message: "2 tests failed".to_string(),
        }),
    ];
    for (index, state) in states.into_iter().enumerate() {
        let (column, row) = ((index % 3) as f32, (index / 3) as f32);
        let node_id = app.runtime.document.add_node(
            canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
                profile: canvas::AgentProfile::Codex,
                alias: Some(format!("Run {}", index + 1)),
                definition_id: None,
                mobile_control: false,
            }),
            canvas::CanvasRect::new(70.0 + column * 400.0, 90.0 + row * 300.0, 360.0, 230.0),
        );
        let Some(to) = state else {
            continue;
        };
        app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "UserPromptSubmit".to_string(),
            to,
            task_title: Some("重构登录模块".to_string()),
            session_id: Some(format!("run-effects-visual-preview-{index}")),
        });
    }
}
