use super::{Theme, ThemeId};

/// 场景：六个可选主题全部为深色，首次使用墨黑。
/// 数据构造：三套中性色与三套已有深色；字面色值来自深色系列设计记录。
/// 执行过程：验证目录、默认入口、显示名与关键材质，不依赖系统外观。
/// 预期结果：默认墨黑、六套配色各自可辨，旧浅色不在可选目录。
#[test]
fn test_core_ui_v2_catalog_matches_reviewed_theme_literals() {
    assert_eq!(
        ThemeId::ALL,
        [
            ThemeId::InkBlack,
            ThemeId::Graphite,
            ThemeId::WarmObsidian,
            ThemeId::SapphireNight,
            ThemeId::Moonstone,
            ThemeId::DeepPetrol,
        ],
        "主题目录应按三中性加三彩色深色顺序暴露"
    );
    assert_eq!(ThemeId::default(), ThemeId::InkBlack, "默认墨黑");
    let default_theme = Theme::default();
    assert_eq!(default_theme.canvas_from, 0x0c0c0eff, "墨黑画布应近中性黑");
    assert_eq!(default_theme.text_primary, 0xe9e9ecff, "墨黑使用柔白文字");
    assert_ne!(default_theme.canvas_from, 0xf6f8fcff, "默认不能是旧浅色");
    assert_theme(ThemeExpectation {
        id: ThemeId::InkBlack,
        stable_id: "ink-black",
        display_name: "Ink Black",
        dark: true,
        base: 0x0c0c0eff,
        raised: 0x1a1a1dff,
        text: 0xe9e9ecff,
        accent: 0xc4ccd9ff,
        signal: 0x8bc6a3ff,
    });
    assert_theme(ThemeExpectation {
        id: ThemeId::Graphite,
        stable_id: "graphite",
        display_name: "Graphite",
        dark: true,
        base: 0x17191cff,
        raised: 0x282c31ff,
        text: 0xe5e8edff,
        accent: 0x9ab8d3ff,
        signal: 0x8bc6a3ff,
    });
    assert_theme(ThemeExpectation {
        id: ThemeId::WarmObsidian,
        stable_id: "warm-obsidian",
        display_name: "Warm Obsidian",
        dark: true,
        base: 0x151210ff,
        raised: 0x27221dff,
        text: 0xeee6daff,
        accent: 0xddb780ff,
        signal: 0x8bc6a3ff,
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
        id: ThemeId::Moonstone,
        stable_id: "moonstone",
        display_name: "Moonstone",
        dark: true,
        base: 0x111117ff,
        raised: 0x292830ff,
        text: 0xe9e7efff,
        accent: 0xc1b9d5ff,
        signal: 0x8bc6a3ff,
    });
    assert_theme(ThemeExpectation {
        id: ThemeId::DeepPetrol,
        stable_id: "deep-petrol",
        display_name: "Deep Petrol",
        dark: true,
        base: 0x0d1619ff,
        raised: 0x1e3036ff,
        text: 0xe3edf0ff,
        accent: 0x94bdc5ff,
        signal: 0x8bc6a3ff,
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
        "{display_name} 的 Terminal 必须保持至少 80% 不透明的稳定内容面"
    );
    assert_ne!(
        theme.term_bg, theme.node_fill,
        "{display_name} 的 Terminal 内容井不得退化为普通玻璃 shell"
    );
}
