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

/// 通用合并外观后保持四组十二页，基础设置、主题、壁纸和特效均为通用的直接目的地。
///
/// 数据构造：通用 4 页 + Norma 4 页（含 Norma 大脑）+ 输入 1 页 + 自动化 3 页 = 12 页，4 个分组。
/// 执行过程：逐组核对生产导航的顺序和页面归属，再检查扁平化后的唯一性与隐藏项。
/// 预期结果：完整结构与用户确认的两级菜单一致；没有独立外观组、重复页面或测试占位页。
#[test]
fn settings_navigation_has_four_groups_and_twelve_unique_pages() {
    use super::SettingsSection::*;
    let expected: &[(&str, &[super::SettingsSection])] = &[
        ("general.title", &[General, Themes, Wallpaper, Effects]),
        (
            "Norma",
            &[NormaPersona, NormaMemory, NormaVoice, NormaBrain],
        ),
        ("settings.input_controls", &[Keyboard]),
        ("settings.ai_automation", &[Agents, Orchestration, Mobile]),
    ];
    assert_eq!(
        super::NAVIGATION_GROUPS.len(),
        4,
        "合并后必须恰有四个导航分组"
    );
    for ((title, sections), (expected_title, expected_sections)) in
        super::NAVIGATION_GROUPS.iter().zip(expected)
    {
        assert_eq!(title, expected_title, "分组名称和顺序必须符合两级菜单");
        assert_eq!(
            sections, expected_sections,
            "分组 {title} 的页面归属和顺序必须正确"
        );
    }
    let pages: Vec<_> = super::NAVIGATION_GROUPS
        .iter()
        .flat_map(|(_, sections)| sections.iter().copied())
        .collect();
    assert_eq!(pages.len(), 12, "页面总数应为 4 + 4 + 1 + 3 = 12");
    for (index, page) in pages.iter().enumerate() {
        assert!(!pages[..index].contains(page), "页面 {page:?} 只能出现一次");
    }
    assert!(
        !pages.contains(&Runtime),
        "测试专用 Runtime 不得进入生产导航"
    );
    assert!(!pages.contains(&Skills), "测试专用 Skills 不得进入生产导航");
    assert!(
        super::NAVIGATION_GROUPS
            .iter()
            .all(|(title, _)| !title.is_empty() && *title != "settings.appearance"),
        "通用应显示分组标题，独立外观分组应被合并"
    );
}

/// 导航项的元素 id 来自 `metadata.index`，重复会让两个按钮抢同一个点击目标。
///
/// 数据构造：取生产导航里全部可见页面。
/// 执行过程：收集每页的 `index` 与 `debug_selector`。
/// 预期结果：两者在可见页面间都不重复（曾因 NormaBrain 与 Effects 同为 13 导致点击失效）。
#[test]
fn settings_navigation_item_ids_are_unique() {
    let metadata: Vec<_> = super::NAVIGATION_GROUPS
        .iter()
        .flat_map(|(_, sections)| sections.iter().map(|section| section.metadata()))
        .collect();
    for (i, item) in metadata.iter().enumerate() {
        for other in &metadata[..i] {
            assert_ne!(item.index, other.index, "导航 index 重复: {}", item.index);
            assert_ne!(item.debug_selector, other.debug_selector);
        }
    }
}
