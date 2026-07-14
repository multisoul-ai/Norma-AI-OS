use super::*;
use crate::canvas::text_selection::TextPoint;
use terminal::TerminalGrid;

/// 终端链接手势：普通点击应在 Norma 中打开，⌘ 点击应交给默认浏览器，双击和非网页文本不应重复打开。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = 80 cols × 24 lines
///   row 0 text = "https://example.com/docs"，链接首格坐标 = (row=0, col=0)
///   普通点击：platform_modifier=false，click_count=1
///   ⌘ 点击：platform_modifier=true，click_count=1
///   双击：platform_modifier=false，click_count=2；按产品约定只允许首击创建一次卡片
///   非网页文本 = "file:///private"，`terminal::link_at` 不会返回可打开 URL
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对同一 http(s) 单元格分别计算普通点击、⌘ 点击和双击意图
///   2. 对 file scheme 单元格计算普通点击意图
///   3. 比较纯决策结果，不触发真实 WebView 或系统 `open` 副作用
///
/// 预期结果：
///   - 断言 A：普通点击得到 OpenBrowserCard 与完整 URL
///   - 断言 B：⌘ 点击得到 OpenDefaultBrowser 与完整 URL
///   - 断言 C：双击得到 Ignore，避免创建第二张 Browser 卡片
///   - 断言 D：file 文本得到 InvalidLink，避免非网页 scheme 被打开且让 UI 可提示原因
#[test]
fn test_terminal_link_intent_routes_clicks_without_duplicate_or_unsafe_opening() {
    let mut web_grid = TerminalGrid::new(80, 24);
    web_grid.advance(b"https://example.com/docs");
    let web_snapshot = web_grid.snapshot();
    let point = TextPoint::new(0, 0);

    let normal = terminal_link_intent(&web_snapshot, point, false, 1);
    let external = terminal_link_intent(&web_snapshot, point, true, 1);
    let double = terminal_link_intent(&web_snapshot, point, false, 2);

    let mut file_grid = TerminalGrid::new(80, 24);
    file_grid.advance(b"file:///private");
    let file_snapshot = file_grid.snapshot();
    let unsafe_scheme = terminal_link_intent(&file_snapshot, point, false, 1);

    assert_eq!(
        normal,
        TerminalLinkIntent::OpenBrowserCard {
            url: "https://example.com/docs".to_string()
        },
        "普通点击 http(s) 链接应创建 Norma Browser 卡片并保留完整目标"
    );
    assert_eq!(
        external,
        TerminalLinkIntent::OpenDefaultBrowser {
            url: "https://example.com/docs".to_string()
        },
        "⌘ 点击 http(s) 链接应请求用户默认浏览器打开同一目标"
    );
    assert_eq!(
        double,
        TerminalLinkIntent::Ignore,
        "双击的第二击不应再创建 Browser 卡片，避免一次手势产生重复页面"
    );
    assert_eq!(
        unsafe_scheme,
        TerminalLinkIntent::InvalidLink,
        "file scheme 不应被打开，UI 应取得无效链接意图以显示简短错误提示"
    );
}
