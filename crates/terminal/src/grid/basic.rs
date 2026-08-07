use super::{TerminalColor, TerminalGrid, TerminalNamedColor};

/// 纯 ASCII 写入后，快照首行前两格为 'h'、'i'。
///
/// 数据构造：
///   grid 24 行 × 80 列；写入字节 b"hi"
///
/// 执行过程：
///   advance(b"hi") → ANSI 解析器把 'h'(列0) 'i'(列1) 写入第 0 行
///
/// 预期结果：
///   - 正断言：rows[0][0].c == 'h'，rows[0][1].c == 'i'
///   - 负断言：rows[0][2].c != 'h'（第三格不应是 'h'，证明没有错位/重复写入）
///   - 尺寸断言：cols == 80，lines == 24
#[test]
fn test_plain_ascii_lands_in_first_row() {
    let mut g = TerminalGrid::new(80, 24);
    g.advance(b"hi");
    let snap = g.snapshot();

    assert_eq!(snap.rows[0][0].c, 'h', "首行第 0 格应为 'h'");
    assert_eq!(snap.rows[0][1].c, 'i', "首行第 1 格应为 'i'");
    assert_ne!(snap.rows[0][2].c, 'h', "第 2 格不应为 'h'，否则说明列错位");
    assert_eq!(snap.cols, 80, "快照列数应等于初始化值 80");
    assert_eq!(snap.lines, 24, "快照行数应等于初始化值 24");
}

/// 含 ANSI 颜色转义的输入不崩溃，可见字符仍落盘。
///
/// 数据构造：b"\x1b[31mX\x1b[0m" —— 红色前景 'X' 再复位
///
/// 执行过程：
///   advance(b"\x1b[31mX\x1b[0m") → 转义序列被解析消费，'X' 写入第 0 列
///
/// 预期结果：
///   - 正断言：rows[0][0].c == 'X'（转义被解析消费，X 正常写入）
///   - 不崩溃（test 本身即验证 panic-free）
#[test]
fn test_ansi_escape_does_not_crash_and_char_lands() {
    let mut g = TerminalGrid::new(80, 24);
    g.advance(b"\x1b[31mX\x1b[0m");
    let snap = g.snapshot();
    assert_eq!(snap.rows[0][0].c, 'X', "ANSI 颜色转义后 'X' 应正常落在首格");
}

/// DSR 光标位置查询：TerminalGrid 必须按查询发生时的 1-based 坐标回写 CSI 响应。
///
/// 数据构造（含关键数值的推导过程）：
///   可见文本 = b"abc" = 3 个 ASCII 单元格，写入后 cursor 为 0-based (line=0, col=3)
///   查询序列 = ESC [ 6 n = [0x1b, b'[', b'6', b'n']，共 4 字节
///   终端协议坐标 = (line + 1, col + 1) = (0 + 1, 3 + 1) = (1, 4)
///   期望响应 = ESC [ 1 ; 4 R = b"\x1b[1;4R"，共 6 字节
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `advance(b"abc")` → ANSI parser 在第 0 行写入三格，光标移动到第 3 列
///   2. `advance(ESC[6n)` → parser 识别 Device Status Report 的 cursor-position 查询
///   3. TerminalGrid 从 parser event 取出 PTY 回写字节，供 TerminalSession 发回子进程
///   4. snapshot() → 保留 abc，查询控制序列不应变成可见文本
///
/// 预期结果：
///   - 断言 A：响应精确为 `ESC[1;4R`，使 Grok 等 TUI 能读取当前光标坐标
///   - 断言 B：响应不是原点 `ESC[1;1R`，避免错误地忽略已写入的三列
///   - 断言 C：首行仍含 abc，DSR 查询不得污染终端网格
#[test]
fn test_dsr_cursor_position_query_returns_current_cursor_to_pty() {
    let mut g = TerminalGrid::new(80, 24);
    let initial_reply = g.advance(b"abc");
    let reply = g.advance(b"\x1b[6n");
    let snap = g.snapshot();
    let first_row: String = snap.rows[0].iter().map(|cell| cell.c).collect();

    assert!(
        initial_reply.is_empty(),
        "普通可见文本不应触发 PTY 回写，实际: {:?}",
        initial_reply
    );
    assert_eq!(
        reply, b"\x1b[1;4R",
        "abc 后的 DSR 查询应回传 1-based 光标坐标 ESC[1;4R，实际: {:?}",
        reply
    );
    assert_ne!(
        reply, b"\x1b[1;1R",
        "DSR 响应不应错误地报告原点 ESC[1;1R，abc 已把光标右移三列"
    );
    assert!(
        first_row.starts_with("abc"),
        "DSR 查询不应污染已写入的 abc，实际首行: {:?}",
        first_row
    );
}

/// OSC 8 链接：解析器应把链接目标保留到中性快照，供 UI 按单元格坐标命中。
///
/// 数据构造（含关键数值的推导过程）：
///   grid = 80 cols × 24 lines
///   OSC 8 open  = ESC ] 8 ; ; https://example.com/docs ESC \\（目标 URI）
///   visible text = "docs"，占第 0 行 col 0..3 四个单元格
///   OSC 8 close = ESC ] 8 ; ; ESC \\（后续文本不应继承链接）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入 OSC 8 open、文本 "docs"、OSC 8 close 与普通字符 "!"
///   2. `TerminalGrid` 解析上游终端单元格的 hyperlink metadata
///   3. `snapshot.hyperlink_at` 分别查询链接文字与关闭后的普通字符
///
/// 预期结果：
///   - 断言 A："docs" 首格可查询到精确目标 URI，证明链接元数据未在快照边界丢失
///   - 断言 B：关闭后的 "!" 查询不到链接，证明目标不会泄漏到后续文本
#[test]
fn test_snapshot_preserves_osc8_hyperlink_target_by_cell_position() {
    let mut g = TerminalGrid::new(80, 24);
    g.advance(b"\x1b]8;;https://example.com/docs\x1b\\docs\x1b]8;;\x1b\\!");
    let snap = g.snapshot();

    assert_eq!(
        snap.hyperlink_at(0, 0),
        Some("https://example.com/docs"),
        "OSC 8 链接文字 docs 的首格应保留精确目标 URI"
    );
    assert_eq!(
        snap.hyperlink_at(0, 4),
        None,
        "OSC 8 close 后的普通字符 ! 不应继承前一个链接目标"
    );
}

/// 裸 http(s) URL 与 OSC 8 目标：快照只应返回经协议校验的安全网页链接。
///
/// 数据构造（含关键数值的推导过程）：
///   row 0 = "Read https://example.com/docs. file:///private"，共 46 个 ASCII cells
///   URL 起始 col = len("Read ") = 5
///   末尾句点 `.` 是 prose 标点，不属于 URI
///   `file:///private` 是非 http(s) scheme，按产品约定不得成为可点击目标
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 80 列 terminal 写入同一行可见文本，不提供 OSC 8 控制序列
///   2. 在 http URL 的首格与 file URL 的首格调用 `link_at`
///   3. 再输入一个目标为 `javascript:`、可见文字却是 https URL 的 OSC 8 链接，并查询首格
///
/// 预期结果：
///   - 断言 A：http URL 可识别且会去掉句末标点
///   - 断言 B：file URL 不可识别，避免打开本地文件或非网页 scheme
///   - 断言 C：OSC 8 的 javascript 目标也不可识别，避免控制序列绕过协议校验
#[test]
fn test_snapshot_detects_bare_https_url_and_rejects_unsafe_schemes() {
    let mut g = TerminalGrid::new(80, 24);
    g.advance(b"Read https://example.com/docs. file:///private");
    let snap = g.snapshot();

    assert_eq!(
        snap.link_at(0, 5).as_deref(),
        Some("https://example.com/docs"),
        "裸 https URL 应可点击且不应把句末句点纳入目标"
    );
    assert_eq!(
        snap.link_at(0, 31),
        None,
        "file:///private 不应成为可点击链接，避免非网页 scheme 被打开"
    );

    let mut osc_grid = TerminalGrid::new(80, 24);
    osc_grid.advance(b"\x1b]8;;javascript:alert(1)\x1b\\https://visible.example\x1b]8;;\x1b\\");
    let osc_snapshot = osc_grid.snapshot();
    assert_eq!(
        osc_snapshot.link_at(0, 0),
        None,
        "OSC 8 的 javascript 目标不应被可见 https 文本回退绕过协议校验"
    );
}

/// 本机 Markdown 路径：工作区内外的文件都应识别，非文件 URI scheme 不得识别。
///
/// 数据构造（含关键数值的推导过程）：
///   工作区内路径 = "docs/adr/0015-license-hard-gate-outside-workbench.md"，首格 col=0
///   工作区外路径 = "../shared/result.md"，首格 col=0，含一个 `..` 父目录分量
///   非文件 URI = "ssh://example.com/shared/result.md"，首格 col=0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别写入工作区内路径、工作区外路径和非文件 URI
///   2. 对三者首格调用 `preview_path_at`
///
/// 预期结果：
///   - 正断言：工作区内路径返回完整相对路径
///   - 正断言：`..` 路径也返回完整目标，允许只读预览工作区外文件
///   - 负断言：ssh URI 不得冒充本机 Preview 文件
#[test]
fn test_snapshot_detects_local_preview_paths_inside_and_outside_workspace() {
    let mut inside_grid = TerminalGrid::new(100, 24);
    inside_grid.advance(b"docs/adr/0015-license-hard-gate-outside-workbench.md");
    let inside = inside_grid.snapshot();
    let mut outside_grid = TerminalGrid::new(100, 24);
    outside_grid.advance(b"../shared/result.md");
    let outside = outside_grid.snapshot();
    let mut remote_grid = TerminalGrid::new(100, 24);
    remote_grid.advance(b"ssh://example.com/shared/result.md");
    let remote = remote_grid.snapshot();

    assert_eq!(
        inside.preview_path_at(0, 0).as_deref(),
        Some("docs/adr/0015-license-hard-gate-outside-workbench.md"),
        "workspace 内的相对 Markdown 路径应保留完整值，以便创建 Preview 卡片"
    );
    assert_eq!(
        outside.preview_path_at(0, 0).as_deref(),
        Some("../shared/result.md"),
        "含 .. 的路径应保留完整值，允许只读预览 workspace 外文件"
    );
    assert_eq!(
        remote.preview_path_at(0, 0),
        None,
        "ssh URI 不是本机文件路径，不得成为 Preview 候选"
    );
}

/// ANSI 前景色进入快照，reset 后回到默认前景色。
///
/// 数据构造：
///   grid 24 行 × 80 列
///   输入 b"\x1b[31mR\x1b[0mD"
///   31m = ANSI named red foreground；0m = reset 到 default foreground/background/attrs
///
/// 执行过程：
///   1. advance(31m) → 当前前景色变为 Red
///   2. 写入 'R' → rows[0][0] 应记录 Red
///   3. advance(0m) → 前景色恢复 Foreground
///   4. 写入 'D' → rows[0][1] 应记录 Foreground
///
/// 预期结果：
///   - 正断言：R 字符存在且 fg == Red
///   - 正断言：D 字符存在且 fg == Foreground
///   - 负断言：D 不应继续携带 Red，否则 reset 样式丢失
#[test]
fn test_snapshot_preserves_ansi_foreground_and_reset() {
    let mut g = TerminalGrid::new(80, 24);
    g.advance(b"\x1b[31mR\x1b[0mD");
    let snap = g.snapshot();

    assert_eq!(snap.rows[0][0].c, 'R', "第 0 格应是红色样式字符 'R'");
    assert_eq!(
        snap.rows[0][0].fg,
        TerminalColor::Named(TerminalNamedColor::Red),
        "31m 应在快照中保留为 ANSI Red 前景色"
    );
    assert_eq!(snap.rows[0][1].c, 'D', "第 1 格应是 reset 后字符 'D'");
    assert_eq!(
        snap.rows[0][1].fg,
        TerminalColor::Named(TerminalNamedColor::Foreground),
        "0m 后前景色应恢复为默认 Foreground"
    );
    assert_ne!(
        snap.rows[0][1].fg,
        TerminalColor::Named(TerminalNamedColor::Red),
        "reset 后的 'D' 不应继续保留 Red 前景色"
    );
}

/// 24-bit true color 前景色进入快照。
///
/// 数据构造：
///   grid 24 行 × 80 列
///   输入 b"\x1b[38;2;1;2;3mT"
///   38;2;r;g;b = true color foreground，r=1、g=2、b=3
///
/// 执行过程：
///   1. advance(true color escape) → 当前前景色变为 RGB(1,2,3)
///   2. 写入 'T' → rows[0][0] 应记录 RGB(1,2,3)
///
/// 预期结果：
///   - 正断言：T 字符存在且 fg == Rgb{1,2,3}
///   - 负断言：T 不应退化为默认 Foreground
#[test]
fn test_snapshot_preserves_truecolor_foreground() {
    let mut g = TerminalGrid::new(80, 24);
    g.advance(b"\x1b[38;2;1;2;3mT");
    let snap = g.snapshot();

    assert_eq!(snap.rows[0][0].c, 'T', "第 0 格应是 truecolor 字符 'T'");
    assert_eq!(
        snap.rows[0][0].fg,
        TerminalColor::Rgb { r: 1, g: 2, b: 3 },
        "38;2;1;2;3m 应保留为 RGB(1,2,3) 前景色"
    );
    assert_ne!(
        snap.rows[0][0].fg,
        TerminalColor::Named(TerminalNamedColor::Foreground),
        "truecolor 前景色不应退化为默认 Foreground"
    );
}

/// 256 色背景色进入快照。
///
/// 数据构造：
///   grid 24 行 × 80 列
///   输入 b"\x1b[48;5;22mB"
///   48;5;N = indexed background，N=22
///
/// 执行过程：
///   1. advance(indexed background escape) → 当前背景色变为 Indexed(22)
///   2. 写入 'B' → rows[0][0] 应记录 Indexed(22)
///
/// 预期结果：
///   - 正断言：B 字符存在且 bg == Indexed(22)
///   - 负断言：B 不应退化为默认 Background
#[test]
fn test_snapshot_preserves_indexed_background() {
    let mut g = TerminalGrid::new(80, 24);
    g.advance(b"\x1b[48;5;22mB");
    let snap = g.snapshot();

    assert_eq!(
        snap.rows[0][0].c, 'B',
        "第 0 格应是 indexed background 字符 'B'"
    );
    assert_eq!(
        snap.rows[0][0].bg,
        TerminalColor::Indexed(22),
        "48;5;22m 应保留为 Indexed(22) 背景色"
    );
    assert_ne!(
        snap.rows[0][0].bg,
        TerminalColor::Named(TerminalNamedColor::Background),
        "indexed 背景色不应退化为默认 Background"
    );
}

/// SGR 文本属性进入快照，reset 后属性清空。
///
/// 数据构造：
///   grid 24 行 × 80 列
///   输入 b"\x1b[1;3;4;9mS\x1b[0mN"
///   1=bold，3=italic，4=underline，9=strikeout，0=reset
///
/// 执行过程：
///   1. advance(1;3;4;9m) → 当前属性打开 bold/italic/underline/strikeout
///   2. 写入 'S' → rows[0][0] 应携带四个属性
///   3. advance(0m) → 属性清空
///   4. 写入 'N' → rows[0][1] 不应携带这些属性
///
/// 预期结果：
///   - 正断言：S 保留 bold/italic/underline/strikeout
///   - 正断言：N 字符存在
///   - 负断言：N 不应保留 bold/italic/underline/strikeout
#[test]
fn test_snapshot_preserves_text_attrs_and_reset() {
    let mut g = TerminalGrid::new(80, 24);
    g.advance(b"\x1b[1;3;4;9mS\x1b[0mN");
    let snap = g.snapshot();

    let styled = &snap.rows[0][0];
    let reset = &snap.rows[0][1];

    assert_eq!(styled.c, 'S', "第 0 格应是带属性字符 'S'");
    assert!(styled.attrs.bold, "'S' 应保留 bold 属性");
    assert!(styled.attrs.italic, "'S' 应保留 italic 属性");
    assert!(styled.attrs.underline, "'S' 应保留 underline 属性");
    assert!(styled.attrs.strikeout, "'S' 应保留 strikeout 属性");
    assert_eq!(reset.c, 'N', "第 1 格应是 reset 后字符 'N'");
    assert!(!reset.attrs.bold, "reset 后 'N' 不应保留 bold 属性");
    assert!(!reset.attrs.italic, "reset 后 'N' 不应保留 italic 属性");
    assert!(
        !reset.attrs.underline,
        "reset 后 'N' 不应保留 underline 属性"
    );
    assert!(
        !reset.attrs.strikeout,
        "reset 后 'N' 不应保留 strikeout 属性"
    );
}

/// inverse 和 hidden flags 进入快照，供 UI 层决定如何绘制。
///
/// 数据构造：
///   grid 24 行 × 80 列
///   输入 b"\x1b[7mI\x1b[0m\x1b[8mH"
///   7=inverse，8=hidden，0=reset
///
/// 执行过程：
///   1. advance(7m) → 当前属性打开 inverse
///   2. 写入 'I' → rows[0][0].attrs.inverse 应为 true
///   3. reset 后 advance(8m) → 当前属性打开 hidden
///   4. 写入 'H' → rows[0][1].attrs.hidden 应为 true
///
/// 预期结果：
///   - 正断言：I 保留 inverse
///   - 正断言：H 保留 hidden
///   - 负断言：H 不应误带 inverse，证明 reset 生效
#[test]
fn test_snapshot_preserves_inverse_and_hidden_attrs() {
    let mut g = TerminalGrid::new(80, 24);
    g.advance(b"\x1b[7mI\x1b[0m\x1b[8mH");
    let snap = g.snapshot();

    assert_eq!(snap.rows[0][0].c, 'I', "第 0 格应是 inverse 字符 'I'");
    assert!(snap.rows[0][0].attrs.inverse, "'I' 应保留 inverse 属性");
    assert_eq!(snap.rows[0][1].c, 'H', "第 1 格应是 hidden 字符 'H'");
    assert!(snap.rows[0][1].attrs.hidden, "'H' 应保留 hidden 属性");
    assert!(
        !snap.rows[0][1].attrs.inverse,
        "reset 后的 hidden 字符不应误带 inverse 属性"
    );
}
