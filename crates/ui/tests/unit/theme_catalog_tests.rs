use super::{Theme, ThemeId};

/// Core UI V2 主题目录：六个稳定 id 必须按设计基线完整暴露，并精确映射关键色值。
///
/// 数据构造（含关键数值的推导过程）：
///   light themes = 3（Paper Blue / Linen Sage / Silver Violet）
///   dark themes  = 3（Sapphire Night / Midnight Violet / Forest Mint）
///   total        = 3 + 3 = 6
///   每套主题的 Base / Text / Accent / Signal 均直接取自
///   `docs/design-docs/core-ui-v2/README.md`，不由实现代码重新计算。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 `ThemeId::ALL`，逐项检查稳定 id 与显示名称。
///   2. 对每个 id 调用 `Theme::for_id`，检查 Base、Text、Accent 与 Signal。
///   3. 检查三浅三深分组，并确认默认主题固定为 Paper Blue。
///
/// 预期结果：
///   - 正断言：六个主题按设计文档顺序完整存在，关键字面量逐一匹配。
///   - 正断言：Paper Blue 是默认主题，前三项为浅色、后三项为深色。
///   - 负断言：不得继续暴露已淘汰的 `ceramic-mist` 等旧主题 id。
#[test]
fn test_core_ui_v2_catalog_matches_reviewed_theme_literals() {
    assert_eq!(
        ThemeId::ALL,
        [
            ThemeId::PaperBlue,
            ThemeId::LinenSage,
            ThemeId::SilverViolet,
            ThemeId::SapphireNight,
            ThemeId::MidnightViolet,
            ThemeId::ForestMint,
        ],
        "Core UI V2 主题目录应按三浅三深顺序完整暴露六套主题"
    );
    assert_eq!(
        ThemeId::default(),
        ThemeId::PaperBlue,
        "默认主题必须是设计文档指定的 Paper Blue"
    );

    assert_theme(ThemeExpectation {
        id: ThemeId::PaperBlue,
        stable_id: "paper-blue",
        display_name: "Paper Blue",
        dark: false,
        base: 0xfffefbff,
        raised: 0xf5f4f1ff,
        text: 0x313d44ff,
        accent: 0x2f80edff,
        signal: 0x64e0b5ff,
    });
    assert_theme(ThemeExpectation {
        id: ThemeId::LinenSage,
        stable_id: "linen-sage",
        display_name: "Linen Sage",
        dark: false,
        base: 0xf9f8f3ff,
        raised: 0xebede6ff,
        text: 0x1d231fff,
        accent: 0x3d9978ff,
        signal: 0x64e0b5ff,
    });
    assert_theme(ThemeExpectation {
        id: ThemeId::SilverViolet,
        stable_id: "silver-violet",
        display_name: "Silver Violet",
        dark: false,
        base: 0xfafaf9ff,
        raised: 0xf0f0f2ff,
        text: 0x232328ff,
        accent: 0x6a5cffff,
        signal: 0x64e0b5ff,
    });
    assert_theme(ThemeExpectation {
        id: ThemeId::SapphireNight,
        stable_id: "sapphire-night",
        display_name: "Sapphire Night",
        dark: true,
        base: 0x071324ff,
        raised: 0x0e2338ff,
        text: 0xeff7ffff,
        accent: 0x54a2ffff,
        signal: 0x64e0b5ff,
    });
    assert_theme(ThemeExpectation {
        id: ThemeId::MidnightViolet,
        stable_id: "midnight-violet",
        display_name: "Midnight Violet",
        dark: true,
        base: 0x0c0c1bff,
        raised: 0x231f3cff,
        text: 0xf8f5ffff,
        accent: 0x8b7cffff,
        signal: 0x64e0b5ff,
    });
    assert_theme(ThemeExpectation {
        id: ThemeId::ForestMint,
        stable_id: "forest-mint",
        display_name: "Forest Mint",
        dark: true,
        base: 0x091716ff,
        raised: 0x112d27ff,
        text: 0xf1fcf8ff,
        accent: 0x64e0b5ff,
        signal: 0xd6ff3bff,
    });

    assert!(
        ThemeId::ALL.iter().all(|id| id.as_str() != "ceramic-mist"),
        "已退出基线的 ceramic-mist 不应继续出现在主题目录"
    );
    assert!(
        ThemeId::ALL.iter().all(|id| id.as_str() != "pearl-lilac"),
        "已退出基线的 pearl-lilac 不应继续出现在主题目录"
    );
    assert!(
        ThemeId::ALL.iter().all(|id| id.as_str() != "sand-coral"),
        "已退出基线的 sand-coral 不应继续出现在主题目录"
    );
}

struct ThemeExpectation {
    id: ThemeId,
    stable_id: &'static str,
    display_name: &'static str,
    dark: bool,
    base: u32,
    raised: u32,
    text: u32,
    accent: u32,
    signal: u32,
}

fn assert_theme(expected: ThemeExpectation) {
    let ThemeExpectation {
        id,
        stable_id,
        display_name,
        dark,
        base,
        raised,
        text,
        accent,
        signal,
    } = expected;
    let theme = Theme::for_id(id);
    assert_eq!(
        id.as_str(),
        stable_id,
        "{display_name} 的 stable id 应匹配设计基线"
    );
    assert_eq!(
        id.display_name(),
        display_name,
        "{stable_id} 的显示名称应匹配设计基线"
    );
    assert_eq!(
        id.is_dark(),
        dark,
        "{display_name} 的 light/dark 分组应匹配设计基线"
    );
    assert_eq!(
        theme.canvas_from, base,
        "{display_name} 的 Base 起始色应精确匹配"
    );
    assert_eq!(
        theme.canvas_to, raised,
        "{display_name} 的 Base/Raised 结束色应精确匹配"
    );
    assert_eq!(
        theme.text_primary, text,
        "{display_name} 的主文字色应精确匹配"
    );
    assert_eq!(theme.accent, accent, "{display_name} 的 Accent 应精确匹配");
    assert_eq!(theme.success, signal, "{display_name} 的 Signal 应精确匹配");
    assert!(
        (theme.term_bg & 0xff) >= 0xcc,
        "{display_name} 的 Terminal 必须保持至少 80% 不透明的深色内容井"
    );
    assert_ne!(
        theme.term_bg, theme.node_fill,
        "{display_name} 的 Terminal 内容井不得退化为普通玻璃 shell"
    );
}
