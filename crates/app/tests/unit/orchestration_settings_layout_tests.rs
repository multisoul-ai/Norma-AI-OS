use super::{canonical_expanded_content_height, warning_status_color};
use ui::Theme;

/// Canonical 展开态高度：五个 Provider 与 Cursor 四组件必须同时留在 golden 面板内。
///
/// 数据构造（含关键数值的推导过程）：
///   header = 104px
///   notice = 48px
///   provider rows = 5 × 72px = 360px
///   Cursor components = 4 × 39px = 156px
///   detail bottom margin = 14px
///   total = 104 + 48 + 360 + 156 + 14 = 682px
///   golden panel height ≈ 698px；remaining = 698 - 682 = 16px
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 计算标题、提示、五个 Provider 行和四个详情行的固定高度。
///   2. 与 682px 精确预算及 698px golden 外框上限比较。
///
/// 预期结果：
///   - 正断言：固定内容预算精确为 682px，Grok 行可见。
///   - 负断言：不得回退到旧的 800px 密度并把 Grok 推出面板。
#[test]
fn canonical_expanded_layout_keeps_grok_inside_golden_panel() {
    let height = canonical_expanded_content_height();

    assert_eq!(
        height, 682.0,
        "Orchestration 展开态固定内容应为 682px，保留 16px 可见余量"
    );
    assert!(
        height <= 698.0,
        "Orchestration 固定内容不得超过 698px golden 面板高度"
    );
    assert_ne!(
        height, 800.0,
        "旧 58/88/46px 密度会把 Grok 推出视区，不得恢复"
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
