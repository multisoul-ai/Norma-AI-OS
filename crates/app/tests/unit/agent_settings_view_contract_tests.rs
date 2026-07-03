/// AS-18: Agents 设置页应采用直接卡片结构，移除右侧详情面板与表单式 list-detail 心智模型。
///
/// 数据构造（含关键数值的推导过程）：
///   source file count        = 2（agents/view.rs + agents/actions.rs）
///   required card ids        = 5 个，用于定位 New、卡片列表、单卡片、编辑卡片、确认删除
///   forbidden legacy anchors = 3 个，settings-agent-detail / "Draft Agent" / "Edit draft"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 agents/view.rs 和 agents/actions.rs 源码
///   2. 检查卡片式稳定锚点是否存在
///   3. 检查右侧详情面板和旧草稿区标题是否已移除
///   4. 检查双击编辑、新建即创建、保存/取消/确认删除 handler 是否已接线
///
/// 预期结果：
///   - 正断言：view 暴露 settings-agent-card / settings-agent-card-editing / on_mouse_down / click_count
///   - 正断言：actions 暴露 auto-create new、begin card edit、cancel card edit handler
///   - 负断言：view 不应继续出现 settings-agent-detail / "Draft Agent" / "Edit draft"
///   - 负断言：字段编辑不应继续依赖右侧详情面板
#[test]
fn test_agents_settings_view_uses_direct_card_contract() {
    let view_source = include_str!("../../src/settings/agents/view.rs");
    let actions_source = include_str!("../../src/settings/agents/actions.rs");

    for anchor in [
        "settings-agent-new",
        "settings-agents-list",
        "settings-agent-card",
        "settings-agent-card-editing",
        "settings-agent-delete-confirm",
    ] {
        assert!(
            view_source.contains(anchor),
            "Agents card view should expose stable anchor: {anchor}"
        );
    }
    assert!(
        view_source.contains("on_mouse_down") && view_source.contains("click_count"),
        "Agent cards should use MouseDownEvent.click_count so double-click enters inline edit mode"
    );
    for handler in [
        "on_agent_settings_new_definition",
        "on_agent_settings_begin_card_edit",
        "on_agent_settings_cancel_card_edit",
        "on_agent_settings_request_delete_definition",
    ] {
        assert!(
            actions_source.contains(handler),
            "Agents actions should wire direct-card handler: {handler}"
        );
    }
    assert!(
        !view_source.contains("settings-agent-detail"),
        "Agents view should not keep a right-side detail pane; cards edit inline"
    );
    assert!(
        !view_source.contains("\"Draft Agent\""),
        "Agents view should not keep the legacy Draft Agent section heading"
    );
    assert!(
        !view_source.contains("\"Edit draft\""),
        "Saved rows should select into details directly instead of exposing Edit draft"
    );
    assert!(
        !view_source.contains("settings-agent-field-edit"),
        "Editable fields should enter edit mode from the field surface, not a separate Edit button"
    );
}
