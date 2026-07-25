use super::{canonical_navigation_item_height, canonical_sidebar_group_spacing};

/// Canonical Sidebar 组节奏：三组导航不得逐组累积偏离 golden。
///
/// 数据构造（含关键数值的推导过程）：
///   top margin = 23px
///   group gap = 23px
///   groups = 3；组间额外位移 = 2 × 23px = 46px
///   相对旧值的修正 = 23 - 11 = 12px/组，与真实 App 叠图的 12/24/36px 偏差一致。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取常规 Sidebar 的顶部间距与组间距。
///   2. 与 canonical 23px 逻辑节奏逐项比较。
///
/// 预期结果：
///   - 正断言：顶部和组间距都精确为 23px。
///   - 负断言：不得恢复压缩后的 11px 间距并让 Orchestration 累积上移 36px。
#[test]
fn sidebar_groups_follow_canonical_twenty_three_pixel_rhythm() {
    let (top_margin, group_gap) = canonical_sidebar_group_spacing();

    assert_eq!(
        top_margin, 23.0,
        "Settings 首个导航组应在标题后保留 23px canonical 逻辑间距"
    );
    assert_eq!(
        group_gap, 23.0,
        "Settings 三个导航组之间应使用 23px canonical 逻辑间距"
    );
    assert_ne!(
        (top_margin, group_gap),
        (11.0, 11.0),
        "压缩后的 11px 节奏会让 Orchestration 相对 golden 累积上移 36px"
    );
}

/// Canonical 导航行高：六个可见入口必须保持 38px 行高，避免累计纵向漂移。
///
/// 数据构造（含关键数值的推导过程）：
///   visible rows = 6
///   canonical row height = 38px
///   rows before Orchestration = 5
///   旧高度累计偏移 = 5 × (40 - 38) = 10px
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Sidebar 导航行高。
///   2. 与 38px golden 行高和旧 40px 高度比较。
///
/// 预期结果：
///   - 正断言：每个入口精确为 38px。
///   - 负断言：不得恢复 40px 并让第六个入口累计下移 10px。
#[test]
fn sidebar_navigation_items_use_canonical_thirty_eight_pixel_height() {
    let height = canonical_navigation_item_height();

    assert_eq!(height, 38.0, "Settings Sidebar 每个导航入口应为 38px");
    assert_ne!(
        height, 40.0,
        "旧 40px 行高会让 Orchestration 相对 golden 累计下移 10px"
    );
}
