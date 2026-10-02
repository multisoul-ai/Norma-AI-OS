use super::TerminalGrid;

/// OSC 8 文件链接：标签被 CLI 换行后，两行都应打开语义目标。
///
/// 数据构造（含关键数值的推导过程）：
///   URI = file:///tmp/docs/report.md；标签两行分别为 docs/ 和 report.md。
///   grid = 20 cols × 4 lines；最长标签 9 cells < 20，CRLF 是主动换行。
/// 执行过程：
///   1. 写入 OSC 8 开始、两行标签与结束序列。
///   2. 分别点击第 0 行与第 1 行的首字符。
///
/// 预期结果：
///   - 两行均返回完整 URI，不返回标签残段。
#[test]
fn test_preview_osc8_target_survives_label_newline() {
    let uri = "file:///tmp/docs/report.md";
    let mut grid = TerminalGrid::new(20, 4);
    grid.advance(format!("\x1b]8;;{uri}\x1b\\docs/\r\nreport.md\x1b]8;;\x1b\\").as_bytes());
    let snapshot = grid.snapshot();
    assert_eq!(
        snapshot.preview_path_at(0, 0).as_deref(),
        Some(uri),
        "首行标签应使用完整文件目标"
    );
    assert_eq!(
        snapshot.preview_path_at(1, 0).as_deref(),
        Some(uri),
        "换行后的标签应使用同一文件目标"
    );
    assert_ne!(
        snapshot.preview_path_at(1, 0).as_deref(),
        Some("report.md"),
        "标签不能替代链接目标"
    );
}

/// CLI 主动折行路径：任一行命中均提供完整候选，不把普通相邻文件拼接。
///
/// 数据构造（含关键数值的推导过程）：
///   第一行 = 两格缩进 + /project/docs/（14 cells）= 16 cells。
///   第二行 = 两格缩进 + quality/report.md（17 cells）= 19 cells。
///   width = 30，完整首行需 16 + 17 = 33 > 30，因此 CLI 在目录后换行。
/// 执行过程：
///   1. 用 CRLF 写入两行，分别提取第一行、第二行候选。
///   2. 写入两个完整独立文件，确认它们不被拼接。
///
/// 预期结果：
///   - 两个点击位置都含 /project/docs/quality/report.md。
///   - 独立文件仍各自命中，候选中不存在错误拼接路径。
#[test]
fn test_preview_candidates_recover_cli_wrapped_path() {
    let mut grid = TerminalGrid::new(30, 4);
    grid.advance(b"  /project/docs/\r\n  quality/report.md");
    let snapshot = grid.snapshot();
    let full = "/project/docs/quality/report.md".to_string();
    assert!(
        snapshot.preview_path_candidates_at(0, 2).contains(&full),
        "点击前半段应保留完整候选"
    );
    assert!(
        snapshot.preview_path_candidates_at(1, 2).contains(&full),
        "点击后半段应保留完整候选"
    );
    assert!(
        !snapshot.rows[0][29].attrs.soft_wrap,
        "本例必须覆盖 CLI 主动换行而非软折行"
    );
    let mut independent = TerminalGrid::new(30, 4);
    independent.advance(b"  docs/first.md\r\n  docs/second.md");
    let snapshot = independent.snapshot();
    assert_eq!(
        snapshot.preview_path_candidates_at(0, 2),
        vec!["docs/first.md"],
        "首文件不得吞并第二个文件"
    );
    assert_eq!(
        snapshot.preview_path_candidates_at(1, 2),
        vec!["docs/second.md"],
        "次文件不得附加前一文件路径"
    );
}

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
    assert!(
        snapshot.preview_path_candidates_at(0, 0).is_empty(),
        "文本原本放得下，不得生成折行候选"
    );
    assert!(
        snapshot.preview_path_candidates_at(1, 0).is_empty(),
        "真实换行的后半段不得被猜测为文件目标"
    );
}

/// 三行路径与不支持的语义链接：完整路径可恢复，语义目标不能被标签覆盖。
///
/// 数据构造（含关键数值的推导过程）：
///   width = 20；两格缩进 + /project/docs/(14) + design-docs/(12) = 28 > 20；
///   两格缩进 + design-docs/(12) + report-name.md(14) = 28 > 20，形成三行。
///   对照标签为 docs/report.md，实际 OSC 8 目标为 ssh://host/private.md。
/// 执行过程：
///   1. 对三行路径的中间行和末行调用候选提取。
///   2. 对 SSH 目标的文件形标签调用候选提取。
///
/// 预期结果：
///   - 中间行和末行都包含完整路径候选。
///   - SSH 链接不能退回可打开的本机文件标签。
#[test]
fn test_preview_candidates_keep_multiline_paths_and_semantic_boundaries() {
    let mut grid = TerminalGrid::new(20, 5);
    grid.advance(b"  /project/docs/\r\n  design-docs/\r\n  report-name.md");
    let snapshot = grid.snapshot();
    let full = "/project/docs/design-docs/report-name.md".to_string();
    assert!(
        snapshot.preview_path_candidates_at(1, 2).contains(&full),
        "中间片段应恢复完整目标"
    );
    assert!(
        snapshot.preview_path_candidates_at(2, 2).contains(&full),
        "末尾片段应恢复完整目标"
    );
    let mut semantic = TerminalGrid::new(20, 5);
    semantic.advance(b"\x1b]8;;ssh://host/private.md\x1b\\docs/report.md\x1b]8;;\x1b\\");
    let snapshot = semantic.snapshot();
    assert_eq!(
        snapshot.hyperlink_at(0, 0),
        Some("ssh://host/private.md"),
        "对照必须保留真实 SSH 目标"
    );
    assert!(
        snapshot.preview_path_candidates_at(0, 0).is_empty(),
        "不支持的目标不能用标签伪装成本机路径"
    );
    assert_eq!(
        snapshot.preview_path_at(0, 0),
        None,
        "直接命中也不得绕过语义目标"
    );
}
