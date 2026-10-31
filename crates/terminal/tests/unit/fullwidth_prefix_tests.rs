use crate::{GridSnapshot, TerminalGrid};

fn snapshot(width: u16, text: &str) -> GridSnapshot {
    let mut grid = TerminalGrid::new(width, 6);
    grid.advance(text.as_bytes());
    grid.snapshot()
}

/// 行内所有路径区间的 (raw, 区间首格字符)。
fn row_paths(snapshot: &GridSnapshot, row: usize) -> Vec<(String, char)> {
    snapshot
        .path_ref_spans_for_row(row)
        .into_iter()
        .map(|span| (span.path_ref.raw, snapshot.rows[row][span.start].c))
        .collect()
}

/// 各类全角标点紧贴路径时，区间都应从路径首字符开始。
///
/// 数据构造：每个前缀 + `docs/a.md`，网格 60 列足够单行容纳；
/// 前缀覆盖：全角冒号、逗号、分号、左括号、书名号、中文引号、省略号、表意句号、
/// 全角叹号/问号，以及两个标点连续（`：（`）。
///
/// 预期结果：
///   - 每种前缀下恰有一个区间，raw 为 `docs/a.md`，区间首格为 'd'。
#[test]
fn test_each_fullwidth_punctuation_prefix_is_cut_before_path() {
    for prefix in [
        "记：",
        "记，",
        "记；",
        "记（",
        "见「",
        "见《",
        "见“",
        "见‘",
        "见…",
        "见。",
        "见！",
        "见？",
        "见：（",
        "见【",
        "见、",
    ] {
        let snap = snapshot(60, &format!("{prefix}docs/a.md"));
        assert_eq!(
            row_paths(&snap, 0),
            vec![("docs/a.md".to_string(), 'd')],
            "前缀 {prefix:?}"
        );
    }
}

/// 前缀之后的行列后缀、ASCII 包裹符、末尾全角标点各自按原规则处理。
///
/// 数据构造：
///   `记：src/main.rs:42:7` → 路径 `src/main.rs`，行 42，列 7
///   `记：(docs/a.md)`      → ASCII 括号仍被剥离
///   `记：docs/a.md。`      → 末尾「。」不属于路径（全角标点不是路径字符，但本行为只约束起点）
///
/// 预期结果：
///   - 行列精确；括号不进入 raw；起点都在路径首字符。
#[test]
fn test_prefix_cut_composes_with_line_col_and_ascii_wrappers() {
    let snap = snapshot(60, "记：src/main.rs:42:7");
    let span = &snap.path_ref_spans_for_row(0)[0];
    assert_eq!(span.path_ref.path, "src/main.rs");
    assert_eq!(span.path_ref.line, Some(42));
    assert_eq!(span.path_ref.col, Some(7));
    assert_eq!(snap.rows[0][span.start].c, 's');

    let snap = snapshot(60, "记：(docs/a.md)");
    assert_eq!(row_paths(&snap, 0), vec![("docs/a.md".to_string(), 'd')]);
}

/// 无全角标点的行为不得改变：纯 ASCII 路径、含中文但无标点的文件名。
///
/// 数据构造：
///   `docs/需求说明.md`（中文文件名、无全角标点）→ 整体保留
///   `see docs/a.md`（空格分隔）→ 仍只取 `docs/a.md`
///
/// 预期结果：
///   - 中文文件名的 raw 完整，区间首格为 'd'。
#[test]
fn test_paths_without_fullwidth_punctuation_are_unchanged() {
    let snap = snapshot(60, "docs/需求说明.md");
    assert_eq!(
        row_paths(&snap, 0),
        vec![("docs/需求说明.md".to_string(), 'd')]
    );

    let snap = snapshot(60, "see docs/a.md");
    assert_eq!(row_paths(&snap, 0), vec![("docs/a.md".to_string(), 'd')]);
}

/// 只有标点、没有路径时不产生区间，也不 panic（起点越过词尾）。
///
/// 数据构造：`记：`（以标点结尾的词）、单独的 `：`。
///
/// 预期结果：区间为空；候选为空。
#[test]
fn test_prefix_without_path_yields_nothing() {
    for text in ["记：", "：", "（）"] {
        let snap = snapshot(20, text);
        assert!(row_paths(&snap, 0).is_empty(), "{text:?}");
        assert!(snap.preview_path_candidates_at(0, 1).is_empty(), "{text:?}");
    }
}

/// 点击前缀中文本身不得命中路径；点击路径任意位置都命中无前缀路径。
///
/// 数据构造：`验证记录：docs/a.md`（8 列中文 + 路径 9 列），60 列网格。
///   中文列 0..=7，路径从第 8 列起。
///
/// 预期结果：
///   - 点击第 0、6 列（中文）→ `preview_path_at` 为 None，`path_ref_at` 为 None。
///   - 点击第 8、12 列 → `Some("docs/a.md")`。
#[test]
fn test_clicking_cjk_prefix_does_not_hit_but_path_does() {
    let snap = snapshot(60, "验证记录：docs/a.md");
    for col in [0, 6] {
        assert_eq!(snap.preview_path_at(0, col), None, "列 {col}");
        assert_eq!(snap.path_ref_at(0, col), None, "列 {col}");
    }
    for col in [10, 14] {
        assert_eq!(
            snap.preview_path_at(0, col).as_deref(),
            Some("docs/a.md"),
            "列 {col}"
        );
        assert_eq!(
            snap.path_ref_at(0, col).map(|p| p.raw).as_deref(),
            Some("docs/a.md"),
            "列 {col}"
        );
    }
}

/// CLI 硬折行 + 全角前缀：多种宽度下点击两行都恢复无前缀的完整路径。
///
/// 数据构造：宽度 w ∈ {30, 40, 52}，第 0 行 = `结果：`（6 列）+ 路径头，路径头长度
///   = w - 6 且以 `-` 结尾（`docs/` + 若干 `x` + `-`），恰好占满一行；
///   第 1 行 = `tail.md`；两行之间是 `\r\n` 硬换行，满足 CLI 在连字符处换行的条件。
///   完整路径 = 路径头 + `tail.md`。
///
/// 预期结果：
///   - 第 0 行末格非空（确认确实占满，不是空转）。
///   - 点击两行都含完整路径；任何候选都不含「：」。
#[test]
fn test_hard_wrapped_path_after_prefix_for_several_widths() {
    for width in [30u16, 40, 52] {
        let head_len = width as usize - 6;
        let head = format!("docs/{}-", "x".repeat(head_len - 6));
        assert_eq!(head.len(), head_len);
        let full = format!("{head}tail.md");
        let snap = snapshot(width, &format!("结果：{head}\r\ntail.md"));
        assert_ne!(
            snap.rows[0][width as usize - 1].c,
            ' ',
            "w={width} 第 0 行应被占满"
        );
        for (row, col) in [(0, 10), (1, 1)] {
            let candidates = snap.preview_path_candidates_at(row, col);
            assert!(
                candidates.contains(&full),
                "w={width} ({row},{col}) {candidates:?}"
            );
            assert!(
                candidates.iter().all(|c| !c.contains('：')),
                "w={width} {candidates:?}"
            );
        }
    }
}

/// 同一行两个「标点+路径」：各取各的，互不吞并。
///
/// 数据构造：`甲：docs/a.md 乙：docs/b.md`。
///
/// 预期结果：两个区间，raw 依次为 docs/a.md、docs/b.md。
#[test]
fn test_two_prefixed_paths_on_one_row_stay_separate() {
    let snap = snapshot(80, "甲：docs/a.md 乙：docs/b.md");
    assert_eq!(
        row_paths(&snap, 0),
        vec![
            ("docs/a.md".to_string(), 'd'),
            ("docs/b.md".to_string(), 'd')
        ]
    );
}

/// 反引号路径、网页链接不受影响。
///
/// 数据构造：`见：`docs/a.md`` 与 `链接：https://example.com/a`。
///
/// 预期结果：反引号路径仍可识别；网页链接 `link_at` 返回完整 URL（URL 不经全角切分）。
#[test]
fn test_backtick_paths_and_web_links_unaffected() {
    let snap = snapshot(60, "见：`docs/a.md`");
    assert!(
        row_paths(&snap, 0)
            .iter()
            .any(|(raw, _)| raw.ends_with("docs/a.md")),
        "{:?}",
        row_paths(&snap, 0)
    );
    let snap = snapshot(60, "链接： https://example.com/a");
    assert_eq!(
        snap.link_at(0, 20).as_deref(),
        Some("https://example.com/a")
    );
}
/// 路径之后的全角标点（句号、右括号、冒号、逗号后接中文）不属于路径。
///
/// 数据构造（网格 60 列，单行）：
///   `见：docs/a.md。`、`（docs/a.md）`、`docs/a.md，下一句`、`docs/a.md：ok`、
///   `（docs/a.md:3）`（行号后接右括号）。
///
/// 预期结果：
///   - 每行恰有一个区间，raw 依次为 docs/a.md、docs/a.md、docs/a.md、docs/a.md、docs/a.md:3；
///   - 区间终点（右开）正好是路径末字符之后，下划线不覆盖标点。
#[test]
fn test_trailing_fullwidth_punctuation_is_not_part_of_path() {
    for (text, raw) in [
        ("见：docs/a.md。", "docs/a.md"),
        ("（docs/a.md）", "docs/a.md"),
        ("docs/a.md，下一句", "docs/a.md"),
        ("docs/a.md：ok", "docs/a.md"),
        ("（docs/a.md:3）", "docs/a.md:3"),
    ] {
        let snap = snapshot(60, text);
        let spans = snap.path_ref_spans_for_row(0);
        assert_eq!(spans.len(), 1, "{text:?} {spans:?}");
        assert_eq!(spans[0].path_ref.raw, raw, "{text:?}");
        let last = snap.rows[0][spans[0].end - 1].c;
        assert!(
            last == 'd' || last == '3',
            "{text:?} 终点应落在路径末字符，实际 {last:?}"
        );
        assert_eq!(
            snap.preview_path_at(0, spans[0].start).as_deref(),
            Some(raw),
            "{text:?} 点击应得到无标点路径"
        );
    }
}

/// 折行候选的末片段后接全角标点：候选不含该标点。
///
/// 数据构造：52 列，第 0 行 `SHA-256 和完整验证记录：artifacts/dev-local-package-`，
///   第 1 行 `20261030-102859/verification.md。`。
///
/// 预期结果：完整路径在候选中，且所有候选都不含「。」「：」。
#[test]
fn test_wrapped_candidate_drops_trailing_fullwidth_punctuation() {
    let snap = snapshot(
        52,
        "SHA-256 和完整验证记录：artifacts/dev-local-package-\r\n20261030-102859/verification.md。",
    );
    let full = "artifacts/dev-local-package-20261030-102859/verification.md".to_string();
    let candidates = snap.preview_path_candidates_at(1, 3);
    assert!(candidates.contains(&full), "{candidates:?}");
    assert!(
        candidates.iter().all(|c| !c.contains(['。', '：'])),
        "{candidates:?}"
    );
}
