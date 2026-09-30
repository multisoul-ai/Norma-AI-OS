use super::*;

/// M-10: 未读完成提示用呼吸描边和外缘柔光，不改变卡片填充与阴影。
///
/// 数据构造（含关键数值的推导过程）：
///   Cool 周期 = 2500ms；淡入 = 150ms；推进 200ms > 150ms 使描边达到可见状态。
///   完成亮起 = 520ms；首段 200ms < 520ms 有增强，续推 600ms 后累计 800ms > 520ms 应结束。
///   查看后推进 200ms > 150ms 淡出，外缘柔光随之消失。
///   测试矩形 100×80 仅用于建立节点，视觉强度与面积无关。
///
/// 执行过程：
///   1. 创建 Idle 节点并解析基准视觉。
///   2. 设置 Cool 完成提示并推进 200ms，读取一次性亮起与持续柔光。
///   3. 再推进 600ms，确认增强结束、持续柔光保留；标记关闭后推进 200ms。
///
/// 预期结果：
///   - 正断言：呼吸描边加粗变色、完成瞬间增强，随后仍有外缘柔光。
///   - 负断言：空闲与已查看时无柔光；填充与阴影颜色不变。
#[test]
fn test_completion_breathing_only_emphasizes_border() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 53;
    let node_rect = CanvasRect::new(0.0, 0.0, 100.0, 80.0);
    store.ensure_node(node_id, node_rect);
    let idle = store.resolve_node_visual(node_id, node_rect, theme);

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    store.tick(Duration::from_millis(200));
    let breathing = store.resolve_node_visual(node_id, node_rect, theme);

    assert!(
        breathing.border_is_emphasized,
        "未读完成提示应绘制较醒目的呼吸描边"
    );
    assert_ne!(
        breathing.stroke_color, idle.stroke_color,
        "呼吸描边应随状态改变颜色"
    );
    assert_eq!(
        breathing.fill_color, idle.fill_color,
        "卡片填充不能被呼吸特效染色"
    );
    assert_eq!(
        breathing.shadow_color, idle.shadow_color,
        "整卡阴影不能被霓虹色染色"
    );
    assert!(idle.border_glow.is_none(), "空闲卡片不应显示外缘柔光");
    let initial_glow = breathing.border_glow.expect("未读完成提示应有外缘柔光");
    assert!(initial_glow.intensity > 0.0, "呼吸柔光应有可见强度");
    assert!(initial_glow.flash > 0.0, "刚完成时应出现一次增强亮起");
    assert!(
        !idle.border_is_emphasized,
        "空闲卡片不应使用完成提示的加粗描边"
    );

    store.tick(Duration::from_millis(600));
    let sustained = store.resolve_node_visual(node_id, node_rect, theme);
    let sustained_glow = sustained.border_glow.expect("未读完成提示应持续呼吸");
    assert_eq!(sustained_glow.flash, 0.0, "增强亮起应在 520ms 后结束");
    assert!(sustained_glow.intensity > 0.0, "亮起结束后外缘柔光仍应可见");

    store.set_breathing_intent(node_id, BreathingIntent::Inactive);
    store.tick(Duration::from_millis(200));
    let viewed = store.resolve_node_visual(node_id, node_rect, theme);
    assert!(viewed.border_glow.is_none(), "查看后外缘柔光应完全消失");
}
