use super::{canonical_expanded_content_height, warning_status_color};
use ui::Theme;

/// Canonical 展开态高度：五个 Provider 与 Cursor 四组件必须同时留在 golden 面板内。
///
/// 数据构造（含关键数值的推导过程）：
///   header = 118px
///   notice = 48px
///   provider rows = 5 × 85px = 425px
///   Cursor components = 4 × 44px = 176px
///   detail bottom margin = 14px
///   total = 118 + 48 + 425 + 176 + 14 = 781px
///   logical panel height = 820px；remaining = 820 - 781 = 39px
///   0.854 display scale 下 outer = 820 × 0.854 ≈ 700px，与 699px golden 光栅外框一致。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 计算标题、提示、五个 Provider 行和四个详情行的固定高度。
///   2. 与 781px 精确逻辑预算及 820px Settings 面板上限比较。
///
/// 预期结果：
///   - 正断言：固定内容预算精确为 781px，光栅化后与 golden 行距一致。
///   - 负断言：不得超过 820px 并把 Grok 推出面板。
#[test]
fn canonical_expanded_layout_keeps_grok_inside_golden_panel() {
    let height = canonical_expanded_content_height();

    assert_eq!(
        height, 781.0,
        "Orchestration 展开态固定内容应为 781px，保留 39px 逻辑余量"
    );
    assert!(
        height <= 820.0,
        "Orchestration 固定内容不得超过 820px Settings 逻辑面板高度"
    );
    assert_ne!(
        height, 800.0,
        "固定内容不得退化为 800px 临界预算并挤压底部安全余量"
    );
}

/// 修复状态语义色：Needs repair 必须保持 warning orange，不能跟随主题主强调色变蓝。
///
/// 数据构造（含关键数值的推导过程）：
///   canonical warning = glass-dark accent_orange = 0xff7932ff
///   SapphireNight primary accent = 0x54a2ffff
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 获取 Orchestration 的 warning status token。
///   2. 与 canonical orange 和 SapphireNight blue 分别比较。
///
/// 预期结果：
///   - 正断言：Needs repair 使用现有 glass-dark orange token。
///   - 负断言：warning 不得退化为 SapphireNight 的蓝色主强调色。
#[test]
fn needs_repair_uses_semantic_orange_instead_of_primary_accent() {
    let warning = warning_status_color();

    assert_eq!(
        warning,
        Theme::glass_dark().accent_orange,
        "Needs repair 必须复用 canonical Settings warning orange token"
    );
    assert_ne!(
        warning,
        Theme::sapphire_night().accent_blue,
        "Needs repair 不得与 Repair 按钮共用蓝色主强调色"
    );
}

/// Settings 标题栏固定高度：118px header 不得在纵向 flex 中被压回旧尺寸
///
/// 数据构造（含关键数值的推导过程）：
///   canonical header = 118px。
///   old header = 104px；修正量 = 118 - 104 = 14px。
///   0.854 display scale 下 header = 118 × 0.854 ≈ 101px，与 golden 分隔线一致。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Settings content 的生产渲染源码。
///   2. 定位 `min_h(118)` 标题栏约束。
///   3. 验证该约束紧接 `flex_none()`，禁止父级在内容密集时收缩标题栏。
///
/// 预期结果：
///   - 正断言：118px 与不可收缩约束同时存在。
///   - 负断言：不得保留可收缩的旧 104px 标题栏组合。
#[test]
fn settings_content_header_preserves_canonical_height_under_flex_pressure() {
    let source = include_str!("../../src/settings/content.rs");

    assert!(
        source.contains(".min_h(px(118.0))\n        .flex_none()"),
        "Settings 118px header must be flex-none so dense Orchestration content cannot shrink it"
    );
    assert!(
        !source.contains(".min_h(px(104.0))\n        .flex()"),
        "Settings header must not retain the shrinkable 104px layout"
    );
}
