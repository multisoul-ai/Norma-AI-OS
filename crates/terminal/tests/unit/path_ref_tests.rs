use super::*;
use crate::TerminalGrid;

/// 带行列后缀的相对路径：`path:行:列` 应拆成路径、行号、列号，且判定为强候选。
///
/// 数据构造（含关键数值的推导过程）：
///   raw = "src/main.rs:42:7"
///   路径部分 = "src/main.rs"（11 字符），行 = 42，列 = 7
///   仅行号变体 raw = "src/main.rs:42"，列应为 None
///   行号 0 变体 raw = "src/main.rs:0"：编辑器行号从 1 起，0 无意义，应丢弃行号而保留路径
///
/// 执行过程：
///   1. 解析三个 raw。
///   2. 逐字段比较 PathRef。
///
/// 预期结果：
///   - `path` 不含 `:行:列`，`line`/`col` 精确；`raw` 保持用户可见文本。
///   - 行号 0 不得产生 `Some(0)`。
#[test]
fn test_parse_path_ref_splits_line_and_column_suffix() {
    let both = parse_path_ref("src/main.rs:42:7").expect("path:line:col 应可识别");
    assert_eq!(both.path, "src/main.rs", "路径部分不得带行列后缀");
    assert_eq!(both.line, Some(42), "第一个数字是行号");
    assert_eq!(both.col, Some(7), "第二个数字是列号");
    assert_eq!(
        both.raw, "src/main.rs:42:7",
        "raw 应保留原文用于缓存键与提示"
    );
    assert_eq!(both.tier, PathTier::Strong, "含 / 与扩展名应是强候选");

    let line_only = parse_path_ref("src/main.rs:42").expect("path:line 应可识别");
    assert_eq!(line_only.line, Some(42), "仅行号时行号应保留");
    assert_eq!(line_only.col, None, "仅行号时列号必须为 None");

    let zero = parse_path_ref("src/main.rs:0").expect("行号 0 仍是合法路径");
    assert_eq!(zero.path, "src/main.rs", "行号 0 时路径仍应保留");
    assert_eq!(zero.line, None, "行号 0 无意义，不应产生 Some(0)");
}

/// 放宽扩展名白名单：代码/配置文件、`~/`、`file://`、包裹符号都应识别。
///
/// 数据构造：
///   - "crates/app/Cargo.toml"：旧白名单只含 md/txt/log/图片，会漏掉 toml
///   - "~/notes/todo.md"：主目录路径
///   - "file:///tmp/a.rs:3"：file URI 带行号
///   - "(src/lib.rs)," 与 "`src/lib.rs`"：括号、逗号、反引号是文本包裹，不属于路径
///
/// 执行过程：逐个解析并比较 `path`。
///
/// 预期结果：
///   - 全部为 Strong，且 `path` 为去包裹后的纯路径。
#[test]
fn test_parse_path_ref_accepts_code_files_home_and_file_uri_after_unwrapping() {
    let toml = parse_path_ref("crates/app/Cargo.toml").expect("toml 应在放宽后可识别");
    assert_eq!(
        toml.tier,
        PathTier::Strong,
        "含目录与扩展名的代码文件是强候选"
    );

    let home = parse_path_ref("~/notes/todo.md").expect("~/ 路径应可识别");
    assert_eq!(
        home.path, "~/notes/todo.md",
        "~ 展开留给 app 解析层，这里保留原样"
    );

    let uri = parse_path_ref("file:///tmp/a.rs:3").expect("file URI 应可识别");
    assert_eq!(uri.path, "file:///tmp/a.rs", "file URI 的行号后缀应被拆出");
    assert_eq!(uri.line, Some(3), "file URI 行号应为 3");

    let wrapped = parse_path_ref("(src/lib.rs),").expect("括号和逗号应被剥离");
    assert_eq!(wrapped.path, "src/lib.rs", "括号与尾逗号不属于路径");
    let ticked = parse_path_ref("`src/lib.rs`").expect("反引号应被剥离");
    assert_eq!(ticked.path, "src/lib.rs", "反引号不属于路径");
}

/// 误报防护：网页链接、版本号、普通词、斜杠命令都不应成为路径引用。
///
/// 数据构造：
///   - "https://example.com/a.md"：网页链接由 link_at 负责
///   - "1.5"、"v1.2"：数字/版本号，扩展名以数字开头
///   - "and/or"：有斜杠但无扩展名
///   - "/compact"：Claude 斜杠命令，绝对路径形态但无扩展名，只能是 NeedsVerify
///   - "main.rs"：无目录的裸文件名，只能是 NeedsVerify（须后台验证存在后才可点击）
///
/// 预期结果：
///   - 前四项返回 None；/compact 与 main.rs 返回 NeedsVerify 而非 Strong。
#[test]
fn test_parse_path_ref_rejects_noise_and_marks_bare_names_needs_verify() {
    for noise in [
        "https://example.com/a.md",
        "1.5",
        "v1.2",
        "and/or",
        "",
        ":42",
    ] {
        assert!(
            parse_path_ref(noise).is_none(),
            "{noise:?} 不应被识别为路径引用"
        );
    }
    let slash = parse_path_ref("/compact").expect("绝对形态应作为待验证候选");
    assert_eq!(
        slash.tier,
        PathTier::NeedsVerify,
        "/compact 无扩展名，不得直接可点击"
    );
    let bare = parse_path_ref("main.rs").expect("裸文件名应作为待验证候选");
    assert_eq!(bare.tier, PathTier::NeedsVerify, "裸文件名必须先验证存在");
}

/// 行内扫描：一行里多个路径各得到精确列区间，包裹符号不进入下划线范围。
///
/// 数据构造（含关键数值的推导过程）：
///   行文本 = "edit (src/a.rs:12) and docs/b.md ok"
///   列索引：e0 d1 i2 t3 空4 (5 s6 ... "src/a.rs:12" 占 6..17（11 字符，右开 17），`)` 在 17
///   "and" 在 19..22；"docs/b.md" 起点 = 23，长 9，终点 32
///   裸词 "edit"/"and"/"ok" 无扩展名无斜杠，不应出现
///
/// 执行过程：
///   1. 写入一行，取 path_ref_spans_for_row(0)。
///
/// 预期结果：
///   - 恰好 2 个区间：(6,17) 对应 src/a.rs 行 12，(23,32) 对应 docs/b.md。
///   - 区间不含 "(" 与 ")"。
#[test]
fn test_path_ref_spans_for_row_reports_exact_columns_without_wrappers() {
    let mut grid = TerminalGrid::new(60, 4);
    grid.advance(b"edit (src/a.rs:12) and docs/b.md ok");
    let snapshot = grid.snapshot();
    let spans = snapshot.path_ref_spans_for_row(0);

    assert_eq!(spans.len(), 2, "应只识别两个路径，实际 {spans:?}");
    assert_eq!(
        (spans[0].start, spans[0].end),
        (6, 17),
        "首个区间应排除左括号与右括号"
    );
    assert_eq!(spans[0].path_ref.path, "src/a.rs", "首个路径应为 src/a.rs");
    assert_eq!(spans[0].path_ref.line, Some(12), "首个路径带行号 12");
    assert_eq!(
        (spans[1].start, spans[1].end),
        (23, 32),
        "第二个区间为 docs/b.md"
    );
    assert_eq!(
        spans[1].path_ref.path, "docs/b.md",
        "第二个路径应为 docs/b.md"
    );
}

/// 单击命中：`path_ref_at` 只在路径区间内命中，区间外与网页链接不命中。
///
/// 数据构造：
///   行文本 = "see main.rs and https://x.io/a.md"
///   "main.rs" 在 4..11（裸文件名，NeedsVerify）；"and" 在 12..15；URL 起点 = 16，列 25 落在 URL 内
///
/// 预期结果：
///   - 列 4、10 命中 main.rs；列 3（空格）、列 12（and）不命中。
///   - URL 内部列 25 不命中（由 link_at 负责，避免双重识别）。
#[test]
fn test_path_ref_at_hits_only_inside_span_and_skips_web_urls() {
    let mut grid = TerminalGrid::new(60, 4);
    grid.advance(b"see main.rs and https://x.io/a.md");
    let snapshot = grid.snapshot();

    assert_eq!(
        snapshot.path_ref_at(0, 4).map(|r| r.path),
        Some("main.rs".to_string()),
        "区间首列应命中"
    );
    assert!(snapshot.path_ref_at(0, 10).is_some(), "区间末列应命中");
    assert!(snapshot.path_ref_at(0, 3).is_none(), "空格列不得命中");
    assert!(
        snapshot.path_ref_at(0, 12).is_none(),
        "无扩展名的 and 不得命中"
    );
    assert!(
        snapshot.path_ref_at(0, 25).is_none(),
        "网页链接内部不得被当作路径"
    );
}

/// 旧入口放宽：`preview_path_at` 应返回代码文件路径并保留 `:行` 后缀，供 app 解析。
///
/// 数据构造：行文本 = "crates/app/src/lib.rs:10:2"，80 列不折行。
///
/// 预期结果：
///   - 返回完整文本（含后缀）；候选列表同样包含。
///   - 无目录的裸词 "lib.rs" 不属于强候选，旧入口不得返回。
#[test]
fn test_preview_path_entry_returns_strong_code_paths_with_suffix() {
    let mut grid = TerminalGrid::new(80, 4);
    grid.advance(b"crates/app/src/lib.rs:10:2");
    let snapshot = grid.snapshot();
    assert_eq!(
        snapshot.preview_path_at(0, 3).as_deref(),
        Some("crates/app/src/lib.rs:10:2"),
        "强候选应带行列后缀返回"
    );

    let mut bare = TerminalGrid::new(80, 4);
    bare.advance(b"lib.rs");
    assert!(
        bare.snapshot().preview_path_at(0, 1).is_none(),
        "裸文件名属于待验证候选，不得经强候选入口返回"
    );
}
