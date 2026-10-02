use super::resolve_terminal_preview_path;
use crate::canvas::terminal_link::{terminal_link_intent, TerminalLinkIntent};
use crate::canvas::text_selection::TextPoint;
use crate::preview::{load_preview_content, PreviewContent};
use terminal::TerminalGrid;

/// 窄卡片文件打开：点击折行任一半段，读取完整路径；有歧义时拒绝猜测。
///
/// 数据构造（含关键数值的推导过程）：
///   文件 = temp/docs/quality/report.md，内容 recovered。
///   宽 grid = 80 cols；窄 grid = 20 cols，首行两空格 + docs/ = 7 cells，
///   完整首行需 7 + quality/report.md(17) = 24 > 20，CLI 插入 CRLF。
/// 执行过程：
///   1. 通过真实 TerminalGrid 和点击意图取得完整/前半/后半三个目标。
///   2. 用与后台相同的 resolver 选择文件，再用实际 loader 读取。
///   3. 新建 quality/report.md，使残段与完整候选同时存在，再解析。
///
/// 预期结果：
///   - 三个点击均读取 recovered，残段文件不存在时不得报不存在。
///   - 两个不同文件都存在时明确报歧义，不任意打开其中一个。
#[test]
fn terminal_preview_recovers_cli_wrap_and_rejects_ambiguity() {
    let temp = tempfile::tempdir().expect("创建临时工作区");
    let root = temp.path();
    let full = root.join("docs/quality/report.md");
    std::fs::create_dir_all(full.parent().expect("完整文件有父目录")).expect("创建完整目录");
    std::fs::write(&full, "recovered").expect("写入完整文件");
    let mut wide = TerminalGrid::new(80, 4);
    wide.advance(b"  docs/quality/report.md");
    let mut narrow = TerminalGrid::new(20, 4);
    narrow.advance(b"  docs/\r\n  quality/report.md");
    let targets = |grid: &TerminalGrid, row| {
        let intent = terminal_link_intent(&grid.snapshot(), TextPoint::new(row, 2), false, 1);
        let TerminalLinkIntent::OpenPreviewCard { paths } = intent else {
            panic!("路径点击必须进入预览，实际 {intent:?}");
        };
        paths
    };
    let wide_path =
        resolve_terminal_preview_path(root, &targets(&wide, 0)).expect("完整路径可解析");
    let first = resolve_terminal_preview_path(root, &targets(&narrow, 0)).expect("前半段可恢复");
    let second_targets = targets(&narrow, 1);
    let second = resolve_terminal_preview_path(root, &second_targets).expect("后半段可恢复");
    let expected = full;
    assert_eq!(wide_path, expected, "宽卡片应打开完整文件");
    assert_eq!(first, expected, "点击前半段应打开相同文件");
    assert_eq!(second, expected, "点击后半段不得丢失 docs 目录");
    assert!(
        !root.join("quality/report.md").exists(),
        "残段本身不存在，测试才能复现原报错"
    );
    let content = load_preview_content(second, None).expect("恢复路径应可实际读取");
    assert_eq!(
        content.text_source(),
        Some("recovered"),
        "预览必须显示原文件内容"
    );
    assert!(
        !matches!(content, PreviewContent::Image { .. }),
        "Markdown 不应误判为图片"
    );
    std::fs::create_dir_all(root.join("quality")).expect("创建歧义目录");
    std::fs::write(root.join("quality/report.md"), "different").expect("写入同名独立文件");
    let ambiguous = resolve_terminal_preview_path(root, &second_targets);
    assert!(
        ambiguous
            .as_ref()
            .is_err_and(|error| error.contains("ambiguous")),
        "同时存在两个候选时必须明确报歧义"
    );
    assert_ne!(ambiguous.ok(), Some(expected), "歧义时不得默认选择拼接结果");
}

/// 符号链接预览：去重按真实文件判断，但读取类型必须保留用户点击的 .md 后缀。
///
/// 数据构造（含关键数值的推导过程）：
///   raw 为无扩展名文件，report.md → raw；两个候选实际指向同一个文件。
/// 执行过程：
///   1. 创建 raw 和 report.md 符号链接。
///   2. 使用 report.md 与 ./report.md 解析，再读取返回路径。
///
/// 预期结果：
///   - 两个别名不应报歧义，Markdown 后缀和原始内容均保留。
///   - 不得把无扩展名真实路径交给类型推断。
#[cfg(unix)]
#[test]
fn terminal_preview_preserves_symlink_extension() {
    let temp = tempfile::tempdir().expect("创建临时工作区");
    std::fs::write(temp.path().join("raw"), "symlink content").expect("写入无扩展名文件");
    std::os::unix::fs::symlink("raw", temp.path().join("report.md")).expect("创建符号链接");
    let path =
        resolve_terminal_preview_path(temp.path(), &["report.md".into(), "./report.md".into()])
            .expect("同一真实文件的别名不应视为歧义");
    assert_eq!(
        path.extension().and_then(|value| value.to_str()),
        Some("md"),
        "解析结果必须保留点击路径的扩展名"
    );
    assert_ne!(
        path.file_name().and_then(|value| value.to_str()),
        Some("raw"),
        "不能返回会丢失类型信息的真实文件名"
    );
    let content = load_preview_content(path, None).expect("符号链接应可按 Markdown 读取");
    assert_eq!(
        content.text_source(),
        Some("symlink content"),
        "应读到符号链接目标内容"
    );
}
