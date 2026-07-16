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
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{
    Color as AlacrittyColor, NamedColor as AlacrittyNamedColor, Processor, Rgb as AlacrittyRgb,
};

mod links;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub attrs: CellAttrs,
    /// OSC 8 目标 URI；`None` 表示这个字符不是语义链接的一部分。
    pub hyperlink: Option<TerminalHyperlink>,
}

/// 与渲染库解耦的终端链接目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalHyperlink {
    uri: String,
}

impl TerminalHyperlink {
    fn from_alacritty(link: alacritty_terminal::term::cell::Hyperlink) -> Self {
        Self {
            uri: link.uri().to_string(),
        }
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }
}

/// 当前屏幕的中性快照，供 UI 渲染使用。
#[derive(Debug, Clone)]
pub struct GridSnapshot {
    pub rows: Vec<Vec<Cell>>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub cols: usize,
    pub lines: usize,
    pub display_offset: usize,
    pub history_size: usize,
    pub alt_screen: bool,
    pub alternate_scroll: bool,
    pub mouse_mode: bool,
    pub sgr_mouse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalWheelAction {
    Scrollback(i32),
    WriteBytes(Vec<u8>),
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
        let display_offset = grid.display_offset();
        let mut rows = vec![
            vec![
                Cell {
                    c: ' ',
                    fg: TerminalColor::Named(TerminalNamedColor::Foreground),
                    bg: TerminalColor::Named(TerminalNamedColor::Background),
                    attrs: CellAttrs::default(),
                    hyperlink: None,
                };
                self.dims.columns
            ];
            self.dims.screen_lines
        ];

        for (line, row) in rows.iter_mut().enumerate().take(self.dims.screen_lines) {
            for (col, output_cell) in row.iter_mut().enumerate().take(self.dims.columns) {
                let point = Point::new(Line(line as i32 - display_offset as i32), Column(col));
                let cell = &grid[point];
                *output_cell = Cell {
                    c: cell.c,
                    fg: terminal_color(cell.fg),
                    bg: terminal_color(cell.bg),
                    attrs: cell_attrs(cell.flags),
                    hyperlink: cell.hyperlink().map(TerminalHyperlink::from_alacritty),
                };
            }
        }

        let (cursor_line, cursor_col) = self.render_cursor_position();
        let mode = self.term.mode();
        GridSnapshot {
            rows,
            cursor_line,
            cursor_col,
            cols: self.dims.columns,
            lines: self.dims.screen_lines,
            display_offset,
            history_size: grid.history_size(),
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
            alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
            mouse_mode: mode.intersects(TermMode::MOUSE_MODE),
            sgr_mouse: mode.contains(TermMode::SGR_MOUSE),
        }
    }
}

pub fn terminal_wheel_action(
    snapshot: &GridSnapshot,
    lines: i32,
    row: u16,
    col: u16,
) -> TerminalWheelAction {
    if lines == 0 {
        return TerminalWheelAction::Scrollback(0);
    }
    if snapshot.alt_screen && snapshot.mouse_mode && snapshot.sgr_mouse {
        return TerminalWheelAction::WriteBytes(sgr_mouse_wheel_bytes(lines, row, col));
    }
    if snapshot.alt_screen && snapshot.alternate_scroll && !snapshot.mouse_mode {
        return TerminalWheelAction::WriteBytes(alternate_scroll_key_bytes(lines));
    }
    TerminalWheelAction::Scrollback(lines)
}

fn sgr_mouse_wheel_bytes(lines: i32, row: u16, col: u16) -> Vec<u8> {
    let button = if lines < 0 { 65 } else { 64 };
    let x = u32::from(col) + 1;
    let y = u32::from(row) + 1;
    let event = format!("\x1b[<{button};{x};{y}M");
    repeat_bytes(event.as_bytes(), lines)
}

fn alternate_scroll_key_bytes(lines: i32) -> Vec<u8> {
    let key = if lines < 0 {
        b"\x1b[B".as_slice()
    } else {
        b"\x1b[A".as_slice()
    };
    repeat_bytes(key, lines)
}

fn repeat_bytes(bytes: &[u8], lines: i32) -> Vec<u8> {
    let count = lines.unsigned_abs() as usize;
    let mut out = Vec::with_capacity(bytes.len() * count);
    for _ in 0..count {
        out.extend_from_slice(bytes);
    }
    out
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
#[path = "grid/basic.rs"]
mod basic;

#[cfg(test)]
#[path = "grid/wheel.rs"]
mod wheel;

#[path = "grid/scrollback.rs"]
mod scrollback;

#[path = "grid/cursor.rs"]
mod cursor;
