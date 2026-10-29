use super::{FeatureGuideState, FeatureGuideTopic};

/// 场景：终版九段演示均能导航并恢复，旧通知页进度迁移到幕僚页。
/// 数据：安装页 1 + 终版视频 9 + 速查页 1 = 11；每页有独立持久化键。
/// 执行：逐页前进、保存并重载，再从末页回退；单独加载旧通知页键。
/// 预期：顺序与恢复位置一致，边界不越界，旧键不会退回首屏。
#[test]
fn final_demo_topics_preserve_navigation_and_saved_progress() {
    let keys = [
        "agent_orchestration",
        "voice_group_control",
        "agent_collaboration",
        "live_node_shortcuts",
        "workspace_layouts",
        "chief_of_staff",
        "right_drawer",
        "switch_workspaces",
        "workspace_home",
        "slash_commands",
        "shortcut_cheat_sheet",
    ];
    assert_eq!(
        FeatureGuideTopic::ALL.len(),
        keys.len(),
        "九段终版演示和两页引导必须全部可达"
    );
    let mut guide = FeatureGuideState::load(None, None, false);
    for (index, key) in keys.iter().enumerate() {
        assert_eq!(
            guide.serialized_topic(),
            *key,
            "第 {} 页必须按终版顺序导航",
            index + 1
        );
        let restored = FeatureGuideState::load(Some("in_progress"), Some(key), true);
        assert_eq!(restored.topic, guide.topic, "每页均须从自己的持久化键恢复");
        assert_eq!(
            restored.topic_position(),
            index + 1,
            "恢复不能退回首屏或错页"
        );
        guide.next_topic();
    }
    assert!(guide.topic.is_last(), "下一步必须停留在最终速查页");
    for key in keys.iter().rev() {
        assert_eq!(guide.serialized_topic(), *key, "上一页必须逆序遍历所有演示");
        guide.previous_topic();
    }
    assert_eq!(guide.topic_position(), 1, "首屏回退不得越界");
    let migrated = FeatureGuideState::load(Some("in_progress"), Some("agent_notifications"), true);
    assert_eq!(
        migrated.serialized_topic(),
        "chief_of_staff",
        "旧状态通知页进度应迁移到幕僚演示"
    );
    assert_ne!(migrated.topic_position(), 1, "旧进度不得静默丢失并退回首屏");
}
