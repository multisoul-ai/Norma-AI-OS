use super::resolve_terminal_path;
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
        let intent =
            terminal_link_intent(&grid.snapshot(), TextPoint::new(row, 2), false, 1, &|_| {
                false
            });
        let TerminalLinkIntent::OpenPath { paths, .. } = intent else {
            panic!("路径点击必须进入预览，实际 {intent:?}");
        };
        paths
    };
    let bases = [root.to_path_buf()];
    let resolve = |targets: &[String]| resolve_terminal_path(&bases, None, targets);
    let wide_path = resolve(&targets(&wide, 0)).expect("完整路径可解析").path;
    let first = resolve(&targets(&narrow, 0)).expect("前半段可恢复").path;
    let second_targets = targets(&narrow, 1);
    let second = resolve(&second_targets).expect("后半段可恢复").path;
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
    let ambiguous = resolve(&second_targets);
    assert!(
        ambiguous
            .as_ref()
            .is_err_and(|error| error.contains("ambiguous")),
        "同时存在两个候选时必须明确报歧义"
    );
    assert_ne!(
        ambiguous.ok().map(|hit| hit.path),
        Some(expected),
        "歧义时不得默认选择拼接结果"
    );
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
    let path = resolve_terminal_path(
        &[temp.path().to_path_buf()],
        None,
        &["report.md".into(), "./report.md".into()],
    )
    .expect("同一真实文件的别名不应视为歧义")
    .path;
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

/// 相对路径解析顺序与行列携带：Agent 工作目录优先于项目根，`~/` 展开主目录，`:行:列` 随结果返回。
///
/// 数据构造（含关键数值的推导过程）：
///   project/ 与 project/wt/ 各有 src/a.rs（内容分别为 root / worktree），
///   project/only_root.rs 仅在项目根存在；home/notes/todo.md 在模拟主目录下。
///   bases = [project/wt, project]（Agent worktree 在前，项目根在后）。
///   目标 "src/a.rs:12:3"：行 12、列 3。
///
/// 执行过程：
///   1. 解析 "src/a.rs:12:3"：两个 base 都命中，应取第一个 base。
///   2. 解析 "only_root.rs"：worktree 无此文件，回落项目根。
///   3. 解析 "~/notes/todo.md:7"，home 指向模拟主目录。
///   4. 解析 "missing.rs" 应报不存在。
///
/// 预期结果：
///   - 1 返回 wt/src/a.rs，line=12，col=3；且不是项目根的同名文件。
///   - 2 返回项目根文件，line/col 为 None。
///   - 3 返回 home/notes/todo.md，line=7。
///   - 4 为 Err，错误文本包含 "does not exist"。
#[test]
fn terminal_path_resolves_worktree_first_home_and_line_suffix() {
    let temp = tempfile::tempdir().expect("创建临时目录");
    let project = temp.path().join("project");
    let worktree = project.join("wt");
    let home = temp.path().join("home");
    for dir in [
        project.join("src"),
        worktree.join("src"),
        home.join("notes"),
    ] {
        std::fs::create_dir_all(dir).expect("创建目录");
    }
    std::fs::write(project.join("src/a.rs"), "root").expect("写入项目根文件");
    std::fs::write(worktree.join("src/a.rs"), "worktree").expect("写入 worktree 文件");
    std::fs::write(project.join("only_root.rs"), "only").expect("写入仅项目根文件");
    std::fs::write(home.join("notes/todo.md"), "todo").expect("写入主目录文件");
    let bases = [worktree.clone(), project.clone()];
    let resolve = |target: &str| resolve_terminal_path(&bases, Some(&home), &[target.to_string()]);

    let first = resolve("src/a.rs:12:3").expect("两个 base 都存在时应命中");
    assert_eq!(first.path, worktree.join("src/a.rs"), "Agent worktree 优先");
    assert_ne!(
        first.path,
        project.join("src/a.rs"),
        "不得选中项目根同名文件"
    );
    assert_eq!(
        (first.line, first.col),
        (Some(12), Some(3)),
        "行列后缀应随结果返回"
    );

    let fallback = resolve("only_root.rs").expect("worktree 缺失时回落项目根");
    assert_eq!(
        fallback.path,
        project.join("only_root.rs"),
        "应回落到项目根"
    );
    assert_eq!(
        (fallback.line, fallback.col),
        (None, None),
        "无后缀时行列为空"
    );

    let tilde = resolve("~/notes/todo.md:7").expect("~/ 应展开到主目录");
    assert_eq!(tilde.path, home.join("notes/todo.md"), "~ 应替换为 home");
    assert_eq!(tilde.line, Some(7), "~ 路径同样携带行号");

    let missing = resolve("missing.rs");
    assert!(
        missing.is_err_and(|error| error.contains("does not exist")),
        "全部 base 都不存在时应报不存在"
    );
}

/// 目录也是合法点击目标：相对目录按 base 顺序解析，`./` 前缀目录同样命中。
///
/// 数据构造（含关键数值的推导过程）：
///   project/wt/lib/（仅 worktree 有）、project/docs/（仅项目根有）；
///   bases = [project/wt, project]；共 2 个目录，各只在一个 base 下存在。
///
/// 执行过程：
///   1. 解析 "./lib"：worktree 下是目录 → 命中 worktree 版本。
///   2. 解析 "./docs"：worktree 下不存在 → 回落项目根。
///   3. 解析 "./nothing"：两个 base 都不存在 → Err。
///
/// 预期结果：
///   - 1 返回 wt/lib，且 path.is_dir()。
///   - 2 返回 project/docs。
///   - 3 为 Err（目录不存在不得被当成命中）。
#[test]
fn terminal_path_resolves_directories_in_base_order() {
    let temp = tempfile::tempdir().expect("创建临时目录");
    let project = temp.path().join("project");
    let worktree = project.join("wt");
    std::fs::create_dir_all(worktree.join("lib")).expect("创建 worktree 目录");
    std::fs::create_dir_all(project.join("docs")).expect("创建项目根目录");
    let bases = [worktree.clone(), project.clone()];
    let resolve = |target: &str| resolve_terminal_path(&bases, None, &[target.to_string()]);

    let lib = resolve("./lib").expect("worktree 目录应命中");
    assert_eq!(lib.path, worktree.join("lib"), "Agent worktree 目录优先");
    assert!(lib.path.is_dir(), "命中结果应是目录");

    let docs = resolve("./docs").expect("worktree 缺失时回落项目根目录");
    assert_eq!(docs.path, project.join("docs"), "应回落到项目根的目录");

    assert!(resolve("./nothing").is_err(), "不存在的目录不得命中");
}
