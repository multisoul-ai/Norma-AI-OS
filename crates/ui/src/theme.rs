//! 主题 token（颜色使用 0xRRGGBBAA）。当前默认是浅色玻璃主题。

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub graphite: u32,
    pub panel: u32,
    pub panel_high: u32,
    pub canvas_from: u32,
    pub canvas_to: u32,
    pub node_fill: u32,
    pub node_stroke: u32,
    pub selected_stroke: u32,
    pub focused_stroke: u32,
    pub edge_highlight: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_tertiary: u32,
    pub accent_orange: u32,
    pub accent_cyan: u32,
    pub accent_green: u32,
    pub accent_violet: u32,
    pub accent_blue: u32,
    pub accent_warm: u32,
    pub destructive: u32,
    pub term_fg: u32,
    pub term_bg: u32,
    pub term_exit_fg: u32,
    pub title_bar: u32,
    pub text: u32,
    pub shadow_color: u32,
    pub shadow_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
}

impl Theme {
    pub fn glass_light() -> Theme {
        Theme {
            graphite: 0x05090bff,
            panel: 0x0e1b16cc,
            panel_high: 0x1f372ab8,
            canvas_from: 0xf8fbffff,
            canvas_to: 0xe8eef8d9,
            node_fill: 0xffffffcc,
            node_stroke: 0xffffffd9,
            selected_stroke: 0x0a84ffff,
            focused_stroke: 0x006fe6ff,
            edge_highlight: 0xfffffff2,
            text_primary: 0xffffffff,
            text_secondary: 0xffffffad,
            text_tertiary: 0xffffff75,
            accent_orange: 0xff7932ff,
            accent_cyan: 0x27d7eaff,
            accent_green: 0x60f078ff,
            accent_violet: 0x9b55ffff,
            accent_blue: 0x4b70ffff,
            accent_warm: 0xf2c26bff,
            destructive: 0xff3c4dff,
            term_fg: 0xe7edf5ff,
            term_bg: 0x111827f6,
            term_exit_fg: 0xff6961ff,
            title_bar: 0xffffffb8,
            text: 0x1f2937ff,
            shadow_color: 0x38506a2e,
            shadow_y: 18.0,
            shadow_blur: 44.0,
            shadow_spread: -18.0,
        }
    }

    pub fn glass_dark() -> Theme {
        Theme {
            graphite: 0x05090bff,
            panel: 0x0e1b16cc,
            panel_high: 0x1f372ab8,
            canvas_from: 0x101722f2,
            canvas_to: 0x1d2635e6,
            node_fill: 0x0e1b16d9,
            node_stroke: 0xffffff24,
            selected_stroke: 0xff7932ff,
            focused_stroke: 0x27d7eaff,
            edge_highlight: 0xffffff3d,
            text_primary: 0xfffffff5,
            text_secondary: 0xffffffad,
            text_tertiary: 0xffffff75,
            accent_orange: 0xff7932ff,
            accent_cyan: 0x27d7eaff,
            accent_green: 0x60f078ff,
            accent_violet: 0x9b55ffff,
            accent_blue: 0x4b70ffff,
            accent_warm: 0xf2c26bff,
            destructive: 0xff3c4dff,
            term_fg: 0xe7edf5ff,
            term_bg: 0x080d13fa,
            term_exit_fg: 0xff6961ff,
            title_bar: 0x13231dd9,
            text: 0xf2f6fbff,
            shadow_color: 0x00000073,
            shadow_y: 20.0,
            shadow_blur: 48.0,
            shadow_spread: -20.0,
        }
    }

    pub fn dark() -> Theme {
        Theme::glass_dark()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::glass_light()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// G-1: glass_light() 返回默认浅色玻璃主题 token
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   node_fill alpha  = 0xcc / 0xff ≈ 80%（玻璃面板需要半透明）
    ///   title_bar alpha  = 0xb8 / 0xff ≈ 72%（标题栏比面板更轻）
    ///   term_bg alpha    = 0xf6 / 0xff ≈ 96%（终端内容区可读优先）
    ///   selected/focused = Apple 蓝同色系（不再使用绿色聚焦）
    ///
    /// 执行过程：
    ///   1. 调用 Theme::glass_light() 创建浅色玻璃主题
    ///   2. 逐项验证默认浅色、玻璃透明度、终端高不透明度与 Apple 蓝强调色
    ///
    /// 预期结果：
    ///   - canvas_from / canvas_to 存在且不同（雾面渐变）
    ///   - node_fill / title_bar 不是不透明色（玻璃外壳）
    ///   - term_bg 比 node_fill 更不透明（终端可读优先）
    ///   - focused_stroke 不是旧绿色（统一 Apple 蓝语言）
    #[test]
    fn test_glass_light_theme_tokens() {
        let theme = Theme::glass_light();
        assert_eq!(
            theme.canvas_from, 0xf8fbffff,
            "canvas_from should be a bright frosted glass start color"
        );
        assert_eq!(
            theme.canvas_to, 0xe8eef8d9,
            "canvas_to should be a cool translucent glass end color"
        );
        assert_ne!(
            theme.canvas_from, theme.canvas_to,
            "canvas colors should differ to create a subtle frosted gradient"
        );
        assert_eq!(
            alpha(theme.node_fill),
            0xcc,
            "node_fill alpha should be semi-transparent glass"
        );
        assert_eq!(
            alpha(theme.title_bar),
            0xb8,
            "title_bar alpha should be lighter than the panel body"
        );
        assert_eq!(
            alpha(theme.term_bg),
            0xf6,
            "term_bg alpha should stay high for terminal readability"
        );
        assert!(
            alpha(theme.term_bg) > alpha(theme.node_fill),
            "terminal background should be more opaque than glass node fill"
        );
        assert_eq!(
            theme.selected_stroke, 0x0a84ffff,
            "selected_stroke should use Apple blue"
        );
        assert_eq!(
            theme.focused_stroke, 0x006fe6ff,
            "focused_stroke should stay in the Apple blue family"
        );
        assert_ne!(
            theme.focused_stroke, 0x35d07fff,
            "focused_stroke should not use the old green focus color"
        );
    }

    /// G-2: glass_dark() 提供暗色玻璃备用主题且 dark() 兼容指向它
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   canvas_from alpha = 0xf2 / 0xff ≈ 95%（暗色画布需要稳定底色）
    ///   node_fill alpha   = 0xd9 / 0xff ≈ 85%（暗色玻璃比浅色略厚）
    ///   term_bg alpha     = 0xfa / 0xff ≈ 98%（暗色终端仍保持最高可读性）
    ///
    /// 执行过程：
    ///   1. 调用 Theme::glass_dark() 创建暗色玻璃主题
    ///   2. 调用 Theme::dark() 验证旧入口仍兼容
    ///   3. 验证暗色主题不等于浅色主题，且终端比节点更不透明
    ///
    /// 预期结果：
    ///   - glass_dark 与 glass_light 不同（确实有双主题）
    ///   - dark() 与 glass_dark() 一致（旧调用方不会得到旧纯色主题）
    ///   - dark terminal 背景比节点 fill 更不透明
    #[test]
    fn test_glass_dark_and_dark_alias() {
        let light_theme = Theme::glass_light();
        let glass_dark = Theme::glass_dark();
        let dark_theme = Theme::dark();
        assert_ne!(
            glass_dark.canvas_from, light_theme.canvas_from,
            "glass_dark should not reuse light canvas start color"
        );
        assert_eq!(
            glass_dark.canvas_from, 0x101722f2,
            "glass_dark canvas_from should be a stable dark glass base"
        );
        assert_eq!(
            alpha(glass_dark.node_fill),
            0xd9,
            "glass_dark node_fill should be translucent but thicker than light glass"
        );
        assert!(
            alpha(glass_dark.term_bg) > alpha(glass_dark.node_fill),
            "glass_dark terminal should be more opaque than node fill"
        );
        assert_eq!(
            dark_theme.canvas_from, glass_dark.canvas_from,
            "dark() should alias glass_dark() canvas_from"
        );
        assert_eq!(
            dark_theme.term_bg, glass_dark.term_bg,
            "dark() should alias glass_dark() terminal background"
        );
    }

    /// G-3: Default trait 固定选择浅色玻璃主题
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   默认策略来自 SPEC：第一版直接替换默认主题为 glass_light()
    ///   canvas_from = 0xf8fbffff，用作浅色雾面渐变起点
    ///   dark canvas_from = 0x101722f2，不应成为 Default
    ///
    /// 执行过程：
    ///   1. 调用 Theme::default()
    ///   2. 调用 Theme::glass_light()
    ///   3. 调用 Theme::glass_dark()
    ///   4. 比较关键 token
    ///
    /// 预期结果：
    ///   - Default 等于 glass_light
    ///   - Default 不等于 glass_dark
    ///   - Default 的 canvas_from 是浅色玻璃起点
    #[test]
    fn test_default_equals_glass_light() {
        let default_theme = Theme::default();
        let light_theme = Theme::glass_light();
        let dark_theme = Theme::glass_dark();
        assert_eq!(
            default_theme.canvas_from, light_theme.canvas_from,
            "Default should use glass_light canvas_from"
        );
        assert_eq!(
            default_theme.node_fill, light_theme.node_fill,
            "Default should use glass_light node fill"
        );
        assert_eq!(
            default_theme.term_bg, light_theme.term_bg,
            "Default should use glass_light terminal background"
        );
        assert_ne!(
            default_theme.canvas_from, dark_theme.canvas_from,
            "Default should not use glass_dark canvas_from"
        );
        assert_ne!(
            default_theme.node_fill, dark_theme.node_fill,
            "Default should not use glass_dark node fill"
        );
    }

    /// G-4: Theme 仍保持 Debug、Clone、Copy 这些轻量 token trait
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   使用 glass_light() 作为主题样本
    ///   copy/clone 是位级轻量复制，不涉及运行时资源或 GPUI 对象
    ///
    /// 执行过程：
    ///   1. 创建主题实例
    ///   2. 验证 debug 格式化成功
    ///   3. 验证 clone 生成独立副本
    ///   4. 验证 copy 能够隐式复制
    ///
    /// 预期结果：
    ///   - format!("{:?}", theme) 应成功生成 Debug 字符串
    ///   - Debug 字符串应包含 "Theme" 字样
    ///   - cloned_theme 应与原始 theme 的所有字段相同
    ///   - copy_theme 应与原始 theme 的所有字段相同
    #[test]
    fn test_theme_derives() {
        let theme = Theme::glass_light();
        let debug_str = format!("{:?}", theme);
        assert!(
            debug_str.contains("Theme"),
            "Debug output should contain 'Theme'"
        );

        let cloned_theme = theme.clone();
        assert_eq!(
            cloned_theme.canvas_from, theme.canvas_from,
            "Cloned theme should have same canvas_from"
        );
        assert_eq!(
            cloned_theme.text, theme.text,
            "Cloned theme should have same text color"
        );

        let copy_theme = theme;
        assert_eq!(
            copy_theme.canvas_from, theme.canvas_from,
            "Copied theme should have same canvas_from"
        );
        assert_eq!(
            copy_theme.focused_stroke, theme.focused_stroke,
            "Copied theme should have same focused_stroke"
        );
    }

    /// G-5: Workspace V1 主题提供品牌语义 token 且不是单色调主题
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   graphite = 0x05090bff（深色工作台基底）
    ///   panel alpha = 0xcc / 0xff ≈ 80%（玻璃面板）
    ///   orange/cyan/green/violet/blue/warm/destructive 均为独立语义色
    ///
    /// 执行过程：
    ///   1. 调用 Theme::glass_dark() 作为 Workspace V1 主视觉样本
    ///   2. 验证品牌基底、文本三层和所有状态色存在
    ///   3. 验证关键状态色互不相等，避免退化为单一 hue
    ///
    /// 预期结果：
    ///   - 正断言：graphite/panel/text token 与设计规范一致
    ///   - 正断言：橙/青/绿/紫/蓝/暖/危险色均存在且互相区分
    ///   - 负断言：destructive 不应复用 orange，错误态不能被主操作色覆盖
    #[test]
    fn test_workspace_v1_semantic_tokens_are_distinct() {
        let theme = Theme::glass_dark();
        assert_eq!(
            theme.graphite, 0x05090bff,
            "graphite should be the stable dark workspace base"
        );
        assert_eq!(
            alpha(theme.panel),
            0xcc,
            "panel alpha should be translucent glass at about 80%"
        );
        assert!(
            alpha(theme.text_primary) > alpha(theme.text_secondary),
            "text_primary should be more prominent than text_secondary"
        );
        assert!(
            alpha(theme.text_secondary) > alpha(theme.text_tertiary),
            "text_secondary should be more prominent than text_tertiary"
        );
        assert_eq!(
            theme.accent_orange, 0xff7932ff,
            "accent_orange should match the selected/current signal"
        );
        assert_eq!(
            theme.accent_cyan, 0x27d7eaff,
            "accent_cyan should match the live/orbit signal"
        );
        assert_eq!(
            theme.accent_green, 0x60f078ff,
            "accent_green should match the done/healthy signal"
        );
        assert_eq!(
            theme.accent_violet, 0x9b55ffff,
            "accent_violet should match the generation signal"
        );
        assert_eq!(
            theme.accent_blue, 0x4b70ffff,
            "accent_blue should match external/sync signal"
        );
        assert_eq!(
            theme.accent_warm, 0xf2c26bff,
            "accent_warm should match subtle warning/rim light"
        );
        assert_eq!(
            theme.destructive, 0xff3c4dff,
            "destructive should match explicit error/danger signal"
        );
        assert_ne!(
            theme.destructive, theme.accent_orange,
            "destructive must not reuse orange because errors need a separate semantic channel"
        );
        assert_ne!(
            theme.accent_cyan, theme.accent_green,
            "cyan live signal and green done signal should remain distinct"
        );
    }

    /// G-6: Workspace V1 终端内容区比玻璃节点更不透明，保障壁纸背景上的可读性
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   glass_dark.node_fill alpha = 0xd6 / 0xff ≈ 84%
    ///   glass_dark.term_bg alpha   = 0xfa / 0xff ≈ 98%
    ///   差值 = 0xfa - 0xd6 = 0x24 = 36 alpha levels
    ///
    /// 执行过程：
    ///   1. 调用 Theme::glass_dark()
    ///   2. 比较 term_bg 与 node_fill 的 alpha
    ///   3. 验证 terminal 前景不是三级弱文本
    ///
    /// 预期结果：
    ///   - 正断言：term_bg alpha 至少比 node_fill 高 24
    ///   - 正断言：term_fg 使用高可读前景
    ///   - 负断言：term_fg 不应等于 text_tertiary
    #[test]
    fn test_workspace_v1_terminal_readability_tokens() {
        let theme = Theme::glass_dark();
        assert!(
            alpha(theme.term_bg) >= alpha(theme.node_fill) + 24,
            "terminal background should be substantially more opaque than glass node fill"
        );
        assert_eq!(
            theme.term_fg, 0xe7edf5ff,
            "terminal foreground should remain high contrast and readable"
        );
        assert_ne!(
            theme.term_fg, theme.text_tertiary,
            "terminal foreground should not use tertiary weak text"
        );
    }

    fn alpha(rgba: u32) -> u8 {
        rgba.to_be_bytes()[3]
    }
}
