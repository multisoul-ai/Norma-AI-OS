use super::*;
use canvas::PreviewType;

/// Preview 类型推断：显式类型优先，未指定时按扩展名识别 Markdown/Text/Image。
///
/// 数据构造（含关键数值推导）：
///   explicit path = "/tmp/report.md"，explicit_type = Some(Text)
///   markdown path = "/tmp/report.md"，extension = md
///   text path = "/tmp/notes.txt"，extension = txt
///   image path = "/tmp/photo.png"，extension = png
///   unsupported path = "/tmp/book.pdf"，extension = pdf（MVP 明确 out of scope）
///
/// 执行过程：
///   1. infer_preview_type(explicit, Some(Text)) → 直接返回 Text
///   2. infer_preview_type(md/txt/png, None) → 按扩展名推断
///   3. infer_preview_type(pdf, None) → 返回 unsupported 错误
///
/// 预期结果：
///   - 正断言：md/txt/png 分别推断为 Markdown/Text/Image
///   - 正断言：显式 Text 覆盖 .md 扩展名
///   - 负断言：pdf 不应被误判为 Text 或 Image
#[test]
fn test_infer_preview_type_uses_explicit_type_then_extension() {
    let explicit = infer_preview_type(
        std::path::Path::new("/tmp/report.md"),
        Some(PreviewType::Text),
    )
    .expect("显式 Text 应直接返回");
    let markdown = infer_preview_type(std::path::Path::new("/tmp/report.md"), None)
        .expect(".md 应推断为 Markdown");
    let text = infer_preview_type(std::path::Path::new("/tmp/notes.txt"), None)
        .expect(".txt 应推断为 Text");
    let image = infer_preview_type(std::path::Path::new("/tmp/photo.png"), None)
        .expect(".png 应推断为 Image");
    let unsupported = infer_preview_type(std::path::Path::new("/tmp/book.pdf"), None);

    assert_eq!(
        explicit,
        PreviewType::Text,
        "显式类型应优先于文件扩展名，.md + explicit Text 应得到 Text"
    );
    assert_eq!(
        markdown,
        PreviewType::Markdown,
        ".md 文件应推断为 Markdown renderer"
    );
    assert_eq!(text, PreviewType::Text, ".txt 文件应推断为 Text renderer");
    assert_eq!(
        image,
        PreviewType::Image,
        ".png 文件应推断为 Image renderer"
    );
    assert!(
        matches!(unsupported, Err(PreviewReadError::UnsupportedType)),
        ".pdf 不在 MVP 范围内，应返回 UnsupportedType"
    );
    assert_ne!(
        unsupported.ok(),
        Some(PreviewType::Image),
        ".pdf 不应被误判为 Image"
    );
}

/// Markdown 读取：文件内容应在后台读取阶段解析为结构化 blocks，避免 render 每帧重 parse。
///
/// 数据构造（含关键数值推导）：
///   temp/report.md 内容共 4 行：
///     "# Title" → Heading(level=1, text="Title")
///     空行 → 分隔段落
///     "- item" → ListItem("item")
///     "plain paragraph" → Paragraph("plain paragraph")
///
/// 执行过程：
///   1. 写入 temp/report.md
///   2. load_preview_content(path, None) → infer Markdown → read_to_string → parse_markdown_blocks
///   3. 检查 PreviewContent::Markdown 的 blocks
///
/// 预期结果：
///   - 正断言：包含 H1 Title、列表 item、正文 paragraph
///   - 负断言：Markdown 不应退化为 PreviewContent::Text
#[test]
fn test_load_markdown_preview_parses_blocks_once() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let path = temp.path().join("report.md");
    std::fs::write(&path, "# Title\n\n- item\n\nplain paragraph\n")
        .expect("test should write markdown file");

    let content = load_preview_content(path.clone(), None).expect("markdown preview 应读取成功");

    let PreviewContent::Markdown(document) = content else {
        panic!("Markdown 文件不应退化为 Text 或 Image content");
    };
    assert!(
        document.blocks.contains(&MarkdownBlock::Heading {
            level: 1,
            text: "Title".to_string()
        }),
        "Markdown blocks 应包含 H1 Title"
    );
    assert!(
        document
            .blocks
            .contains(&MarkdownBlock::ListItem("item".to_string())),
        "Markdown blocks 应包含列表项 item"
    );
    assert!(
        document
            .blocks
            .contains(&MarkdownBlock::Paragraph("plain paragraph".to_string())),
        "Markdown blocks 应包含正文段落"
    );
    assert!(
        !matches!(PreviewContent::Markdown(document), PreviewContent::Text(_)),
        "Markdown content 不应被作为纯文本保存"
    );
}

/// Preview 读取错误：目录和不支持扩展名都应返回错误态，而不是静默创建空内容。
///
/// 数据构造（含关键数值推导）：
///   dir path = temp/subdir（是目录，不是文件）
///   pdf path = temp/book.pdf（普通文件，但扩展名 pdf 不在 MVP 支持范围）
///   text path = temp/notes.txt，内容 "hello"
///
/// 执行过程：
///   1. load_preview_content(dir, None) → NotAFile
///   2. load_preview_content(pdf, None) → UnsupportedType
///   3. load_preview_content(txt, None) → Text("hello")
///
/// 预期结果：
///   - 正断言：txt 能读成 PreviewContent::Text
///   - 负断言：目录不应被读取，不支持类型不应 fallback 成 Text
#[test]
fn test_load_preview_content_reports_directory_and_unsupported_type_errors() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let dir = temp.path().join("subdir");
    std::fs::create_dir(&dir).expect("test should create subdir");
    let pdf = temp.path().join("book.pdf");
    std::fs::write(&pdf, "pdf bytes").expect("test should write unsupported file");
    let text = temp.path().join("notes.txt");
    std::fs::write(&text, "hello").expect("test should write text file");

    let dir_result = load_preview_content(dir, None);
    let pdf_result = load_preview_content(pdf, None);
    let text_result = load_preview_content(text, None).expect("text preview 应读取成功");

    assert!(
        matches!(dir_result, Err(PreviewReadError::NotAFile)),
        "目录路径不应被当作 Preview 文件读取"
    );
    assert!(
        matches!(pdf_result, Err(PreviewReadError::UnsupportedType)),
        "不支持类型不应 fallback 成纯文本"
    );
    assert_eq!(
        text_result,
        PreviewContent::Text("hello".to_string()),
        ".txt 文件应读成纯文本内容"
    );
    assert!(
        !matches!(pdf_result.ok(), Some(PreviewContent::Text(_))),
        ".pdf 不应被作为 Text content 保存"
    );
}

/// Preview 路径解析：工作区外 file URI 应解码为真实绝对路径，不得拼接 workspace root。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace root = "/workspace/project"
///   file URI = "file:///tmp/Norma%20Images/%E6%88%90%E5%93%81.png"
///   percent-decoding 后 = "/tmp/Norma Images/成品.png"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将工作区 root 与工作区外 file URI 传给 resolve_preview_path。
///   2. 解析 file scheme，并对路径执行 percent-decoding。
///   3. 比较结果是否为 URI 指向的真实绝对路径。
///
/// 预期结果：
///   - 正断言：结果精确等于 "/tmp/Norma Images/成品.png"。
///   - 负断言：结果不得落到 "/workspace/project" 下，否则外部路径仍被错误拼接。
#[test]
fn test_resolve_preview_path_decodes_external_file_uri() {
    let workspace_root = std::path::Path::new("/workspace/project");
    let uri = "file:///tmp/Norma%20Images/%E6%88%90%E5%93%81.png";

    let resolved = resolve_preview_path(workspace_root, uri)
        .expect("本机 file URI 应解析为可供 Preview 使用的绝对路径");

    assert_eq!(
        resolved,
        std::path::PathBuf::from("/tmp/Norma Images/成品.png"),
        "file URI 应完成空格和中文 percent-decoding，并保留工作区外绝对位置"
    );
    assert!(
        !resolved.starts_with(workspace_root),
        "工作区外 file URI 不得被拼接到 workspace root 下"
    );
}
