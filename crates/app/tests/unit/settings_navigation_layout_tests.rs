use super::{canonical_navigation_item_height, canonical_sidebar_group_spacing};
use crate::settings::panel::SettingsSection;

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

/// Norma 是独立设置入口，Voice 内容收敛在其语音分区。
///
/// 数据构造：使用真实 SettingsSection 元数据，无模拟页面。
/// 执行过程：取得 Norma 标题与所有侧栏分组。
/// 预期结果：标题为 Norma；AI 分组包含 Norma，输入分组不含 Norma。
#[test]
fn norma_is_an_ai_settings_section() {
    assert_eq!(
        SettingsSection::Norma.title(),
        "Norma",
        "应显示独立 Norma 入口"
    );
    assert!(
        super::AUTOMATION_SECTIONS.contains(&SettingsSection::Norma),
        "Norma 应在 AI 分组"
    );
    assert!(
        !super::INPUT_SECTIONS.contains(&SettingsSection::Norma),
        "不能仍作为独立 Voice 输入设置"
    );
}
