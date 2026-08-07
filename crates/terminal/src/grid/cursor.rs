use super::*;

impl TerminalGrid {
    pub(super) fn render_cursor_position(&self) -> (usize, usize) {
        let grid = self.term.grid();
        let cursor = grid.cursor.point;
        let max_line = self.dims.screen_lines.saturating_sub(1) as i32;
        let mut cursor_line = cursor.line.0.clamp(0, max_line) as usize;
        let mut cursor_col = cursor.column.0.min(self.dims.columns.saturating_sub(1));

        if self.dims.screen_lines == 0 || self.dims.columns == 0 {
            cursor_line = 0;
            cursor_col = 0;
        } else if cursor_col > 0 {
            let point = Point::new(Line(cursor_line as i32), Column(cursor_col));
            if grid[point].flags.contains(Flags::WIDE_CHAR_SPACER) {
                cursor_col -= 1;
            }
        }

        (cursor_line, cursor_col)
    }
}
