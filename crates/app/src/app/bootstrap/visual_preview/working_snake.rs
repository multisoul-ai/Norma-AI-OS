use crate::app::MultisoulApp;
use runtime::{AgentTaskState, AgentTaskUpdate};

pub(super) fn configure(app: &mut MultisoulApp) {
    // initial_window_dimensions(working-snake) = 1280 × 800；固定 reflow 基线，确保
    // 620 × 420px 卡片保持在可观察的居中尺寸，不被首帧铺满整个 Canvas。
    app.last_reflow_window_size = Some((1280.0, 800.0));
    let definition = app.agent_definitions.use_queue_visual_preview();
    let node_id = app.runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Codex,
            alias: Some("Adam".to_string()),
            definition_id: Some(definition.id),
        }),
        canvas::CanvasRect::new(330.0, 150.0, 620.0, 420.0),
    );
    let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Codex,
        provider_event: "UserPromptSubmit".to_string(),
        to: AgentTaskState::Thinking,
        task_title: Some("验证 Working Snake 局部 60 FPS 动画".to_string()),
        session_id: Some("working-snake-visual-preview".to_string()),
    });
    debug_assert!(changed, "visual preview Agent task state should change");
}
