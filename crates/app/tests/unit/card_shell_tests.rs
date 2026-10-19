/// 卡片外壳自行绘制完整圆角背景，避免分区采样在顶角内部留下缺块。
///
/// 数据构造（含关键数值推导）：
///   GPUI 0.2.2 的子元素裁剪为矩形，自身 background 则遵循 corner_radii。
///   card radius = 14px；旧采样跳过整个 radius×radius 方块，错误挖掉圆弧内部。
///
/// 执行过程：
///   1. 截取 `node_div` 外壳构造链。
///   2. 检查背景与圆角由同一个外壳绘制，标题栏使用内圈半径。
///   3. 检查 Canvas 不再挂载矩形壁纸分区采样。
///
/// 预期结果：
///   - 正断言：外壳保留连续圆角背景，标题栏保留内圈圆角背景。
///   - 负断言：不得再挂载会挖空角落的分区采样层。
#[test]
fn test_node_card_shell_paints_continuous_rounded_material() {
    let source = include_str!("../../src/canvas/view.rs");
    let shell = source
        .split("let mut node_div = div()")
        .nth(1)
        .and_then(|source| source.split("if hovered_terminal_link").next())
        .expect("应能定位 node_div 外壳构造链，否则无法验证圆角背景所有权");

    assert!(
        shell.contains(".rounded(px(node_radius))"),
        "卡片外壳必须保留统一圆角轮廓"
    );
    assert!(
        shell.contains(".bg(card_background(theme))"),
        "背景必须由圆角外壳自身绘制，保证顶角与中间拥有相同材质"
    );
    assert!(
        source.contains(".rounded_t(px(content_radius))")
            && source.contains(".bg(card_title_background("),
        "标题栏必须同时保留顶部圆角与毛玻璃背景"
    );
    assert!(
        !source.contains("render_card_frosted_backdrop"),
        "不得再挂载挖空角落的采样层"
    );
}
