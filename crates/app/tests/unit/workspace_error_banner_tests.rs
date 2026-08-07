use super::*;

/// Workspace 错误横幅：Paper Blue 下正文与关闭按钮必须和深色底板形成可见对比。
///
/// 数据构造（含关键数值的推导过程）：
///   Paper Blue graphite RGB = 0x313d44（横幅底板沿用该深色）
///   expected foreground RGB = 0xffffff（深色底板上的高对比正文）
///   expected muted RGB      = 0xffffff（关闭按钮可降低 alpha，但 RGB 仍保持白色）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Paper Blue 主题并计算 workspace error 专用配色。
///   2. 分别比较底板、正文和关闭按钮的 RGB 通道，忽略透明度差异。
///   3. 校验正文使用 0xf5 alpha、关闭按钮使用 0xb8 alpha。
///
/// 预期结果：
///   - 正断言：正文为 `0xfffffff5`，关闭按钮为 `0xffffffb8`。
///   - 负断言：正文和关闭按钮的 RGB 均不得等于 graphite 的 `0x313d44`。
#[test]
fn test_workspace_error_banner_uses_light_foreground_on_paper_blue() {
    let theme = Theme::paper_blue();
    let colors = workspace_error_banner_colors(theme);

    assert_eq!(
        colors.foreground, 0xfffffff5,
        "Paper Blue 的错误正文必须使用高对比白色 0xf5 alpha"
    );
    assert_eq!(
        colors.muted, 0xffffffb8,
        "Paper Blue 的错误关闭按钮必须使用可见白色 0xb8 alpha"
    );
    assert_ne!(
        colors.foreground & 0xffffff00,
        colors.background & 0xffffff00,
        "错误正文 RGB 不得与 graphite 底板相同，否则浅色主题下文字会消失"
    );
    assert_ne!(
        colors.muted & 0xffffff00,
        colors.background & 0xffffff00,
        "错误关闭按钮 RGB 不得与 graphite 底板相同，否则用户无法关闭横幅"
    );
}
