use super::diff::{diff_stats, is_binary_patch, parse_diff, DiffKind};
use super::layout::explorer_layout;
use super::tree::{change_rows, search_rows, ChangeRow};
use std::collections::BTreeSet;

/// 场景：右栏在截图宽度与常规宽度下必须正文、目录并排。
/// 数据构造：565px（用户截图）、474px（常规）、371px（临界）、370px、185px 五个抽屉宽度。
/// 执行过程：计算布局。
/// 预期结果：565→目录 0.32×565=180.8；474→0.32×474=151.7 取下限 170；
/// 371→170 目录 + 1px 分隔 = 正文 200 刚好不紧凑；370/185 进入紧凑模式。
#[test]
fn explorer_layout_keeps_document_and_tree_side_by_side_until_document_is_too_narrow() {
    let wide = explorer_layout(565.0);
    assert!(
        (wide.sidebar - 180.8).abs() < 0.01,
        "565px 目录宽 {}",
        wide.sidebar
    );
    assert!(!wide.compact, "565px 不应紧凑");
    assert!(
        (wide.document - 383.2).abs() < 0.01,
        "565px 正文宽 {}",
        wide.document
    );
    let normal = explorer_layout(474.0);
    assert_eq!(normal.sidebar, 170.0, "474px 目录取最小 170");
    assert!(!normal.compact, "474px 正文仍可并排");
    assert!(
        !explorer_layout(371.0).compact,
        "371px 正文恰好 200，不紧凑"
    );
    assert!(explorer_layout(370.0).compact, "370px 进入紧凑");
    assert!(explorer_layout(185.0).compact, "185px 不得强行并排");
}

fn paths(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// 场景：已更改文件按目录层级展示，目录先于文件。
/// 数据构造：docs/research/a.md、docs/b.md、README.md。
/// 执行过程：构造无筛选、无折叠的树。
/// 预期结果：docs(0) → research(1) → a.md(2) → b.md(1) → README.md(0)，index 对应输入顺序。
#[test]
fn change_tree_groups_by_directory_with_stable_file_identity() {
    let rows = change_rows(
        &paths(&["docs/research/a.md", "docs/b.md", "README.md"]),
        &BTreeSet::new(),
        "",
    );
    let shape = rows
        .iter()
        .map(|r| match r {
            ChangeRow::Dir { label, depth, .. } => format!("D{depth}:{label}"),
            ChangeRow::File { index, depth, name } => format!("F{depth}:{name}:{index}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        shape,
        [
            "D0:docs",
            "D1:research",
            "F2:a.md:0",
            "F1:b.md:1",
            "F0:README.md:2"
        ],
        "层级与身份"
    );
}

/// 场景：折叠目录隐藏后代；筛选时忽略折叠并保留祖先。
/// 数据构造：同上，折叠 docs；再以 "a.md" 筛选。
/// 执行过程：分别构造两棵树。
/// 预期结果：折叠后只剩 docs（open=false）与 README；筛选后 docs/research/a.md 三行，b.md 被滤掉。
#[test]
fn change_tree_collapse_hides_children_and_search_keeps_ancestors() {
    let all = paths(&["docs/research/a.md", "docs/b.md", "README.md"]);
    let collapsed = BTreeSet::from(["docs".to_string()]);
    let rows = change_rows(&all, &collapsed, "");
    assert_eq!(rows.len(), 2, "折叠 docs 后仅剩 docs 与 README: {rows:?}");
    assert!(
        matches!(&rows[0], ChangeRow::Dir { open: false, .. }),
        "docs 为收起态"
    );
    let rows = change_rows(&all, &collapsed, "A.MD");
    assert_eq!(rows.len(), 3, "筛选保留祖先且无视折叠: {rows:?}");
    assert!(
        !rows
            .iter()
            .any(|r| matches!(r, ChangeRow::File { name, .. } if name == "b.md")),
        "不匹配的 b.md 不出现"
    );
}

/// 场景：文件搜索结果仍是树，而不是平铺路径按钮。
/// 数据构造：src/lib.rs、src/app/main.rs 两个命中。
/// 执行过程：构造搜索树。
/// 预期结果：src(0) 只出现一次，app(1)，main.rs(2)，lib.rs(1)；标签只含末段名。
#[test]
fn search_rows_synthesize_each_ancestor_once() {
    let rows = search_rows(&[
        ("src/lib.rs".into(), false),
        ("src/app/main.rs".into(), false),
    ]);
    let shape = rows
        .iter()
        .map(|r| format!("{}:{}:{}", r.depth, r.label, r.directory))
        .collect::<Vec<_>>();
    assert_eq!(
        shape,
        [
            "0:src:true",
            "1:app:true",
            "2:main.rs:false",
            "1:lib.rs:false"
        ],
        "祖先去重且带层级"
    );
}

/// 场景：diff 行号与新增/删除语义必须真实，不得把所有行染成新增。
/// 数据构造：一个含上下文、删除、新增的 hunk，起始 old=10 new=10。
/// 执行过程：解析并统计。
/// 预期结果：元信息无行号；上下文 10；删除取旧行号 11；新增取新行号 11；统计 +1 -1。
#[test]
fn diff_lines_carry_numbers_and_kinds() {
    let patch = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -10,2 +10,2 @@\n ctx\n-old\n+new\n";
    let lines = parse_diff(patch);
    assert_eq!(lines[0].kind, DiffKind::Meta, "头部为元信息");
    assert_eq!(lines[0].number, None, "元信息无行号");
    let body = &lines[4..];
    assert_eq!(
        (body[0].kind, body[0].number),
        (DiffKind::Context, Some(10)),
        "上下文"
    );
    assert_eq!(
        (body[1].kind, body[1].number),
        (DiffKind::Remove, Some(11)),
        "删除用旧行号"
    );
    assert_eq!(
        (body[2].kind, body[2].number),
        (DiffKind::Add, Some(11)),
        "新增用新行号"
    );
    assert_eq!(diff_stats(&lines), (1, 1), "增删统计");
    assert!(!is_binary_patch(patch), "文本补丁不是二进制");
    assert!(
        is_binary_patch("Binary files a/x and b/x differ\n"),
        "二进制补丁识别"
    );
}
