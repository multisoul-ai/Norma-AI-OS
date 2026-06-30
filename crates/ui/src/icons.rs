//! ui icons — 顶栏图标枚举与渲染辅助。GPUI svg() 按 asset 路径加载嵌入的 SVG 文件。

use gpui::{px, rgba, svg, SharedString, Styled};

/// Color alpha helper: replace the low byte of a 0xRRGGBBAA color with `alpha`.
/// Centralized here to replace the copies duplicated across app chrome modules.
pub fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

/// Toolbar icon variants. Each maps to an embedded SVG asset path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarIcon {
    Scroll,
    Plus,
    Terminal,
    Claude,
    Codex,
    OpenCode,
    Preview,
    Settings,
}

impl ToolbarIcon {
    /// Asset path resolved via the app's AssetSource (wired in crates/app/src/assets.rs).
    pub fn asset_path(self) -> &'static str {
        match self {
            ToolbarIcon::Scroll => "icons/toolbar-scroll.svg",
            ToolbarIcon::Plus => "icons/toolbar-plus.svg",
            ToolbarIcon::Terminal => "icons/toolbar-terminal.svg",
            ToolbarIcon::Claude => "icons/toolbar-claude.svg",
            ToolbarIcon::Codex => "icons/toolbar-codex.svg",
            ToolbarIcon::OpenCode => "icons/toolbar-opencode.svg",
            ToolbarIcon::Preview => "icons/toolbar-preview.svg",
            ToolbarIcon::Settings => "icons/toolbar-settings.svg",
        }
    }
}

/// Render a toolbar icon as a GPUI Svg element sized `size` px and tinted `color` (0xRRGGBBAA).
///
/// NOTE: gpui 0.2.2 `Styled` trait has no `.size()` method; sizing is via `.w()`/`.h()`.
pub fn toolbar_icon(icon: ToolbarIcon, color: u32, size: f32) -> gpui::Svg {
    svg()
        .path(SharedString::from(icon.asset_path()))
        .text_color(rgba(color))
        .w(px(size))
        .h(px(size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 顶栏图标 asset 路径：每个 ToolbarIcon 变体都映射到唯一的、命名规范的 SVG 路径。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   变体总数 = 8（Scroll / Plus / Terminal / Claude / Codex / OpenCode / Preview / Settings）
    ///   命名前缀 = "icons/toolbar-"
    ///   命名后缀 = ".svg"
    ///   期望路径表（variant -> asset_path）：
    ///     Scroll    -> "icons/toolbar-scroll.svg"
    ///     Plus      -> "icons/toolbar-plus.svg"
    ///     Terminal  -> "icons/toolbar-terminal.svg"
    ///     Claude    -> "icons/toolbar-claude.svg"
    ///     Codex     -> "icons/toolbar-codex.svg"
    ///     OpenCode  -> "icons/toolbar-opencode.svg"
    ///     Preview   -> "icons/toolbar-preview.svg"
    ///     Settings  -> "icons/toolbar-settings.svg"
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 遍历 8 个变体，调用 `ToolbarIcon::asset_path(variant)` 取得 &str
    ///   2. 对每条路径检查前缀 "icons/toolbar-" 与后缀 ".svg"
    ///   3. 对每条路径与期望字面量做精确相等断言
    ///   4. 将 8 条路径收集进 HashSet，断言去重后长度 == 8
    ///
    /// 预期结果：
    ///   - 正断言：每个变体的路径以 "icons/toolbar-" 开头
    ///   - 正断言：每个变体的路径以 ".svg" 结尾
    ///   - 正断言：每个变体的路径精确等于其期望字面量
    ///   - 负断言：8 个变体之间不存在重复路径（集合大小 == 8）
    #[test]
    fn toolbar_icon_asset_paths_are_unique_and_well_formed() {
        let variants = [
            ToolbarIcon::Scroll,
            ToolbarIcon::Plus,
            ToolbarIcon::Terminal,
            ToolbarIcon::Claude,
            ToolbarIcon::Codex,
            ToolbarIcon::OpenCode,
            ToolbarIcon::Preview,
            ToolbarIcon::Settings,
        ];
        // expected len = 8（与 ToolbarIcon 变体数一致，手工推导）
        let expected: [(&ToolbarIcon, &str); 8] = [
            (&ToolbarIcon::Scroll, "icons/toolbar-scroll.svg"),
            (&ToolbarIcon::Plus, "icons/toolbar-plus.svg"),
            (&ToolbarIcon::Terminal, "icons/toolbar-terminal.svg"),
            (&ToolbarIcon::Claude, "icons/toolbar-claude.svg"),
            (&ToolbarIcon::Codex, "icons/toolbar-codex.svg"),
            (&ToolbarIcon::OpenCode, "icons/toolbar-opencode.svg"),
            (&ToolbarIcon::Preview, "icons/toolbar-preview.svg"),
            (&ToolbarIcon::Settings, "icons/toolbar-settings.svg"),
        ];

        // 前缀/后缀校验：每条路径都遵循 icons/toolbar-<name>.svg 规范。
        for v in &variants {
            let path = v.asset_path();
            assert!(
                path.starts_with("icons/toolbar-"),
                "path {:?} for {:?} must start with \"icons/toolbar-\"",
                path,
                v
            );
            assert!(
                path.ends_with(".svg"),
                "path {:?} for {:?} must end with \".svg\"",
                path,
                v
            );
        }

        // 正断言：每个变体映射到其确切的期望路径（防错位/防默认占位）。
        for (variant, want) in expected.iter() {
            assert_eq!(
                variant.asset_path(),
                *want,
                "variant {:?} must map to its exact expected asset path",
                variant
            );
        }

        // 负断言：8 个变体之间无重复路径（任意两个变体不得共用同一 SVG）。
        // 推导：variants.len() == 8；若集合大小 < 8 则存在重复。
        let set: HashSet<&'static str> = variants.iter().map(|v| v.asset_path()).collect();
        assert_eq!(
            set.len(),
            variants.len(),
            "all {} toolbar icon variants must map to distinct asset paths; got {} unique paths",
            variants.len(),
            set.len()
        );
    }

    /// with_alpha 颜色辅助：用新 alpha 替换 0xRRGGBBAA 的低字节，保留高位 RGB。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   base color  = 0x10203040 (R=0x10 G=0x20 B=0x30 A=0x40)
    ///   new alpha   = 0xFF
    ///   mask        = 0xffffff00 (清零低字节，保留高 24 位 = RRGGBB)
    ///   expected    = (0x10203040 & 0xffffff00) | 0xFF = 0x102030FF
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. with_alpha(0x10203040, 0xFF) → 位运算保留 RGB，替换 alpha 字节
    ///   2. 结果应为 0x102030FF
    ///
    /// 预期结果：
    ///   - 正断言：替换后 RGB 分量不变（0x102030 保留）
    ///   - 正断言：alpha 字节被替换为 0xFF
    #[test]
    fn with_alpha_replaces_low_byte_preserving_rgb() {
        // base = 0x10203040, alpha = 0xFF
        // expected = 0x102030FF（推导见上方注释）
        let result = with_alpha(0x10203040, 0xFF);
        assert_eq!(
            result, 0x102030FF,
            "with_alpha must replace alpha byte (0x40 -> 0xFF) while keeping 0x102030 RGB"
        );
        // 正断言：alpha=0x00 时低字节为 0
        assert_eq!(
            with_alpha(0x10203040, 0x00),
            0x10203000,
            "with_alpha with alpha=0 must zero the low byte"
        );
    }
}
