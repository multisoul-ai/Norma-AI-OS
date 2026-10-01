use super::*;
use canvas::AgentProfile;
use std::collections::HashSet;
use storage::{AgentDefinition, AgentDefinitionOrigin};

fn definition(id: i64, name: &str) -> AgentDefinition {
    AgentDefinition {
        id,
        name: name.to_string(),
        speech_alias: None,
        runtime_profile: AgentProfile::Codex,
        workflow_description: String::new(),
        enabled: true,
        sort_order: 0,
        origin: AgentDefinitionOrigin::Explicit,
    }
}

/// AS-GROUP-1：Agents 左栏按“是否已在 Canvas 上”分成 ON CANVAS 与 LIBRARY 两组，组内保持原顺序。
///
/// 数据构造（含关键数值的推导过程）：
///   definitions = 4 条：Alan(1)、Bella(2)、Cora(3)、Dana(4)，输入顺序即列表顺序
///   on canvas ids = {2, 4}，共 2 个
///   预期 ON CANVAS = 2 条（Bella、Dana），LIBRARY = 4 - 2 = 2 条（Alan、Cora）
///
/// 执行过程：
///   1. 把 4 条定义与 {2, 4} 交给 group_definitions。
///   2. 逐条读取两组的 id 序列。
///
/// 预期结果：
///   - 正断言：ON CANVAS 依次为 2、4；LIBRARY 依次为 1、3。
///   - 负断言：任何 Agent 不得同时出现在两组；总数不得丢失（2 + 2 = 4）。
#[test]
fn test_group_definitions_splits_on_canvas_from_library_preserving_order() {
    let definitions = [
        definition(1, "Alan"),
        definition(2, "Bella"),
        definition(3, "Cora"),
        definition(4, "Dana"),
    ];
    let on_canvas: HashSet<i64> = [2, 4].into_iter().collect();

    let groups = group_definitions(definitions.iter().collect(), &on_canvas);
    let canvas_ids: Vec<i64> = groups.on_canvas.iter().map(|d| d.id).collect();
    let library_ids: Vec<i64> = groups.library.iter().map(|d| d.id).collect();

    assert_eq!(
        canvas_ids,
        vec![2, 4],
        "ON CANVAS 应只含 Bella 与 Dana 且保持顺序"
    );
    assert_eq!(
        library_ids,
        vec![1, 3],
        "LIBRARY 应只含 Alan 与 Cora 且保持顺序"
    );
    assert!(
        canvas_ids.iter().all(|id| !library_ids.contains(id)),
        "同一 Agent 不得同时出现在两个分组"
    );
    assert_eq!(
        canvas_ids.len() + library_ids.len(),
        4,
        "分组不得丢失 Agent"
    );
}

/// AS-GROUP-2：左栏副标题描述项目内 Agent 总数与 Canvas 上的数量，并处理单复数。
///
/// 数据构造：
///   (total, on_canvas) = (12, 2)、(1, 0)、(0, 0)，分别覆盖复数、单数、空项目
///   on_canvas 仅统计仍存在于总数内的 Agent，本函数直接采用调用方给出的数量
///
/// 执行过程：依次调用 roster_summary 并比较文案。
///
/// 预期结果：
///   - 正断言：12/2 → "12 agents · 2 on Canvas"；1/0 → "1 agent · 0 on Canvas"；0/0 → "No agents yet"。
///   - 负断言：单个 Agent 不得写成 "1 agents"。
#[test]
fn test_roster_summary_formats_counts_and_plurals() {
    assert_eq!(
        roster_summary(12, 2),
        "12 agents · 2 on Canvas",
        "多个 Agent 应使用复数并附带 Canvas 数量"
    );
    assert_eq!(
        roster_summary(1, 0),
        "1 agent · 0 on Canvas",
        "单个 Agent 应使用单数"
    );
    assert_eq!(
        roster_summary(0, 0),
        "No agents yet",
        "空项目应给出空状态文案"
    );
    assert_ne!(
        roster_summary(1, 0),
        "1 agents · 0 on Canvas",
        "不得出现 1 agents"
    );
}
