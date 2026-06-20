//! 主题色（0xRRGGBB）。本切片仅一套 dark 主题，够验证选中/聚焦反馈。

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub canvas_bg: u32,
    pub node_fill: u32,
    pub node_stroke: u32,
    pub selected_stroke: u32,
    pub focused_stroke: u32,
    pub term_fg: u32,
    pub term_bg: u32,
    pub title_bar: u32,
    pub text: u32,
}

impl Theme {
    pub fn dark() -> Theme {
        Theme {
            canvas_bg: 0x14161a,
            node_fill: 0x2a2d34,
            node_stroke: 0x3a3f47,
            selected_stroke: 0x4f9dff,
            focused_stroke: 0x35d07f,
            term_fg: 0xd0d0d0,
            term_bg: 0x1e1e1e,
            title_bar: 0x32363d,
            text: 0xe6e6e6,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 dark() 返回正确的主题色值
    ///
    /// 数据构造：调用 Theme::dark() 获取暗色主题
    ///
    /// 执行过程：
    ///   1. 调用 Theme::dark() 创建主题
    ///   2. 验证各字段的十六进制颜色值
    ///
    /// 预期结果：
    ///   - canvas_bg 应为 0x14161a（深灰背景）
    ///   - node_fill 应为 0x2a2d34（节点填充色）
    ///   - node_stroke 应为 0x3a3f47（节点边框色）
    ///   - selected_stroke 应为 0x4f9dff（蓝色选中反馈）
    ///   - focused_stroke 应为 0x35d07f（绿色聚焦反馈）
    ///   - term_fg 应为 0xd0d0d0（终端前景色）
    ///   - term_bg 应为 0x1e1e1e（终端背景色）
    ///   - title_bar 应为 0x32363d（标题栏色）
    ///   - text 应为 0xe6e6e6（文本色）
    #[test]
    fn test_dark_theme_colors() {
        let theme = Theme::dark();
        assert_eq!(theme.canvas_bg, 0x14161a, "canvas_bg should be dark gray background");
        assert_eq!(theme.node_fill, 0x2a2d34, "node_fill should be node fill color");
        assert_eq!(theme.node_stroke, 0x3a3f47, "node_stroke should be node stroke color");
        assert_eq!(theme.selected_stroke, 0x4f9dff, "selected_stroke should be blue selection feedback");
        assert_eq!(theme.focused_stroke, 0x35d07f, "focused_stroke should be green focus feedback");
        assert_eq!(theme.term_fg, 0xd0d0d0, "term_fg should be terminal foreground color");
        assert_eq!(theme.term_bg, 0x1e1e1e, "term_bg should be terminal background color");
        assert_eq!(theme.title_bar, 0x32363d, "title_bar should be title bar color");
        assert_eq!(theme.text, 0xe6e6e6, "text should be text color");
    }

    /// 验证 Default trait 实现正确调用 dark()
    ///
    /// 数据构造：使用 Theme::default() 和 Theme::dark()
    ///
    /// 执行过程：
    ///   1. 调用 Theme::default() 创建默认主题
    ///   2. 调用 Theme::dark() 创建暗色主题
    ///   3. 比较两者的所有字段
    ///
    /// 预期结果：
    ///   - Default::default() 应返回与 dark() 相同的值
    ///   - Default::default() 返回的主题不应为 None
    ///   - Dark theme 的所有字段值应与 Default 值一致
    #[test]
    fn test_default_equals_dark() {
        let default_theme = Theme::default();
        let dark_theme = Theme::dark();
        assert_eq!(default_theme.canvas_bg, dark_theme.canvas_bg, "Default should have same canvas_bg as dark()");
        assert_eq!(default_theme.node_fill, dark_theme.node_fill, "Default should have same node_fill as dark()");
        assert_eq!(default_theme.node_stroke, dark_theme.node_stroke, "Default should have same node_stroke as dark()");
        assert_eq!(default_theme.selected_stroke, dark_theme.selected_stroke, "Default should have same selected_stroke as dark()");
        assert_eq!(default_theme.focused_stroke, dark_theme.focused_stroke, "Default should have same focused_stroke as dark()");
        assert_eq!(default_theme.term_fg, dark_theme.term_fg, "Default should have same term_fg as dark()");
        assert_eq!(default_theme.term_bg, dark_theme.term_bg, "Default should have same term_bg as dark()");
        assert_eq!(default_theme.title_bar, dark_theme.title_bar, "Default should have same title_bar as dark()");
        assert_eq!(default_theme.text, dark_theme.text, "Default should have same text as dark()");
    }

    /// 验证 Theme 实现了 Debug、Clone、Copy traits
    ///
    /// 数据构造：创建一个 Theme 实例
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
        let theme = Theme::dark();
        let debug_str = format!("{:?}", theme);
        assert!(debug_str.contains("Theme"), "Debug output should contain 'Theme'");

        let cloned_theme = theme.clone();
        assert_eq!(cloned_theme.canvas_bg, theme.canvas_bg, "Cloned theme should have same canvas_bg");
        assert_eq!(cloned_theme.text, theme.text, "Cloned theme should have same text color");

        let copy_theme = theme;
        assert_eq!(copy_theme.canvas_bg, theme.canvas_bg, "Copied theme should have same canvas_bg");
        assert_eq!(copy_theme.focused_stroke, theme.focused_stroke, "Copied theme should have same focused_stroke");
    }
}
