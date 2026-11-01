use super::*;

/// 场景：模型用编号步骤和 Markdown 动词标签，原生展开区仍须呈现原型中的行动要点。
/// 构造：三条编号步骤、目标和验收，包含不得推送的限制。
/// 执行：使用生产 task_points 提取展示内容。
/// 预期：三条完整步骤全部可见，否定保留，目标和验收不混入步骤。
#[test]
fn proposal_numbered_steps_are_visible_without_changing_the_task() {
    let task = "目标：修复阻塞 CI 的脚本\n1. **核对证据**：检查失败日志。\n2) **修复脚本**：只改已确认错误，不得推送。\n3. **补齐测试**：运行聚焦回归。\n验收：CI 通过且提交可追溯。";
    assert_eq!(
        task_points(task),
        [
            "**核对证据**：检查失败日志。",
            "**修复脚本**：只改已确认错误，不得推送。",
            "**补齐测试**：运行聚焦回归。"
        ],
        "编号步骤不能被静默丢弃，否则展开区只剩理由与原文入口"
    );
    assert!(
        task.contains("不得推送"),
        "展示提取不改变完整任务和授权限制"
    );
}

/// 场景：展开区的目标和交付必须取自原始任务，短的历史段落也能直接阅读。
/// 构造：中英文显式事实、短段落和超长未结构化指令。
/// 执行：调用生产 task_summary，检查事实与步骤独立投影。
/// 预期：显式验收和否定完整保留；不猜测长段落的目标或交付。
#[test]
fn proposal_summary_preserves_explicit_facts_and_legacy_text() {
    let task = "目标：修复 CI\n1. 排查脚本。\n2. 不得推送。\n验收：聚焦回归通过。";
    let summary = parse(task);
    assert_eq!(summary.goal, Some("修复 CI"), "目标应来自显式字段");
    assert_eq!(summary.output, Some("聚焦回归通过。"), "验收不能被吞进原文");
    assert_eq!(summary.steps, ["排查脚本。", "不得推送。"], "步骤保留限制");
    let legacy = parse("为登录接口写使用文档，不得推送");
    assert_eq!(
        legacy.goal,
        Some("为登录接口写使用文档，不得推送"),
        "历史短段落直接可读"
    );
    assert!(legacy.output.is_none(), "不能为历史内容编造交付");
    let long = "不得推送；".repeat(30);
    assert!(parse(&long).goal.is_none(), "长段落不得硬截以免丢失限制");
    assert_eq!(
        parse("Goal: Fix CI\n- Verify logs\nOutput: Regression passes").output,
        Some("Regression passes"),
        "英文事实同样可读"
    );
}
