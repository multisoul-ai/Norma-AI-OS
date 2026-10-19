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

    let normal = terminal_link_intent(&web_snapshot, point, false, 1, &unverified);
    let external = terminal_link_intent(&web_snapshot, point, true, 1, &unverified);
    let double = terminal_link_intent(&web_snapshot, point, false, 2, &unverified);

    let mut file_grid = TerminalGrid::new(80, 24);
    file_grid.advance(b"ssh://example.com/private.png");
    let file_snapshot = file_grid.snapshot();
    let unsafe_scheme = terminal_link_intent(&file_snapshot, point, false, 1, &unverified);

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
///   - 正断言：得到 OpenPath，并保留完整 file URI 供 app 规范化。
///   - 负断言：不得得到 InvalidLink，否则 Preview 创建与异步读取链路不会执行。
#[test]
fn test_terminal_link_intent_routes_external_file_uri_to_preview() {
    let uri = "file:///Users/alan/.codex/generated_images/run/output.png";
    let mut grid = TerminalGrid::new(240, 24);
    grid.advance(uri.as_bytes());
    let snapshot = grid.snapshot();

    let intent = terminal_link_intent(&snapshot, TextPoint::new(0, 0), false, 1, &unverified);

    assert_eq!(
        intent,
        TerminalLinkIntent::OpenPath {
            paths: vec![uri.to_string()],
            system: false
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
///   - 正断言：绝对路径得到 OpenPath 并保持完整路径。
///   - 正断言：外部相对路径得到 OpenPath 并保持 `..` 分量。
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

    let absolute_intent = terminal_link_intent(
        &absolute_snapshot,
        TextPoint::new(0, 0),
        false,
        1,
        &unverified,
    );
    let relative_intent = terminal_link_intent(
        &relative_snapshot,
        TextPoint::new(0, 0),
        false,
        1,
        &unverified,
    );

    assert_eq!(
        absolute_intent,
        TerminalLinkIntent::OpenPath {
            paths: vec![absolute_path.to_string()],
            system: false
        },
        "工作区外绝对路径应进入只读 Preview 创建链路"
    );
    assert_eq!(
        relative_intent,
        TerminalLinkIntent::OpenPath {
            paths: vec![relative_outside_path.to_string()],
            system: false
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

/// 代码文件与行列后缀：点击 `crates/app/src/lib.rs:10:2` 应得到 OpenPath，后缀原样交给 app 解析；
/// ⌘ 点击同一路径应请求系统应用打开（system=true），而不是 Norma 内部预览。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = 80 cols × 24 lines；路径 "crates/app/src/lib.rs:10:2" 共 26 字符 < 80，不折行。
///   点击位置 = (row 0, col 5)，落在 "crates" 之内。
///   旧白名单只含 md/txt/log/图片，`.rs` 会被判为无效；放宽后应可点击。
///
/// 执行过程：
///   1. 写入路径，分别以 platform_modifier=false/true 计算意图。
///   2. 对仅含裸文件名 "lib.rs"（NeedsVerify）的行计算普通点击。
///
/// 预期结果：
///   - 普通点击 → OpenPath{paths=[原文], system=false}。
///   - ⌘ 点击 → OpenPath{paths=[原文], system=true}。
///   - 裸文件名在未验证前不得产生 OpenPath（保持文本选择），避免普通单词被误当链接。
#[test]
fn test_terminal_link_intent_opens_code_path_with_suffix_and_command_uses_system() {
    let text = "crates/app/src/lib.rs:10:2";
    let mut grid = TerminalGrid::new(80, 24);
    grid.advance(text.as_bytes());
    let snapshot = grid.snapshot();
    let point = TextPoint::new(0, 5);

    let normal = terminal_link_intent(&snapshot, point, false, 1, &unverified);
    let command = terminal_link_intent(&snapshot, point, true, 1, &unverified);

    assert_eq!(
        normal,
        TerminalLinkIntent::OpenPath {
            paths: vec![text.to_string()],
            system: false
        },
        "普通点击代码文件路径应在 Norma 内打开并保留 :行:列 后缀"
    );
    assert_eq!(
        command,
        TerminalLinkIntent::OpenPath {
            paths: vec![text.to_string()],
            system: true
        },
        "⌘ 点击路径应请求系统应用打开"
    );

    let mut bare = TerminalGrid::new(80, 24);
    bare.advance(b"lib.rs");
    let bare_intent = terminal_link_intent(
        &bare.snapshot(),
        TextPoint::new(0, 1),
        false,
        1,
        &unverified,
    );
    assert_eq!(
        bare_intent,
        TerminalLinkIntent::BeginTextSelection,
        "未验证存在的裸文件名不得变成链接"
    );
}

fn unverified(_: &PathRef) -> bool {
    false
}

fn verified(_: &PathRef) -> bool {
    true
}

/// 有下划线就必须能点：缓存确认存在的裸文件名（NeedsVerify）点击应打开，未确认的仍保持文本选择。
///
/// 数据构造：
///   grid = 80 cols × 24 lines；row 0 = "see lib.rs now"。
///   "lib.rs" 位于列 4..10（"see " 占 4 格，"lib.rs" 共 6 字符），点击列 5 落在其内。
///   "lib.rs" 无目录 → NeedsVerify；`verified` 模拟缓存报告 Exists，`unverified` 模拟 Missing/Unknown。
///
/// 执行过程：
///   1. 用 verified 计算普通点击与 ⌘ 点击。
///   2. 用 unverified 计算普通点击。
///   3. 点击空白处（列 2，"see" 内但不在路径内）用 verified 计算。
///
/// 预期结果：
///   - 已确认存在 → OpenPath{paths=["lib.rs"], system=false}；⌘ → system=true。
///   - 未确认存在 → BeginTextSelection，普通单词不会被误当链接。
///   - 点在 "see" 上 → BeginTextSelection，verified 只对命中的路径生效，不是整行放行。
#[test]
fn test_terminal_link_intent_opens_verified_bare_filename_only() {
    let mut grid = TerminalGrid::new(80, 24);
    grid.advance(b"see lib.rs now");
    let snapshot = grid.snapshot();

    let normal = terminal_link_intent(&snapshot, TextPoint::new(0, 5), false, 1, &verified);
    let command = terminal_link_intent(&snapshot, TextPoint::new(0, 5), true, 1, &verified);
    let missing = terminal_link_intent(&snapshot, TextPoint::new(0, 5), false, 1, &unverified);
    let elsewhere = terminal_link_intent(&snapshot, TextPoint::new(0, 2), false, 1, &verified);

    assert_eq!(
        normal,
        TerminalLinkIntent::OpenPath {
            paths: vec!["lib.rs".to_string()],
            system: false
        },
        "缓存确认存在的裸文件名画了下划线，点击必须能打开"
    );
    assert_eq!(
        command,
        TerminalLinkIntent::OpenPath {
            paths: vec!["lib.rs".to_string()],
            system: true
        },
        "⌘ 点击已确认的裸文件名应请求系统应用"
    );
    assert_eq!(
        missing,
        TerminalLinkIntent::BeginTextSelection,
        "未确认存在的裸文件名不得变成链接"
    );
    assert_eq!(
        elsewhere,
        TerminalLinkIntent::BeginTextSelection,
        "点在路径之外的单词上不得因为同一行有已验证路径而被放行"
    );
}

/// 含中文的路径：下划线覆盖整段，点击也必须命中整段。
///
/// 数据构造：
///   grid = 80 cols × 24 lines；row 0 = "docs/文档.md"。
///   "docs/" 占 5 格；"文""档" 各占 2 格（宽字符 + 占位格）共 4 格；".md" 占 3 格 → 列 0..12。
///   alacritty 的宽字符占位格 `c` 是空格，旧的点击切词会在占位格处断开。
///   点击列 9（".md" 内，位于中文之后）。
///
/// 预期结果：
///   - 点击得到 OpenPath{paths=["docs/文档.md"]}，不是 BeginTextSelection。
///   - 下划线 span 覆盖列 0..12，与可点击范围一致。
#[test]
fn test_terminal_link_intent_opens_path_containing_wide_characters() {
    let text = "docs/文档.md";
    let mut grid = TerminalGrid::new(80, 24);
    grid.advance(text.as_bytes());
    let snapshot = grid.snapshot();

    let intent = terminal_link_intent(&snapshot, TextPoint::new(0, 9), false, 1, &unverified);
    let spans = snapshot.path_ref_spans_for_row(0);

    assert_eq!(
        intent,
        TerminalLinkIntent::OpenPath {
            paths: vec![text.to_string()],
            system: false
        },
        "含宽字符的强候选路径画了下划线，点击必须能打开"
    );
    assert_eq!(spans.len(), 1, "整行只有一个路径 span");
    assert_eq!(
        (spans[0].start, spans[0].end),
        (0, 12),
        "下划线应覆盖含占位格的整段 12 列"
    );
}
