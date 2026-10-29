use ui::{Theme, ThemeId};

/// 场景：用户取消浅色版本，目录只提供六套深色，首次使用默认墨黑。
/// 数据构造：独立设计清单为三套中性色、现有冷蓝及两套矿物主题；总数 3 + 3 = 6。
/// 执行过程：从公开目录读取稳定 id、主题极性和默认入口。
/// 预期结果：顺序和默认值符合选择，不能继续暴露任一旧浅色主题。
#[test]
fn dark_only_catalog_defaults_to_ink_black() {
    let ids = ThemeId::ALL.map(|id| id.as_str());
    assert_eq!(
        ids,
        [
            "ink-black",
            "graphite",
            "warm-obsidian",
            "sapphire-night",
            "moonstone",
            "deep-petrol"
        ],
        "目录应为六套深色主题"
    );
    assert_eq!(
        ThemeId::default().as_str(),
        "ink-black",
        "默认主题应为用户偏好的墨黑"
    );
    for id in ThemeId::ALL {
        assert!(id.is_dark(), "{id:?} 必须使用深色组件模式");
        assert!(!Theme::for_id(id).is_light(), "{id:?} 不能保留浅色材质");
        assert!(
            !["paper-blue", "linen-sage", "silver-violet"].contains(&id.as_str()),
            "浅色主题不得出现在目录"
        );
    }
}

/// 场景：旧浅色选择升级后进入墨黑，同时保留已有深色偏好。
/// 数据构造：三个退役 id、六个当前 id，以及一个未知 id。
/// 执行过程：通过稳定 id 的恢复入口解析，不读取系统明暗外观。
/// 预期结果：旧浅色统一迁移为墨黑；有效深色不被覆盖；未知值交由默认回退。
#[test]
fn saved_light_ids_migrate_without_overwriting_dark_preferences() {
    for old in ["paper-blue", "linen-sage", "silver-violet"] {
        assert_eq!(
            ThemeId::from_saved_id(old),
            Some(ThemeId::InkBlack),
            "{old} 应迁移到墨黑"
        );
    }
    for id in ThemeId::ALL {
        assert_eq!(
            ThemeId::from_saved_id(id.as_str()),
            Some(id),
            "{id:?} 已有深色选择应保持"
        );
    }
    assert_eq!(
        ThemeId::from_saved_id("unknown-theme"),
        None,
        "未知 id 不能冒充有效主题"
    );
}

/// 场景：中性与矿物深色用于长期阅读，正文与辅助字应清楚，表面不能压成同一块黑。
/// 数据构造：设计要求墨黑 RGB 通道差不超过 3；小字号对比至少 4.5:1，正文至少 7:1。
/// 执行过程：读取公开主题 token，计算 sRGB 相对亮度与阅读面的对比。
/// 预期结果：墨黑近中性，五套都有层级，辅助字实色，终端以柔白配深底。
#[test]
fn neutral_dark_materials_have_readable_layers() {
    fn luma(color: u32) -> f64 {
        let [r, g, b] = [24, 16, 8].map(|shift| {
            let v = ((color >> shift) & 255) as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        r * 0.2126 + g * 0.7152 + b * 0.0722
    }
    let ink = Theme::for_id(ThemeId::InkBlack);
    let channels = [24, 16, 8].map(|shift| (ink.canvas_from >> shift) & 255);
    assert!(
        channels.iter().max().unwrap() - channels.iter().min().unwrap() <= 3,
        "墨黑不能偏蓝或偏紫"
    );
    for id in [
        ThemeId::InkBlack,
        ThemeId::Graphite,
        ThemeId::WarmObsidian,
        ThemeId::Moonstone,
        ThemeId::DeepPetrol,
    ] {
        let theme = Theme::for_id(id);
        assert!(
            luma(theme.panel_high) > luma(theme.canvas_from),
            "{id:?} 提升层应比画布亮"
        );
        assert!(luma(theme.term_bg) < 0.02, "{id:?} 终端保持稳定深底");
        for text in [
            theme.text_primary,
            theme.text_secondary,
            theme.text_tertiary,
        ] {
            let contrast = (luma(text) + 0.05) / (luma(theme.panel_high) + 0.05);
            assert!(
                contrast >= 4.5,
                "{id:?} 文字对比不能低于4.5:1，实际 {contrast}"
            );
            assert_eq!(text & 255, 255, "{id:?} 文字不随玻璃材质降低透明度");
        }
        assert!(
            (luma(theme.term_fg) + 0.05) / (luma(theme.term_bg) + 0.05) >= 7.0,
            "{id:?} 终端正文应至少7:1"
        );
        assert_ne!(
            theme.node_fill, theme.term_bg,
            "{id:?} 卡片壳和终端内容面应分层"
        );
    }
}

/// 场景：已保存的旧紫、绿主题升级为用户选定的新材质，不回退墨黑。
/// 数据构造：旧 id 与新 canonical id 的独立映射及原型的关键色值。
/// 执行过程：从持久化恢复入口读取目录和材质，同时验证新 id 可往返。
/// 预期结果：旧 id 不再出现在选择器，新色值生效，终端约 97% 不透明。
#[test]
fn replacement_themes_restore_legacy_preferences_and_selected_prototype() {
    for (old, current, from, to, text, accent, terminal) in [
        (
            "midnight-violet",
            "moonstone",
            0x111117ff,
            0x292830ff,
            0xe9e7efff,
            0xc1b9d5ff,
            0x101014f8,
        ),
        (
            "forest-mint",
            "deep-petrol",
            0x0d1619ff,
            0x1e3036ff,
            0xe3edf0ff,
            0x94bdc5ff,
            0x0c1418f8,
        ),
    ] {
        let restored = ThemeId::from_saved_id(old).expect("旧主题应有迁移目标");
        assert_eq!(restored.as_str(), current, "{old} 应迁移到选定替代款");
        assert_eq!(
            ThemeId::from_saved_id(current),
            Some(restored),
            "新 id 应能持久化往返"
        );
        assert!(
            !ThemeId::ALL.iter().any(|id| id.as_str() == old),
            "旧主题不得继续出现在目录"
        );
        let theme = Theme::for_id(restored);
        assert_eq!(theme.canvas_from, from, "{current} 背景起点来自选定原型");
        assert_eq!(theme.canvas_to, to, "{current} 背景终点来自选定原型");
        assert_eq!(theme.text_primary, text, "{current} 正文来自选定原型");
        assert_eq!(theme.accent, accent, "{current} 强调色应低饱和");
        // 248 / 255 = 97.25%，保留稳定终端阅读面。
        assert_eq!(theme.term_bg, terminal, "{current} 终端材质来自选定原型");
        assert_eq!(theme.text_secondary & 255, 255, "辅助字不能随材质透明");
        assert_eq!(theme.text_tertiary & 255, 255, "三级文字不能随材质透明");
    }
}
