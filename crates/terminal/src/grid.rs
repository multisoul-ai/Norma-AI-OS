//! alacritty_terminal 封装：把 PTY 字节解析为 grid，并产出中性快照供 UI 渲染。
//!
//! API 适配说明（alacritty_terminal =0.26.0）：
//! - `Config` 路径：`alacritty_terminal::term::Config`（有 `Default` 实现）
//! - `Dimensions` 路径：`alacritty_terminal::grid::Dimensions`
//! - `Processor` 路径：`alacritty_terminal::vte::ansi::Processor`
//! - `advance` 签名：`processor.advance(&mut handler, bytes: &[u8])`
//! - 网格索引：`grid[Point::new(Line(i32), Column(usize))]`
//! - Cell 字段：`cell.c: char`（确认为 `.c`，非 `.character`）
//! - Cursor：通过 `term.grid().cursor.point` 获取
//! - 事件监听器：直接使用 `alacritty_terminal::event::VoidListener`（库内置）

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{
    Color as AlacrittyColor, NamedColor as AlacrittyNamedColor, Processor, Rgb as AlacrittyRgb,
};

/// 中性终端颜色（不含 alacritty 类型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColor {
    Named(TerminalNamedColor),
    Indexed(u8),
    Rgb { r: u8, g: u8, b: u8 },
}

/// 中性命名颜色，覆盖 ANSI 16 色、默认前景/背景/光标与 dim/bright 默认色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalNamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Foreground,
    Background,
    Cursor,
    DimBlack,
    DimRed,
    DimGreen,
    DimYellow,
    DimBlue,
    DimMagenta,
    DimCyan,
    DimWhite,
    BrightForeground,
    DimForeground,
}

/// 中性单元格属性（不含 alacritty 类型）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellAttrs {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub hidden: bool,
    pub strikeout: bool,
    pub wide_char_spacer: bool,
}

/// 中性单元格（不含 alacritty 类型）。fg/bg 保留终端颜色语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub attrs: CellAttrs,
}

/// 当前屏幕的中性快照，供 UI 渲染使用。
#[derive(Debug, Clone)]
pub struct GridSnapshot {
    pub rows: Vec<Vec<Cell>>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub cols: usize,
    pub lines: usize,
}

/// 实现 alacritty 的 Dimensions：仅可见屏幕（本切片不做 scrollback 虚拟化）。
#[derive(Clone, Copy)]
struct TermDimensions {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }
    fn screen_lines(&self) -> usize {
        self.screen_lines
    }
    fn columns(&self) -> usize {
        self.columns
    }
}

/// alacritty_terminal 封装：持有 Term + ANSI Processor，支持字节输入和屏幕快照。
pub struct TerminalGrid {
    term: Term<VoidListener>,
    processor: Processor,
    dims: TermDimensions,
}

impl TerminalGrid {
    /// 创建指定尺寸的终端网格。
    pub fn new(cols: u16, lines: u16) -> TerminalGrid {
        let dims = TermDimensions {
            columns: cols as usize,
            screen_lines: lines as usize,
        };
        let term = Term::new(Config::default(), &dims, VoidListener);
        TerminalGrid {
            term,
            processor: Processor::new(),
            dims,
        }
    }

    /// 喂一批字节给 ANSI 解析器，更新 grid 状态。
    pub fn advance(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    /// 调整终端尺寸。
    pub fn resize(&mut self, cols: u16, lines: u16) {
        self.dims = TermDimensions {
            columns: cols as usize,
            screen_lines: lines as usize,
        };
        self.term.resize(self.dims);
    }

    /// 产出当前屏幕的中性快照。
    ///
    /// 索引说明：alacritty_terminal 0.26 中 Line(0) 为顶行，
    /// Line(screen_lines - 1) 为底行，与视觉顺序一致。
    pub fn snapshot(&self) -> GridSnapshot {
        let grid = self.term.grid();
        let mut rows = vec![
            vec![
                Cell {
                    c: ' ',
                    fg: TerminalColor::Named(TerminalNamedColor::Foreground),
                    bg: TerminalColor::Named(TerminalNamedColor::Background),
                    attrs: CellAttrs::default(),
                };
                self.dims.columns
            ];
            self.dims.screen_lines
        ];

        for (line, row) in rows.iter_mut().enumerate().take(self.dims.screen_lines) {
            for (col, output_cell) in row.iter_mut().enumerate().take(self.dims.columns) {
                let point = Point::new(Line(line as i32), Column(col));
                let cell = &grid[point];
                *output_cell = Cell {
                    c: cell.c,
                    fg: terminal_color(cell.fg),
                    bg: terminal_color(cell.bg),
                    attrs: cell_attrs(cell.flags),
                };
            }
        }

        let cursor = self.term.grid().cursor.point;
        GridSnapshot {
            rows,
            cursor_line: cursor.line.0.max(0) as usize,
            cursor_col: cursor.column.0,
            cols: self.dims.columns,
            lines: self.dims.screen_lines,
        }
    }
}

fn terminal_color(color: AlacrittyColor) -> TerminalColor {
    match color {
        AlacrittyColor::Named(named) => TerminalColor::Named(terminal_named_color(named)),
        AlacrittyColor::Indexed(index) => TerminalColor::Indexed(index),
        AlacrittyColor::Spec(AlacrittyRgb { r, g, b }) => TerminalColor::Rgb { r, g, b },
    }
}

fn terminal_named_color(color: AlacrittyNamedColor) -> TerminalNamedColor {
    match color {
        AlacrittyNamedColor::Black => TerminalNamedColor::Black,
        AlacrittyNamedColor::Red => TerminalNamedColor::Red,
        AlacrittyNamedColor::Green => TerminalNamedColor::Green,
        AlacrittyNamedColor::Yellow => TerminalNamedColor::Yellow,
        AlacrittyNamedColor::Blue => TerminalNamedColor::Blue,
        AlacrittyNamedColor::Magenta => TerminalNamedColor::Magenta,
        AlacrittyNamedColor::Cyan => TerminalNamedColor::Cyan,
        AlacrittyNamedColor::White => TerminalNamedColor::White,
        AlacrittyNamedColor::BrightBlack => TerminalNamedColor::BrightBlack,
        AlacrittyNamedColor::BrightRed => TerminalNamedColor::BrightRed,
        AlacrittyNamedColor::BrightGreen => TerminalNamedColor::BrightGreen,
        AlacrittyNamedColor::BrightYellow => TerminalNamedColor::BrightYellow,
        AlacrittyNamedColor::BrightBlue => TerminalNamedColor::BrightBlue,
        AlacrittyNamedColor::BrightMagenta => TerminalNamedColor::BrightMagenta,
        AlacrittyNamedColor::BrightCyan => TerminalNamedColor::BrightCyan,
        AlacrittyNamedColor::BrightWhite => TerminalNamedColor::BrightWhite,
        AlacrittyNamedColor::Foreground => TerminalNamedColor::Foreground,
        AlacrittyNamedColor::Background => TerminalNamedColor::Background,
        AlacrittyNamedColor::Cursor => TerminalNamedColor::Cursor,
        AlacrittyNamedColor::DimBlack => TerminalNamedColor::DimBlack,
        AlacrittyNamedColor::DimRed => TerminalNamedColor::DimRed,
        AlacrittyNamedColor::DimGreen => TerminalNamedColor::DimGreen,
        AlacrittyNamedColor::DimYellow => TerminalNamedColor::DimYellow,
        AlacrittyNamedColor::DimBlue => TerminalNamedColor::DimBlue,
        AlacrittyNamedColor::DimMagenta => TerminalNamedColor::DimMagenta,
        AlacrittyNamedColor::DimCyan => TerminalNamedColor::DimCyan,
        AlacrittyNamedColor::DimWhite => TerminalNamedColor::DimWhite,
        AlacrittyNamedColor::BrightForeground => TerminalNamedColor::BrightForeground,
        AlacrittyNamedColor::DimForeground => TerminalNamedColor::DimForeground,
    }
}

fn cell_attrs(flags: Flags) -> CellAttrs {
    CellAttrs {
        bold: flags.contains(Flags::BOLD),
        dim: flags.contains(Flags::DIM),
        italic: flags.contains(Flags::ITALIC),
        underline: flags.intersects(Flags::ALL_UNDERLINES),
        inverse: flags.contains(Flags::INVERSE),
        hidden: flags.contains(Flags::HIDDEN),
        strikeout: flags.contains(Flags::STRIKEOUT),
        wide_char_spacer: flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
