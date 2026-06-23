use gpui::{px, rgb, rgba, Hsla, Pixels};
use terminal::{Cell, TerminalColor, TerminalNamedColor};
use ui::Theme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerminalPalette {
    pub(super) ansi_colors: [Hsla; 16],
    pub(super) extended_colors: [Hsla; 256],
    pub(super) foreground: Hsla,
    pub(super) background: Hsla,
    pub(super) cursor: Hsla,
}

impl TerminalPalette {
    pub(super) fn from_theme(theme: Theme) -> Self {
        let ansi_colors = [
            rgb_to_hsla(0x00, 0x00, 0x00),
            rgb_to_hsla(0xcc, 0x00, 0x00),
            rgb_to_hsla(0x4e, 0x9a, 0x06),
            rgb_to_hsla(0xc4, 0xa0, 0x00),
            rgb_to_hsla(0x34, 0x65, 0xa4),
            rgb_to_hsla(0x75, 0x50, 0x7b),
            rgb_to_hsla(0x06, 0x98, 0x9a),
            rgb_to_hsla(0xd3, 0xd7, 0xcf),
            rgb_to_hsla(0x55, 0x57, 0x53),
            rgb_to_hsla(0xef, 0x29, 0x29),
            rgb_to_hsla(0x8a, 0xe2, 0x34),
            rgb_to_hsla(0xfc, 0xe9, 0x4f),
            rgb_to_hsla(0x72, 0x9f, 0xcf),
            rgb_to_hsla(0xad, 0x7f, 0xa8),
            rgb_to_hsla(0x34, 0xe2, 0xe2),
            rgb_to_hsla(0xee, 0xee, 0xec),
        ];

        let mut extended_colors = [Hsla::default(); 256];
        extended_colors[..16].copy_from_slice(&ansi_colors);

        let mut index = 16;
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    extended_colors[index] = rgb_to_hsla(
                        if r == 0 { 0 } else { 55 + r * 40 },
                        if g == 0 { 0 } else { 55 + g * 40 },
                        if b == 0 { 0 } else { 55 + b * 40 },
                    );
                    index += 1;
                }
            }
        }

        for i in 0..24 {
            let gray = (8 + i * 10) as u8;
            extended_colors[232 + i] = rgb_to_hsla(gray, gray, gray);
        }

        Self {
            ansi_colors,
            extended_colors,
            foreground: rgba(theme.term_fg).into(),
            background: rgba(theme.term_bg).into(),
            cursor: rgba(theme.term_fg).into(),
        }
    }

    pub(super) fn resolve(self, color: TerminalColor) -> Hsla {
        match color {
            TerminalColor::Named(named) => self.resolve_named(named),
            TerminalColor::Indexed(index) => self.extended_colors[usize::from(index)],
            TerminalColor::Rgb { r, g, b } => rgb_to_hsla(r, g, b),
        }
    }

    fn resolve_named(self, named: TerminalNamedColor) -> Hsla {
        match named {
            TerminalNamedColor::Black => self.ansi_colors[0],
            TerminalNamedColor::Red => self.ansi_colors[1],
            TerminalNamedColor::Green => self.ansi_colors[2],
            TerminalNamedColor::Yellow => self.ansi_colors[3],
            TerminalNamedColor::Blue => self.ansi_colors[4],
            TerminalNamedColor::Magenta => self.ansi_colors[5],
            TerminalNamedColor::Cyan => self.ansi_colors[6],
            TerminalNamedColor::White => self.ansi_colors[7],
            TerminalNamedColor::BrightBlack => self.ansi_colors[8],
            TerminalNamedColor::BrightRed => self.ansi_colors[9],
            TerminalNamedColor::BrightGreen => self.ansi_colors[10],
            TerminalNamedColor::BrightYellow => self.ansi_colors[11],
            TerminalNamedColor::BrightBlue => self.ansi_colors[12],
            TerminalNamedColor::BrightMagenta => self.ansi_colors[13],
            TerminalNamedColor::BrightCyan => self.ansi_colors[14],
            TerminalNamedColor::BrightWhite => self.ansi_colors[15],
            TerminalNamedColor::Foreground => self.foreground,
            TerminalNamedColor::Background => self.background,
            TerminalNamedColor::Cursor => self.cursor,
            TerminalNamedColor::DimBlack => dim_color(self.ansi_colors[0]),
            TerminalNamedColor::DimRed => dim_color(self.ansi_colors[1]),
            TerminalNamedColor::DimGreen => dim_color(self.ansi_colors[2]),
            TerminalNamedColor::DimYellow => dim_color(self.ansi_colors[3]),
            TerminalNamedColor::DimBlue => dim_color(self.ansi_colors[4]),
            TerminalNamedColor::DimMagenta => dim_color(self.ansi_colors[5]),
            TerminalNamedColor::DimCyan => dim_color(self.ansi_colors[6]),
            TerminalNamedColor::DimWhite => dim_color(self.ansi_colors[7]),
            TerminalNamedColor::BrightForeground => brighten_color(self.foreground),
            TerminalNamedColor::DimForeground => dim_color(self.foreground),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerminalGlassStyle {
    pub(super) cursor_fill: Hsla,
    pub(super) cursor_stroke: Hsla,
    pub(super) cursor_highlight: Hsla,
    pub(super) selection_highlight: Hsla,
    pub(super) cursor_radius: Pixels,
    pub(super) cursor_stroke_width: Pixels,
    pub(super) cursor_highlight_height: Pixels,
}

impl TerminalGlassStyle {
    pub(super) fn from_theme(theme: Theme) -> Self {
        let accent: Hsla = rgba(theme.focused_stroke).into();
        let highlight: Hsla = rgba(theme.edge_highlight).into();
        Self {
            cursor_fill: hsla_with_alpha(accent, 0.42),
            cursor_stroke: hsla_with_alpha(accent, 0.86),
            cursor_highlight: hsla_with_alpha(highlight, 0.55),
            selection_highlight: hsla_with_alpha(accent, 0.28),
            cursor_radius: px(3.0),
            cursor_stroke_width: px(1.0),
            cursor_highlight_height: px(2.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ResolvedCellStyle {
    pub(super) fg: Hsla,
    pub(super) bg: Hsla,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strikeout: bool,
    pub(super) hidden: bool,
    pub(super) wide_char_spacer: bool,
}

impl ResolvedCellStyle {
    pub(super) fn text_style(self) -> TerminalTextStyle {
        TerminalTextStyle {
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strikeout: self.strikeout,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerminalTextStyle {
    pub(super) fg: Hsla,
    pub(super) bg: Hsla,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strikeout: bool,
}

pub(super) fn resolve_cell_style(cell: &Cell, palette: TerminalPalette) -> ResolvedCellStyle {
    let mut fg = palette.resolve(cell.fg);
    let mut bg = palette.resolve(cell.bg);

    if cell.attrs.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }

    if cell.attrs.dim {
        fg = dim_color(fg);
    }

    ResolvedCellStyle {
        fg,
        bg,
        bold: cell.attrs.bold,
        italic: cell.attrs.italic,
        underline: cell.attrs.underline,
        strikeout: cell.attrs.strikeout,
        hidden: cell.attrs.hidden,
        wide_char_spacer: cell.attrs.wide_char_spacer,
    }
}

fn rgb_to_hsla(r: u8, g: u8, b: u8) -> Hsla {
    rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)).into()
}

fn hsla_with_alpha(mut color: Hsla, alpha: f32) -> Hsla {
    color.a = alpha.clamp(0.0, 1.0);
    color
}

fn dim_color(mut color: Hsla) -> Hsla {
    color.l *= 0.7;
    color
}

fn brighten_color(mut color: Hsla) -> Hsla {
    color.l = (color.l * 1.2).min(1.0);
    color
}
