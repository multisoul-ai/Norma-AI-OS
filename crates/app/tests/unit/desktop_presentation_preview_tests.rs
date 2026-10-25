use super::*;

/// 场景：显式类型覆盖扩展名，且 Present 可以读取来源项目外的指定文件。
/// 数据构造：临时目录里的 .txt Markdown 与 .md 纯文本，不依赖工作空间 root。
/// 执行过程：调用抽屉使用的生产文件加载函数，并通过本地内容协议读取 HTML。
/// 预期结果：Markdown 标题真正渲染；Text 保留原文，不误走富文本或加载中状态。
#[test]
fn desktop_presentation_preview_honors_type_and_external_file() {
    let temp = tempfile::tempdir().expect("外部文件目录");
    let markdown = temp.path().join("report.txt");
    std::fs::write(&markdown, "# Visible heading").expect("写入 Markdown");
    let LoadedContent::Rich { preview, .. } =
        load_presented_file(markdown, canvas::PreviewType::Markdown).expect("显式 Markdown 应加载")
    else {
        panic!("显式 Markdown 必须创建富文本预览");
    };
    let response = crate::preview::local_content::respond(
        wry::http::Request::builder()
            .uri(&preview.url)
            .body(vec![])
            .expect("本地预览请求"),
    );
    let html = String::from_utf8(response.body().clone()).expect("HTML 应为 UTF-8");
    assert!(
        html.contains("<h1>Visible heading</h1>"),
        "扩展名 .txt 不得覆盖显式 Markdown 类型"
    );
    assert!(!html.contains("纯文本使用编辑器"), "不能误用默认扩展名类型");
    let sibling = temp.path().join("private.txt");
    std::fs::write(&sibling, "unrelated private data").expect("相邻文件");
    let sibling_url = url::Url::parse(&preview.url)
        .expect("预览 URL")
        .join("private.txt")
        .expect("相邻资源 URL");
    let denied = crate::preview::local_content::respond(
        wry::http::Request::builder()
            .uri(sibling_url.as_str())
            .body(vec![])
            .expect("越界资源请求"),
    );
    assert_eq!(
        denied.status(),
        404,
        "展示单个外部文件不能授权读取整个父目录"
    );
    assert!(
        !String::from_utf8_lossy(denied.body()).contains("unrelated private data"),
        "拒绝响应不能泄露相邻文件"
    );
    let text = temp.path().join("plain.md");
    std::fs::write(&text, "# Literal heading").expect("写入 Text");
    let LoadedContent::Text(source) =
        load_presented_file(text, canvas::PreviewType::Text).expect("显式 Text 应加载")
    else {
        panic!("显式 Text 必须进入纯文本展示");
    };
    assert_eq!(source, "# Literal heading", "Text 应保留原始 Markdown 符号");
}

/// 场景：--type image 展示无扩展名的图片。
/// 数据构造：2×2 的 PNG 写入无扩展名文件，使用真实本地内容协议请求图片资源。
/// 执行过程：生产加载函数创建预览，再 GET 该资源。
/// 预期结果：图片正文可用且 MIME 为 image/png，不会被 nosniff 当作任意二进制阻止。
#[test]
fn desktop_presentation_preview_serves_extensionless_image() {
    let temp = tempfile::tempdir().expect("图片目录");
    let path = temp.path().join("image-without-extension");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 80, 200, 255]))
        .save_with_format(&path, image::ImageFormat::Png)
        .expect("写入测试 PNG");
    let LoadedContent::Rich { preview, text, .. } =
        load_presented_file(path, canvas::PreviewType::Image).expect("显式图片加载")
    else {
        panic!("图片必须使用富媒体预览");
    };
    let url = url::Url::parse(&preview.url)
        .expect("预览 URL")
        .join("image-without-extension")
        .expect("资源 URL");
    let response = crate::preview::local_content::respond(
        wry::http::Request::builder()
            .uri(url.as_str())
            .body(vec![])
            .expect("资源请求"),
    );
    assert_eq!(response.status(), 200, "无扩展名图片必须可以加载");
    assert_eq!(
        response.headers()["Content-Type"],
        "image/png",
        "必须按真实格式返回 MIME"
    );
    assert!(
        response.body().starts_with(b"\x89PNG"),
        "应返回实际 PNG 数据"
    );
    assert!(text.is_none(), "图片不能被当成 UTF-8 文本加载");
}
