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
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;

/// 中性单元格（不含 alacritty 类型）。fg/bg 为 0xRRGGBB。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub fg: u32,
    pub bg: u32,
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
        let dims = TermDimensions { columns: cols as usize, screen_lines: lines as usize };
        let term = Term::new(Config::default(), &dims, VoidListener);
        TerminalGrid { term, processor: Processor::new(), dims }
    }

    /// 喂一批字节给 ANSI 解析器，更新 grid 状态。
    pub fn advance(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    /// 调整终端尺寸。
    pub fn resize(&mut self, cols: u16, lines: u16) {
        self.dims = TermDimensions { columns: cols as usize, screen_lines: lines as usize };
        self.term.resize(self.dims);
    }

    /// 默认前景/背景颜色（0xRRGGBB）。本切片使用固定主题色；完整主题留后续任务。
    fn default_colors() -> (u32, u32) {
        (0xd0d0d0, 0x1e1e1e) // fg 浅灰 / bg 深灰
    }

    /// 产出当前屏幕的中性快照。
    ///
    /// 索引说明：alacritty_terminal 0.26 中 Line(0) 为顶行，
    /// Line(screen_lines - 1) 为底行，与视觉顺序一致。
    pub fn snapshot(&self) -> GridSnapshot {
        let (def_fg, def_bg) = Self::default_colors();
        let grid = self.term.grid();
        let mut rows =
            vec![
                vec![Cell { c: ' ', fg: def_fg, bg: def_bg }; self.dims.columns];
                self.dims.screen_lines
            ];

        for line in 0..self.dims.screen_lines {
            for col in 0..self.dims.columns {
                let point = Point::new(Line(line as i32), Column(col));
                let cell = &grid[point];
                rows[line][col] = Cell { c: cell.c, fg: def_fg, bg: def_bg };
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
}
