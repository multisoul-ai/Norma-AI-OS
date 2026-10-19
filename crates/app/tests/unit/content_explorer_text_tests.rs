use super::diff::parse_diff_for_display;
use super::text_index::{display_width, TextIndex};
use super::tree::{change_rows, row_is_staged, ChangeRow};
use std::collections::BTreeSet;

/// 场景：正文行索引必须与 `str::lines()` 语义一致，且最长行按显示宽度而非字节数。
/// 数据构造："ab\r\n中文\n\tx" —— CRLF、CJK、Tab 三类行；末行无换行。
/// 执行过程：构造索引并逐行取值。
/// 预期结果：3 行为 "ab"、"中文"、"\tx"；显示宽度 2、4、5，最长行为第 3 行；
/// 字节数视角下 "中文"=6 会误判为最长。
#[test]
fn text_index_matches_lines_and_measures_display_width() {
    let text = "ab\r\n中文\n\tx";
    let index = TextIndex::new("v1", text);
    assert_eq!(index.count(), 3, "三行");
    let lines = (0..3).map(|i| index.line(text, i)).collect::<Vec<_>>();
    assert_eq!(lines, ["ab", "中文", "\tx"], "CRLF 被剥离且内容原样");
    assert_eq!(display_width("中文"), 4, "CJK 占两格");
    assert_eq!(display_width("\tx"), 5, "Tab 显示为四格");
    assert_eq!(index.longest, Some(2), "最长行按显示宽度判定");
    assert!(index.matches("v1", text), "同版本同长度命中缓存");
    assert!(!index.matches("v2", text), "版本变化必须失效");
    assert_eq!(TextIndex::new("v", "").count(), 1, "空文件仍显示一个空行");
    assert_eq!(index.line(text, 99), "", "越界取空串而不是 panic");
}

/// 场景：大小写不同的同名目录不能互相穿插。
/// 数据构造：Foo/a、foo/b、Foo/c 三个变更。
/// 执行过程：构造变更树。
/// 预期结果：每个目录只出现一次，Foo 的两个文件相邻在 Foo 下，foo 的文件在 foo 下。
#[test]
fn change_tree_keeps_case_distinct_directories_contiguous() {
    let rows = change_rows(
        &[
            "Foo/a".to_string(),
            "foo/b".to_string(),
            "Foo/c".to_string(),
        ],
        &BTreeSet::new(),
        "",
        None,
    );
    let dirs = rows
        .iter()
        .filter(|r| matches!(r, ChangeRow::Dir { .. }))
        .count();
    assert_eq!(dirs, 2, "Foo 与 foo 各一次: {rows:?}");
    let order = rows
        .iter()
        .map(|r| match r {
            ChangeRow::Dir { label, .. } => label.clone(),
            ChangeRow::File { name, .. } => name.clone(),
        })
        .collect::<Vec<_>>();
    assert_eq!(order, ["Foo", "a", "c", "foo", "b"], "目录内文件相邻");
}

/// 场景：已暂存后又改动（MM）的文件，行按钮应暂存剩余改动而不是整体取消暂存。
/// 数据构造：四种 (staged, unstaged) 组合。
/// 执行过程：计算行是否按“已暂存”处理。
/// 预期结果：仅 (true,false) 为已暂存；(true,true) 为否，按钮保持 “+”。
#[test]
fn partially_staged_rows_offer_staging_the_rest() {
    assert!(row_is_staged(true, false), "完全暂存才显示取消暂存");
    assert!(!row_is_staged(true, true), "MM 仍可暂存剩余改动");
    assert!(!row_is_staged(false, true), "仅未暂存");
    assert!(!row_is_staged(false, false), "无状态不当作已暂存");
}

/// 场景：差异展示不显示 plumbing 头，但保留重命名与二进制说明。
/// 数据构造：含 diff --git/index/---/+++ 与 rename 行的补丁，内容里有删除行 "-- x"。
/// 执行过程：解析展示行。
/// 预期结果：四类头被丢弃；rename 行保留；hunk 内的 "--- x" 删除行不被误删。
#[test]
fn display_diff_drops_plumbing_headers_only() {
    let patch = "diff --git a/f b/f\nrename from f\nrename to g\nindex 1..2 100644\n--- a/f\n+++ b/g\n@@ -1 +1 @@\n--- x\n+y\n";
    let lines = parse_diff_for_display(patch);
    let texts = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>();
    assert!(
        texts.contains(&"rename from f"),
        "重命名说明保留: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.starts_with("diff --git")),
        "diff --git 被丢弃"
    );
    assert!(
        texts.contains(&"-- x"),
        "hunk 内以 -- 开头的删除内容保留: {texts:?}"
    );
}
