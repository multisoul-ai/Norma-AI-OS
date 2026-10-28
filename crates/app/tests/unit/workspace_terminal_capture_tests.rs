use super::*;

fn capture_grid(grid: &terminal::TerminalGrid) -> Capture {
    let snapshot = grid.snapshot();
    let total = snapshot.history_size + snapshot.lines;
    let theme = ui::Theme::paper_blue();
    Capture {
        frame: WorkspaceTerminalFrame {
            version: String::new(),
            cols: snapshot.cols,
            screen_lines: snapshot.lines,
            start_line: 0,
            total_lines: total,
            running: true,
            alt_screen: snapshot.alt_screen,
            cursor: Some(TerminalCursor {
                line: snapshot.history_size + snapshot.cursor_line,
                column: snapshot.cursor_col,
            }),
            background: theme.term_bg >> 8,
            foreground: theme.term_fg >> 8,
            rows: Vec::new(),
        },
        live_rows: grid.rows_for_absolute(0, total - 1),
        history_rows: None,
        before: None,
        anchor_rows: Vec::new(),
        boundary_rows: Vec::new(),
        palette: TerminalPalette::from_theme(theme),
        history_version: None,
    }
}
fn live_at(grid: &terminal::TerminalGrid, start: usize) -> Capture {
    let mut capture = capture_grid(grid);
    capture.frame.start_line = start;
    capture.live_rows = grid.rows_for_absolute(start, capture.frame.total_lines - 1);
    capture.anchor_rows = grid.rows_for_absolute(start.saturating_sub(8), start - 1);
    capture
}
fn history_before(grid: &terminal::TerminalGrid, before: usize, version: String) -> Capture {
    let mut capture = capture_grid(grid);
    capture.before = Some(before);
    capture.history_version = Some(version);
    capture.history_rows = Some(grid.rows_for_absolute(before.saturating_sub(120), before - 1));
    capture.boundary_rows = grid.rows_for_absolute(before.saturating_sub(8), before - 1);
    capture
}

/// 场景：Mac ANSI网格的颜色、中文宽字、光标和原位进度保留，读取不改变Mac尺寸或滚动。
/// 数据构造：8列×3行；红色中文占2格，回车覆盖进度10%→20%，光标只移动一列。
/// 执行过程：解析Mac输出 → 两次后台编码 → 只改光标 → 验证版本与原网格状态。
/// 预期结果：红颜色及spacer保留；原位覆盖没有新增行；光标变化改变版本；Mac状态不被读取改写。
#[test]
fn terminal_capture_preserves_color_wide_progress_and_cursor_without_mutation() {
    let mut grid = terminal::TerminalGrid::new(8, 3);
    grid.advance("\x1b[31m中\x1b[0m\r\n10%".as_bytes());
    let first: WorkspaceTerminalFrame =
        serde_json::from_value(encode(capture_grid(&grid)).expect("首帧")).expect("网格");
    assert_eq!(first.rows[0][0].text, "中", "宽字必须仍在原格");
    // GPUI HSLA 往返最多有 1/255 量化误差；Paper Blue 新 ANSI red = #a62636。
    for shift in [16, 8, 0] {
        let actual = ((first.rows[0][0].fg >> shift) & 255) as i32;
        let expected = ((0xa62636_u32 >> shift) & 255) as i32;
        assert!(
            (actual - expected).abs() <= 1,
            "手机应复用 Mac 浅色 ANSI 红色，channel={shift}"
        );
    }
    assert_ne!(
        first.rows[0][0].fg, 0xcc0000,
        "浅色终端不能仍编码旧暗色 ANSI 红色"
    );
    assert!(first.rows[0][1].flags & 32 != 0, "右半格应是spacer");
    grid.advance(b"\r20%");
    let progress: WorkspaceTerminalFrame =
        serde_json::from_value(encode(capture_grid(&grid)).expect("进度帧")).expect("网格");
    assert_eq!(progress.rows[1][0].text, "2", "CR进度必须原位覆盖10%");
    assert_eq!(
        progress.total_lines, first.total_lines,
        "CR不得制造额外历史行"
    );
    assert_ne!(progress.version, first.version, "原位内容改变必须更新版本");
    grid.advance(b"\x1b[2;5H");
    let cursor: WorkspaceTerminalFrame =
        serde_json::from_value(encode(capture_grid(&grid)).expect("光标帧")).expect("网格");
    assert_ne!(cursor.version, progress.version, "仅光标变化也必须刷新");
    assert_eq!(
        cursor.cursor.expect("可见光标").column,
        4,
        "光标列应保持Mac定位"
    );
    assert_eq!(grid.snapshot().cols, 8, "读取不能resize Mac");
    assert_eq!(grid.snapshot().display_offset, 0, "读取不能改变Mac滚动位置");
}

/// 场景：历史读取只返回before之前的页，旧版本拒绝；备用屏状态参与版本。
/// 数据构造：8列×3行写5行，history=5-3=2；before=2读取前两行。
/// 执行过程：捕获实时页 → 绑定版本读历史 → 篡改版本 → 切换备用屏。
/// 预期结果：历史为A/B不含当前屏C；没有历史光标；旧版本拒绝；备用屏明确显示。
#[test]
fn terminal_capture_history_and_alternate_screen_are_scoped() {
    let mut grid = terminal::TerminalGrid::new(8, 3);
    grid.advance(b"A\r\nB\r\nC\r\nD\r\nE");
    let live: WorkspaceTerminalFrame =
        serde_json::from_value(encode(live_at(&grid, 2)).expect("实时页")).expect("网格");
    let history = history_before(&grid, 2, live.version.clone());
    let page: WorkspaceTerminalFrame =
        serde_json::from_value(encode(history).expect("历史页")).expect("网格");
    assert_eq!(page.rows[0][0].text, "A", "最早行必须可分页读取");
    assert_eq!(page.rows[1][0].text, "B", "历史页末行必须在before之前");
    assert_ne!(page.rows[0][0].text, "C", "不得重复当前屏顶行");
    assert!(page.cursor.is_none(), "历史页没有实时光标");
    let stale = history_before(&grid, 2, "old-version".into());
    assert_eq!(
        encode(stale).expect_err("过期历史应拒绝"),
        "terminal_changed",
        "历史不能混入不同版本"
    );
    grid.advance(b"\x1b[?1049h");
    let alt: WorkspaceTerminalFrame =
        serde_json::from_value(encode(capture_grid(&grid)).expect("备用屏")).expect("网格");
    assert!(alt.alt_screen, "备用屏必须明确编码");
    assert_ne!(alt.version, live.version, "屏幕切换必须更新版本");
}

/// 场景：持续原位进度和追加输出不能让已读历史页无法翻页。
/// 数据构造：8列×3行写5行，旧历史边界before=2，对应A/B；新增F后总行数5+1=6。
/// 执行过程：读取实时页 → 同行E改为20% → 追加F → 使用原历史边界请求A/B。
/// 预期结果：A/B完整保留；不混入C或新进度；读取不改变Mac滚动。
#[test]
fn terminal_capture_history_survives_progress_and_appended_output() {
    let mut grid = terminal::TerminalGrid::new(8, 3);
    grid.advance(b"A\r\nB\r\nC\r\nD\r\nE");
    let live: WorkspaceTerminalFrame =
        serde_json::from_value(encode(live_at(&grid, 2)).expect("实时页")).expect("网格");
    grid.advance(b"\r20%\r\nF");
    let history = history_before(&grid, 2, live.version);
    let page: WorkspaceTerminalFrame =
        serde_json::from_value(encode(history).expect("新输出不能破坏旧历史读取"))
            .expect("历史网格");
    assert_eq!(page.rows[0][0].text, "A", "追加输出后最早历史A必须仍可读");
    assert_eq!(page.rows[1][0].text, "B", "原边界前的B必须保留");
    assert_ne!(page.rows[1][0].text, "C", "历史分页不能重复下一页的C");
    assert_eq!(grid.snapshot().display_offset, 0, "读取不能滚动Mac终端");
}

/// 场景：历史清空后原分页边界失效，不能把新坐标的数据接到旧页。
/// 数据构造：8列×3行写5行，before=2锚定A/B；CSI 3J删除2行历史，坐标0/1变为C/D。
/// 执行过程：捕获旧边界 → 清空历史 → 用旧版本请求同一before。
/// 预期结果：terminal_changed拒绝新页；实时画面仍为C/D/E；不改变Mac滚动位置。
#[test]
fn terminal_capture_rejects_shifted_history_boundary() {
    let mut grid = terminal::TerminalGrid::new(8, 3);
    grid.advance(b"A\r\nB\r\nC\r\nD\r\nE");
    let live: WorkspaceTerminalFrame =
        serde_json::from_value(encode(live_at(&grid, 2)).expect("实时页")).expect("网格");
    grid.advance(b"\x1b[3J");
    let error = encode(history_before(&grid, 2, live.version)).expect_err("清空后旧边界必须失效");
    assert_eq!(error, "terminal_changed", "已清空历史不得被分页拼接到旧页");
    assert_eq!(
        grid.rows_for_absolute(0, 0)[0][0].c,
        'C',
        "实时主屏仍可阅读C"
    );
    assert_ne!(grid.rows_for_absolute(0, 0)[0][0].c, 'A', "旧历史A已失效");
    assert_eq!(grid.snapshot().display_offset, 0, "拒绝历史不能滚动Mac");
}
