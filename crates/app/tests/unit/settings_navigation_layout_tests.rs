use super::{canonical_navigation_item_height, canonical_sidebar_group_spacing};

/// Canonical Sidebar 组节奏：三组导航不得逐组累积偏离 golden。
///
/// 数据构造（含关键数值的推导过程）：
///   top margin = 23.5px
///   group gap = 23.5px
///   groups = 3；组间额外位移 = 2 × 23.5px = 47px
///   相对旧值的修正 = 23.5 - 23 = 0.5px/组，抵消第三组剩余 1px 累计偏差。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取常规 Sidebar 的顶部间距与组间距。
///   2. 与 canonical 23.5px 逻辑节奏逐项比较。
///
/// 预期结果：
///   - 正断言：顶部和组间距都精确为 23.5px。
///   - 负断言：不得恢复 23px 并保留最后 1px 累计偏差。
#[test]
fn sidebar_groups_follow_canonical_twenty_three_point_five_pixel_rhythm() {
    let (top_margin, group_gap) = canonical_sidebar_group_spacing();

    assert_eq!(
        top_margin, 23.5,
        "Settings 首个导航组应在标题后保留 23.5px canonical 逻辑间距"
    );
    assert_eq!(
        group_gap, 23.5,
        "Settings 三个导航组之间应使用 23.5px canonical 逻辑间距"
    );
    assert_ne!(
        (top_margin, group_gap),
        (23.0, 23.0),
        "23px 节奏会让 Orchestration 相对 golden 保留约 1px 累积上移"
    );
}

/// Canonical 导航行高：七个可见入口必须保持 39px 行高，避免累计纵向漂移。
///
/// 数据构造（含关键数值的推导过程）：
///   visible rows = 7
///   canonical row height = 39px
///   rows before Orchestration = 6
///   旧高度累计偏移 = 6 × (39 - 38) = 6px
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Sidebar 导航行高。
///   2. 与 39px golden 行高和旧 38px 高度比较。
///
/// 预期结果：
///   - 正断言：每个入口精确为 39px。
///   - 负断言：不得恢复 38px 并让第六个入口累计上移 5px。
#[test]
fn sidebar_navigation_items_use_canonical_thirty_nine_pixel_height() {
    let height = canonical_navigation_item_height();

    assert_eq!(height, 39.0, "Settings Sidebar 每个导航入口应为 39px");
    assert_ne!(
        height, 40.0,
        "旧 40px 行高会让 Orchestration 相对 golden 累计下移 5px"
    );
    assert_ne!(
        height, 38.0,
        "旧 38px 行高会让 Orchestration 相对 golden 累计上移 5px"
    );
}

/// Norma 位于设置最前面，记忆、语音、人设是可直接导航的三个页面。
///
/// 数据构造：读取生产导航分组；Norma 子页面数 = 记忆(1)+语音(1)+人设(1)=3。
/// 执行过程：取第一个分组及页面标题，再检查其他导航组。
/// 预期结果：首组为 Norma 且顺序明确；AI 与输入分组没有重复的 Norma 入口。
#[test]
fn norma_is_the_first_group_with_three_direct_destinations() {
    let (title, sections) = super::NAVIGATION_GROUPS[0];
    assert_eq!(title, "Norma", "首次查看设置时必须首先看到独立 Norma 板块");
    assert_eq!(
        sections
            .iter()
            .map(|section| section.title())
            .collect::<Vec<_>>(),
        ["Memory", "Voice", "Persona"],
        "三个页面必须分别直达，不能继续在同一 Norma 页面内切标签"
    );
    assert!(
        !super::AUTOMATION_SECTIONS
            .iter()
            .any(|section| sections.contains(section)),
        "Norma 不应继续混在 AI & Automation 分组中"
    );
    assert!(
        !super::INPUT_SECTIONS
            .iter()
            .any(|section| sections.contains(section)),
        "语音入口必须仅属于 Norma，不能重复出现在输入分组"
    );
}
