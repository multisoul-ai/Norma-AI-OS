use super::{Theme, ThemeId};

/// Product UI V2 浅色主题：Paper Blue 必须保持中性纸白、Cobalt 焦点与深色 Terminal。
///
/// 数据构造（含关键数值的推导过程）：
///   shell alpha       = 0x66 / 0xff = 40%（玻璃壳，不承载长文本）
///   canvas overlay A  = 0xeb / 0xff ≈ 92%（压低壁纸彩度）
///   terminal alpha    = 0xeb / 0xff ≈ 92%（独立深色内容井）
///   terminal-shell 差 = 0xeb - 0x66 = 0x85 = 133 alpha levels
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 PaperBlue id 构造 Paper Blue。
///   2. 检查环境 overlay、玻璃 shell、文字、Cobalt 与 Mint。
///   3. 比较 Terminal 与 shell 的不透明度，确认两层没有合并。
///
/// 预期结果：
///   - 正断言：Paper Blue 色值与 Core UI V2 设计基线精确匹配。
///   - 正断言：Terminal 明显比玻璃 shell 更不透明。
///   - 负断言：Paper Blue 不应回退到旧 `#111214` 文字或绿色焦点。
#[test]
fn test_paper_blue_material_roles_match_core_ui_v2() {
    let theme = Theme::for_id(ThemeId::PaperBlue);

    assert_eq!(
        theme.canvas_from, 0xfffefbff,
        "Paper Blue Base 应为 #FFFEFB"
    );
    assert_eq!(
        theme.canvas_to, 0xf5f4f1ff,
        "Paper Blue 第二 Base 应为 #F5F4F1"
    );
    assert_eq!(
        theme.canvas_overlay_to, 0xd4eaf7d6,
        "Paper Blue Environment 应为 #D4EAF7"
    );
    assert_eq!(
        theme.text_primary, 0x313d44ff,
        "Paper Blue Text 应为 #313D44"
    );
    assert_eq!(
        theme.accent, 0x2f80edff,
        "Paper Blue Accent 应为 Cobalt #2F80ED"
    );
    assert_eq!(
        theme.success, 0x64e0b5ff,
        "Paper Blue Signal 应为 Mint #64E0B5"
    );
    assert_eq!(
        alpha(theme.node_fill),
        0x66,
        "Paper Blue shell 应保持 40% 玻璃透明度"
    );
    assert_eq!(
        alpha(theme.term_bg),
        0xeb,
        "Paper Blue Terminal 应保持约 92% 不透明"
    );
    assert!(
        alpha(theme.term_bg) > alpha(theme.node_fill) + 100,
        "Terminal 内容井必须显著比 shell 更不透明"
    );
    assert_ne!(
        theme.text_primary, 0x111214ff,
        "Paper Blue 文字不应回退到旧 Graphite 字面量"
    );
    assert_ne!(
        theme.focused_stroke, 0x35d07fff,
        "Paper Blue 焦点不应回退到旧绿色"
    );
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
