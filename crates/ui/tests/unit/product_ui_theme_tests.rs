use super::{Theme, ThemeId};

/// 场景：浅色改版采用冷白与钴蓝，标题和终端均有稳定浅色阅读面。
/// 数据构造：Paper Blue 由设计值构造，卡片 alpha=240/255、终端 alpha=248/255。
/// 执行过程：读取实际 token，核对材质分层与强调色。
/// 预期结果：纸白蓝保持蓝色身份，终端比外壳更稳定，壁纸提亮色可用。
#[test]
fn test_paper_blue_material_roles_match_core_ui_v2() {
    let theme = Theme::for_id(ThemeId::PaperBlue);
    assert_eq!(theme.canvas_from, 0xf6f8fcff, "纸白蓝使用冷纸白底色");
    assert_eq!(theme.canvas_to, 0xeaf0f8ff, "第二底色为淡蓝灰");
    assert_eq!(
        theme.canvas_overlay_to, 0xeff6ffbd,
        "壁纸底部保留浅蓝透光层"
    );
    assert_eq!(theme.text_primary, 0x202938ff, "正文使用深蓝灰");
    assert_eq!(theme.accent, 0x2563ebff, "强调色保持钴蓝身份");
    assert_eq!(theme.success, 0x16784bff, "小字号状态绿保持对比");
    assert_eq!(alpha(theme.node_fill), 240, "外壳提供稳定浅色背景");
    assert_eq!(alpha(theme.term_bg), 248, "终端阅读面约 97% 不透明");
    assert!(
        alpha(theme.term_bg) > alpha(theme.node_fill),
        "终端比外壳更稳定"
    );
    assert_ne!(theme.term_bg >> 8, 0x0e151d, "浅色终端不得保留旧深色底");
}

/// Dark 入口分层：用户主题走 Sapphire Night，历史 `glass_dark/dark` 保持动画兼容。
///
/// 数据构造（含关键数值的推导过程）：
///   Sapphire Base      = #071324（用户可选主题）
///   Legacy Dark Base   = #101722（旧运动测试与内部调用）
///   Legacy Accent      = #FF7932
///   Legacy Terminal α  = 0xfa / 0xff ≈ 98%
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过 `ThemeId::SapphireNight` 构造用户可选主题。
///   2. 分别调用 `glass_dark` 与 `dark`，比较历史 Base、Accent 和 Terminal。
///
/// 预期结果：
///   - 正断言：Sapphire id 仍精确映射新的 Sapphire Night。
///   - 正断言：两个历史入口彼此一致并保留旧色值。
///   - 负断言：历史 Dark 不得冒充 Sapphire Night，避免改变无关动画契约。
#[test]
fn test_theme_id_sapphire_and_legacy_dark_keep_separate_contracts() {
    let sapphire = Theme::for_id(ThemeId::SapphireNight);
    let glass_dark = Theme::glass_dark();
    let dark = Theme::dark();

    assert_eq!(
        sapphire.canvas_from, 0x071324ff,
        "SapphireNight id 应映射 Core UI V2 Sapphire Base"
    );
    assert_eq!(
        glass_dark.canvas_from, 0x101722f2,
        "glass_dark 应保留历史暗色 Base"
    );
    assert_eq!(
        dark.accent_orange, 0xff7932ff,
        "dark 应保留历史动画使用的 Orange Accent"
    );
    assert_eq!(
        dark.term_bg, glass_dark.term_bg,
        "dark 与 glass_dark 的历史 Terminal 契约应一致"
    );
    assert_ne!(
        dark.canvas_from, sapphire.canvas_from,
        "历史 Dark 不应冒充用户可选的 Sapphire Night"
    );
}

fn alpha(rgba: u32) -> u8 {
    rgba.to_be_bytes()[3]
}
