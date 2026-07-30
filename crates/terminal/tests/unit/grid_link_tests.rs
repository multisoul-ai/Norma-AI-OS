use super::TerminalGrid;

/// 自动折行的 Preview 路径：点击任一视觉行都应恢复同一个完整本机文件目标。
///
/// 数据构造（含关键数值的推导过程）：
///   path = "docs/design-docs/assets/voice-input-overflow/prototype-a-floating-voice-draft.png"
///   path length = 81 ASCII cells
///   control grid width = 100 cols > path(81)，路径保持单行
///   wrapped grid width = 66 cols
///   row 0 = path[0..66]，结尾为 "floating-"；row 1 = 剩余 81 - 66 = 15 cells
///   点击 A = row 0,col 0；点击 B = row 1,col 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 先把完整路径写入 100 列 control grid，确认同一文本在不折行时可识别。
///   2. 再连续写入 66 列 ANSI grid，触发终端软折行而不插入换行符。
///   3. 确认折行边界和 `soft_wrap` 标记，再从两行分别调用 `preview_path_at`。
///   4. 比较两次命中是否都还原为写入前的 81 字符路径。
///
/// 预期结果：
///   - 正断言：同一路径在 100 列 control grid 中可识别，排除路径格式校验问题。
///   - 正断言：66 列网格折行边界无额外空白，且首行带 `soft_wrap`。
///   - 正断言：点击折行前、后半段都返回完整路径，能够创建 Preview 卡片。
///   - 负断言：两处都不得只返回 "voice-draft.png" 等残缺片段。
#[test]
fn test_snapshot_detects_preview_path_across_terminal_soft_wrap() {
    let path = "docs/design-docs/assets/voice-input-overflow/prototype-a-floating-voice-draft.png";
    let mut control_grid = TerminalGrid::new(100, 24);
    control_grid.advance(path.as_bytes());
    let control_snapshot = control_grid.snapshot();
    // first row = 66 cells；second row = path(81) - first row(66) = 15 cells
    let mut grid = TerminalGrid::new(66, 24);
    grid.advance(path.as_bytes());
    let snapshot = grid.snapshot();

    assert_eq!(
        control_snapshot.preview_path_at(0, 0).as_deref(),
        Some(path),
        "同一长路径在不折行时应可识别，证明扩展名和连字符不是失败原因"
    );
    assert_eq!(
        snapshot.rows[0][65].c, '-',
        "软折行第一行末格应保留 floating- 的连字符，不能被空白替换"
    );
    assert_eq!(
        snapshot.rows[1][0].c, 'v',
        "软折行第二行首格应紧接 voice-draft.png 的 v，不能插入缩进或换行字符"
    );
    assert!(
        snapshot.rows[0][65].attrs.soft_wrap,
        "alacritty 的 WRAPLINE 必须进入中性快照，供路径命中区分软折行与真实换行"
    );
    assert!(
        !control_snapshot.rows[0][99].attrs.soft_wrap,
        "未折行的 control grid 行末不得误带 soft_wrap"
    );

    let first_row_target = snapshot.preview_path_at(0, 0);
    let second_row_target = snapshot.preview_path_at(1, 0);

    assert_eq!(
        first_row_target.as_deref(),
        Some(path),
        "点击软折行路径的第一视觉行应恢复完整目标，不能因该行尚无文件扩展名而失效"
    );
    assert_eq!(
        second_row_target.as_deref(),
        Some(path),
        "点击软折行路径的第二视觉行应恢复完整目标，不能只看到无目录的文件名片段"
    );
    assert_ne!(
        first_row_target.as_deref(),
        Some("docs/design-docs/assets/voice-input-overflow/prototype-a-floating-"),
        "第一视觉行不得作为残缺路径独立打开"
    );
    assert_ne!(
        second_row_target.as_deref(),
        Some("voice-draft.png"),
        "第二视觉行不得丢失前一行的目录和文件名前缀"
    );
}

/// 真实换行的两个路径片段：不得被误拼成一个可点击 Preview 目标。
///
/// 数据构造（含关键数值的推导过程）：
///   first = "docs/design-docs/assets/voice-input-overflow/prototype-a-floating-"，66 cells
///   second = "voice-draft.png"，15 cells
///   input = first + CRLF(2 bytes) + second
///   grid width = 100 cols > first(66)，因此换行来自 CRLF，不是终端软折行
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 把两个片段用真实 CRLF 写入 100 列 ANSI grid。
///   2. 确认第一行不带 `soft_wrap`。
///   3. 分别从两行调用 `preview_path_at`。
///
/// 预期结果：
///   - 正断言：真实换行边界保持为非 soft-wrap。
///   - 负断言：第一行和第二行都不得被拼成完整路径或独立 Preview 目标。
#[test]
fn test_snapshot_does_not_join_preview_path_fragments_across_hard_newline() {
    let first = "docs/design-docs/assets/voice-input-overflow/prototype-a-floating-";
    let second = "voice-draft.png";
    let full_path = format!("{first}{second}");
    let input = format!("{first}\r\n{second}");
    let mut grid = TerminalGrid::new(100, 24);
    grid.advance(input.as_bytes());
    let snapshot = grid.snapshot();

    assert!(
        !snapshot.rows[0][99].attrs.soft_wrap,
        "CRLF 产生的真实换行不得标记为 soft_wrap"
    );
    assert_eq!(
        snapshot.preview_path_at(0, 0),
        None,
        "真实换行前的无扩展名路径片段不得独立成为 Preview 目标"
    );
    assert_eq!(
        snapshot.preview_path_at(1, 0),
        None,
        "真实换行后的无目录文件名不得独立成为 Preview 目标"
    );
    assert_ne!(
        snapshot.preview_path_at(0, 0).as_deref(),
        Some(full_path.as_str()),
        "路径命中不得跨真实换行拼接相邻文本"
    );
}
