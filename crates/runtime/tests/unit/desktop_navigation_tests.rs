use super::adjacent_target;

/// 目录网格支持响应式列数、空列表、边界和末行缺格，不产生不存在的索引。
/// 数据：8项3列的末行只有2项；另测1列、2列与0项。
/// 执行：从第2行末格向下；从上下边界、左右端点及失效选择移动。
/// 预期：上下保持列/边界，缺格落到7；左右循环；空列表无目标。
#[test]
fn directory_grid_preserves_columns_and_handles_edges() {
    use super::directory_grid_target as target;
    use canvas::NavigationDirection::*;
    assert_eq!(
        target(8, 3, Some(5), Down),
        Some(7),
        "末行缺格落到最后存在的卡片"
    );
    assert_eq!(target(8, 3, Some(7), Up), Some(4), "向上保留当前列");
    assert_eq!(target(8, 3, Some(7), Down), Some(7), "下边界保持");
    assert_eq!(target(8, 3, Some(1), Up), Some(1), "上边界保持");
    assert_eq!(target(8, 3, Some(0), Left), Some(7), "左端循环");
    assert_eq!(target(8, 3, Some(7), Right), Some(0), "右端循环");
    assert_eq!(target(8, 2, Some(1), Down), Some(3), "两列时不能继续跳三格");
    assert_eq!(target(8, 1, Some(1), Down), Some(2), "窄窗单列逐项移动");
    assert_eq!(target(8, 3, Some(99), Down), Some(0), "失效选择回首项");
    assert_eq!(target(1, 3, None, Up), Some(0), "单项首选可达");
    assert_eq!(target(0, 0, None, Right), None, "空列表不能制造选择");
}

/// 循环导航按创建时间及 ID 稳定排序，连续切换不受最近访问重排影响。
///
/// 数据构造：a 在 10ms 创建；b/c 同在 20ms 创建，以 ID 打破平局；
///   三项 = a + b + c，输入顺序为最近访问 c/a/b，随后改为 b/c/a。
/// 执行过程：从 a 向前到 b，再用 b 作为尚未完成的目标向前到 c；验证双向环绕与空集合。
/// 预期结果：按 a/b/c 循环，不会因最近访问重排退回 a；单项稳定，空集合没有目标。
#[test]
fn cycles_by_creation_order_and_latest_target() {
    let recent = [(20, "c"), (10, "a"), (20, "b")];
    let pending = adjacent_target(recent, Some("a"), false);
    assert_eq!(pending, Some("b"), "a 的下一项应是 b");
    assert_ne!(pending, Some("c"), "最近访问顺序不能替代创建顺序");
    assert_eq!(
        adjacent_target([(20, "b"), (20, "c"), (10, "a")], pending, false),
        Some("c"),
        "连续操作必须基于最新目标 b 前往 c"
    );
    assert_eq!(
        adjacent_target(recent, Some("c"), false),
        Some("a"),
        "末尾向前环绕到 a"
    );
    assert_eq!(
        adjacent_target(recent, Some("a"), true),
        Some("c"),
        "首项向后环绕到 c"
    );
    assert_eq!(
        adjacent_target(recent, Some("removed"), false),
        Some("a"),
        "失效目标向前从首项开始"
    );
    assert_eq!(
        adjacent_target(recent, None, true),
        Some("c"),
        "没有目标时向后从末项开始"
    );
    assert_eq!(
        adjacent_target([(10, "a")], Some("a"), false),
        Some("a"),
        "单项应保持原目标"
    );
    assert_eq!(
        adjacent_target([], Some("a"), false),
        None,
        "空列表不能产生虚构目标"
    );
}
