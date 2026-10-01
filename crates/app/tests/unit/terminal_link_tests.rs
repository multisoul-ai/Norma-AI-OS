use super::*;
use crate::canvas::text_selection::TextPoint;
use terminal::TerminalGrid;

/// 终端链接手势：普通点击应在 Norma 中打开，⌘ 点击应交给默认浏览器，双击和非本机 URI 不应重复打开。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = 80 cols × 24 lines
///   row 0 text = "https://example.com/docs"，链接首格坐标 = (row=0, col=0)
///   普通点击：platform_modifier=false，click_count=1
///   ⌘ 点击：platform_modifier=true，click_count=1
///   双击：platform_modifier=false，click_count=2；按产品约定只允许首击创建一次卡片
///   非本机 URI = "ssh://example.com/private.png"，不是 Browser 或 Preview 支持的 scheme
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对同一 http(s) 单元格分别计算普通点击、⌘ 点击和双击意图
///   2. 对 ssh scheme 单元格计算普通点击意图
///   3. 比较纯决策结果，不触发真实 WebView 或系统 `open` 副作用
///
/// 预期结果：
///   - 断言 A：普通点击得到 OpenBrowserCard 与完整 URL
///   - 断言 B：⌘ 点击得到 OpenDefaultBrowser 与完整 URL
///   - 断言 C：双击得到 Ignore，避免创建第二张 Browser 卡片
///   - 断言 D：ssh URI 得到 InvalidLink，避免非本机 scheme 被打开且让 UI 可提示原因
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
    file_grid.advance(b"ssh://example.com/private.png");
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
        "ssh scheme 不应被 Browser 或 Preview 打开，UI 应取得无效链接意图"
    );
}

/// Agent 本机交付物：点击工作区外的 file URI 应创建 Preview，而不是归为无效网页链接。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = 240 cols × 24 lines，完整 URI 长度小于 240，不发生终端换行
///   URI = "file:///Users/alan/.codex/generated_images/run/output.png"
///   点击位置 = row 0, col 0，即 URI 首格
///   普通点击 = platform_modifier=false, click_count=1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将工作区外图片 file URI 写入 Agent/Terminal 共用的 ANSI grid。
///   2. 对 URI 首格执行一次普通点击决策。
///   3. 检查点击目标是否完整传入 Preview 创建意图。
///
/// 预期结果：
///   - 正断言：得到 OpenPreviewCard，并保留完整 file URI 供 app 规范化。
///   - 负断言：不得得到 InvalidLink，否则 Preview 创建与异步读取链路不会执行。
#[test]
fn test_terminal_link_intent_routes_external_file_uri_to_preview() {
    let uri = "file:///Users/alan/.codex/generated_images/run/output.png";
    let mut grid = TerminalGrid::new(240, 24);
    grid.advance(uri.as_bytes());
    let snapshot = grid.snapshot();

    let intent = terminal_link_intent(&snapshot, TextPoint::new(0, 0), false, 1);

    assert_eq!(
        intent,
        TerminalLinkIntent::OpenPreviewCard {
            paths: vec![uri.to_string()]
        },
        "工作区外图片 file URI 应进入 Preview 创建链路并保留完整目标"
    );
    assert_ne!(
        intent,
        TerminalLinkIntent::InvalidLink,
        "工作区外图片 file URI 不应被误判为无效网页链接"
    );
}

/// Agent 本机路径：绝对路径和可越出 workspace 的相对路径都应进入只读 Preview。
///
/// 数据构造（含关键数值的推导过程）：
///   每个 grid = 160 cols × 24 lines，两个路径均短于 160，不发生终端换行
///   绝对路径 = "/Users/alan/Downloads/output.png"
///   外部相对路径 = "../shared/result.md"
///   点击位置 = row 0, col 0；普通单击
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别把绝对路径和含 `..` 的相对路径写入 ANSI grid。
///   2. 对两个路径首格分别计算点击意图。
///   3. 检查两者是否保持原始目标进入 app 路径解析。
///
/// 预期结果：
///   - 正断言：绝对路径得到 OpenPreviewCard 并保持完整路径。
///   - 正断言：外部相对路径得到 OpenPreviewCard 并保持 `..` 分量。
///   - 负断言：两种本机路径都不得被归为 InvalidLink。
#[test]
fn test_terminal_link_intent_accepts_local_paths_outside_workspace() {
    let absolute_path = "/Users/alan/Downloads/output.png";
    let relative_outside_path = "../shared/result.md";
    let mut absolute_grid = TerminalGrid::new(160, 24);
    absolute_grid.advance(absolute_path.as_bytes());
    let absolute_snapshot = absolute_grid.snapshot();
    let mut relative_grid = TerminalGrid::new(160, 24);
    relative_grid.advance(relative_outside_path.as_bytes());
    let relative_snapshot = relative_grid.snapshot();

    let absolute_intent = terminal_link_intent(&absolute_snapshot, TextPoint::new(0, 0), false, 1);
    let relative_intent = terminal_link_intent(&relative_snapshot, TextPoint::new(0, 0), false, 1);

    assert_eq!(
        absolute_intent,
        TerminalLinkIntent::OpenPreviewCard {
            paths: vec![absolute_path.to_string()]
        },
        "工作区外绝对路径应进入只读 Preview 创建链路"
    );
    assert_eq!(
        relative_intent,
        TerminalLinkIntent::OpenPreviewCard {
            paths: vec![relative_outside_path.to_string()]
        },
        "含 .. 的相对路径应保留原值，交给 app 基于 workspace root 解析"
    );
    assert_ne!(
        absolute_intent,
        TerminalLinkIntent::InvalidLink,
        "工作区外绝对路径不得被误判为 InvalidLink"
    );
    assert_ne!(
        relative_intent,
        TerminalLinkIntent::InvalidLink,
        "可越出 workspace 的相对路径不得被误判为 InvalidLink"
    );
}
