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

    let styled = snap.rows[0][0];
    let reset = snap.rows[0][1];

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
