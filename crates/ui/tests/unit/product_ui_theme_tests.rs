use super::Theme;

/// G-7: Product UI V2 默认主题必须使用已评审品牌字面量与浅色文字层级。
///
/// 数据构造（含关键数值的推导过程）：
///   graphite       = #111214ff（原型 `--graphite`）
///   canvas_from    = #f7f5f2ff（原型 `--warm-white`）
///   canvas_to      = #e8eef8d9（原型 `--canvas-mist`，alpha=0xd9≈85%）
///   node_fill      = #e8eef854（Instrument Glass shell，alpha=0x54≈33%）
///   title_bar      = #f8fafd70（冷白标题层，alpha=0x70≈44%）
///   term_bg        = #090e16eb（深色内容井，alpha=0xeb≈92%）
///   node_stroke    = #ffffffad（中性外壳描边，alpha=0xad≈68%）
///   text_secondary = #111214a3（graphite 64%，round(255×0.64)=163=0xa3）
///   text_tertiary  = #1112146b（graphite 42%，round(255×0.42)=107=0x6b）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 Theme::default()，走产品默认 `glass_light()`。
///   2. 对照 `docs/product-ui.html` 的独立字面量检查基础面、玻璃面与状态色。
///   3. 比较三层文字 alpha，确保浅色界面层级从 primary 递减。
///
/// 预期结果：
///   - 正断言：基础面、统一卡片层级、Cobalt/Violet/Acid/Coral/Mint 精确匹配原型。
///   - 正断言：primary > secondary > tertiary，浅色界面正文保持清晰层级。
///   - 负断言：卡片外壳不得回退到 70% 不透明度，让标题区双层叠白接近实色。
#[test]
fn test_product_ui_v2_brand_tokens_match_reviewed_literals() {
    let theme = Theme::default();

    assert_eq!(theme.graphite, 0x111214ff, "graphite must match #111214");
    assert_eq!(
        theme.canvas_from, 0xf7f5f2ff,
        "canvas_from must match warm-white"
    );
    assert_eq!(
        theme.canvas_to, 0xe8eef8d9,
        "canvas_to must match canvas-mist"
    );
    assert_eq!(
        theme.node_fill, 0xe8eef854,
        "node_fill must use the reviewed 33% frosted-glass shell"
    );
    assert_eq!(
        theme.title_bar, 0xf8fafd70,
        "title_bar must use the reviewed 44% frosted title surface"
    );
    assert_eq!(
        theme.term_bg, 0x090e16eb,
        "term_bg must use the reviewed 92% dark content well"
    );
    assert_eq!(
        theme.node_stroke, 0xffffffad,
        "node_stroke must use the shared 68% neutral shell border"
    );
    assert_eq!(
        theme.accent_blue, 0x2f80edff,
        "accent_blue must match Cobalt"
    );
    assert_eq!(
        theme.accent_violet, 0x6a5cffff,
        "accent_violet must match Violet"
    );
    assert_eq!(theme.accent_warm, 0xd6ff3bff, "accent_warm must match Acid");
    assert_eq!(
        theme.destructive, 0xff6b7aff,
        "destructive must match Coral"
    );
    assert_eq!(
        theme.accent_green, 0x64e0b5ff,
        "accent_green must match Mint"
    );
    assert!(
        alpha(theme.text_primary) > alpha(theme.text_secondary),
        "primary text alpha must exceed secondary text alpha"
    );
    assert!(
        alpha(theme.text_secondary) > alpha(theme.text_tertiary),
        "secondary text alpha must exceed tertiary text alpha"
    );
    assert_ne!(
        theme.node_fill, 0xecf2f9b3,
        "Canvas card shell must not return to the 70% surface that made card headers nearly opaque"
    );
}

fn alpha(rgba: u32) -> u8 {
    rgba.to_be_bytes()[3]
}
