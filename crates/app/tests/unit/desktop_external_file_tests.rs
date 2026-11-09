use super::*;

/// 场景：普通文件标签打开项目外文本，不依赖 Present 的显式类型。
/// 数据构造：互不包含的项目目录和外部目录，文件名含中文与空格。
/// 执行过程：调用普通标签的生产加载函数。
/// 预期结果：保留完整文本，不返回项目外错误或富媒体内容。
#[test]
fn external_file_text_opens_from_project_tab() {
    let project = tempfile::tempdir().expect("项目目录");
    let external = tempfile::tempdir().expect("外部目录");
    let path = external.path().join("外部 report.txt");
    std::fs::write(&path, "EXTERNAL DOCUMENT\n第二行").expect("文本夹具");
    let result = load_local_file(Some(project.path().to_owned()), path, "外部 report.txt")
        .expect("项目外文本应加载");
    let LoadedContent::Text(text) = result else {
        panic!("文本不能进入富媒体分支")
    };
    assert_eq!(text, "EXTERNAL DOCUMENT\n第二行", "外部文档应保留原文");
}

fn request(
    preview: &LocalPreview,
    name: &str,
    range: Option<&str>,
) -> wry::http::Response<Vec<u8>> {
    let url = url::Url::parse(&preview.url)
        .expect("预览 URL")
        .join(name)
        .expect("资源 URL");
    let mut request = wry::http::Request::builder().uri(url.as_str());
    if let Some(range) = range {
        request = request.header("Range", range);
    }
    crate::preview::local_content::respond(request.body(vec![]).expect("资源请求"))
}

/// 场景：外部 Markdown/HTML/图片/视频在普通文件标签预览，同时隔离相邻文件。
/// 数据构造：独立项目与外部目录、带空格中文文件名、私有相邻文本；视频用字节夹具验证协议。
/// 执行过程：生产加载函数生成页面，再请求媒体资源、视频范围和未授权资源。
/// 预期结果：文档渲染、媒体字节/MIME/Range 正确，相邻文件不可读取，关闭后 token 失效。
#[test]
fn external_file_rich_previews_are_isolated() {
    let project = tempfile::tempdir().expect("项目目录");
    let external = tempfile::tempdir().expect("外部目录");
    std::fs::write(external.path().join("private.txt"), "PRIVATE CONTENT").expect("私有文件");
    for (name, bytes, expected) in [
        (
            "外部 report.md",
            b"# EXTERNAL MARKDOWN".as_slice(),
            "<h1>EXTERNAL MARKDOWN</h1>",
        ),
        (
            "外部 page.html",
            b"<h1>EXTERNAL HTML</h1>".as_slice(),
            "<h1>EXTERNAL HTML</h1>",
        ),
        (
            "外部 image.svg",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"4\" height=\"4\"/></svg>"
                .as_slice(),
            "<img",
        ),
        (
            "外部 movie.mp4",
            b"0123456789abcdef".as_slice(),
            "<video controls",
        ),
    ] {
        let path = external.path().join(name);
        std::fs::write(&path, bytes).expect("外部文件");
        let LoadedContent::Rich { preview, .. } =
            load_local_file(Some(project.path().to_owned()), path, name).expect("外部富媒体应加载")
        else {
            panic!("富媒体不能显示原始文本")
        };
        let page = request(&preview, "__norma_preview__", None);
        assert_eq!(page.status(), 200, "页面应可读取：{name}");
        assert!(
            String::from_utf8_lossy(page.body()).contains(expected),
            "应输出对应预览内容：{name}"
        );
        let resource = request(&preview, name, None);
        assert_eq!(resource.body(), bytes, "只返回所选文件的内容：{name}");
        if name.ends_with("mp4") {
            let part = request(&preview, name, Some("bytes=2-5"));
            assert_eq!(part.status(), 206, "视频支持分段读取");
            assert_eq!(part.body(), b"2345", "范围包含第 2 至 5 字节");
            assert_eq!(part.headers()["Content-Type"], "video/mp4", "使用视频 MIME");
        }
        let denied = request(&preview, "private.txt", None);
        assert_eq!(denied.status(), 404, "不能授权整个外部目录");
        assert!(
            !String::from_utf8_lossy(denied.body()).contains("PRIVATE CONTENT"),
            "拒绝响应不能泄漏相邻内容"
        );
        let url = preview.url.clone();
        drop(preview);
        let closed = crate::preview::local_content::respond(
            wry::http::Request::builder()
                .uri(url)
                .body(vec![])
                .expect("关闭请求"),
        );
        assert_eq!(closed.status(), 404, "关闭预览后应撤销资源 token");
    }
}

/// 场景：项目内文档仍能引用项目内图片；外部读取保留大小/二进制/目录检查。
/// 数据构造：项目 Markdown 和 SVG、外部超限文本、NUL 文本及目录。
/// 执行过程：读取生产预览和附属资源，再逐一尝试非法文本。
/// 预期结果：项目内附属图片可读取；非法文本返回错误而不是空白成功。
#[test]
fn external_file_limits_and_project_assets_are_preserved() {
    let project = tempfile::tempdir().expect("项目目录");
    let external = tempfile::tempdir().expect("外部目录");
    std::fs::write(project.path().join("report.md"), "![image](image.svg)").expect("文档");
    std::fs::write(project.path().join("image.svg"), "<svg/>").expect("图片");
    let LoadedContent::Rich { preview, .. } = load_local_file(
        Some(project.path().to_owned()),
        project.path().join("report.md"),
        "report.md",
    )
    .expect("项目文档") else {
        panic!("Markdown 预览")
    };
    assert_eq!(
        request(&preview, "image.svg", None).status(),
        200,
        "项目内附属资源仍可访问"
    );
    for (name, bytes) in [
        ("binary.txt", vec![0, 1]),
        (
            "large.txt",
            vec![b'a'; workspace::content::MAX_TEXT_BYTES as usize + 1],
        ),
    ] {
        let path = external.path().join(name);
        std::fs::write(&path, bytes).expect("非法文本夹具");
        assert!(
            load_local_file(Some(project.path().to_owned()), path, name).is_err(),
            "必须拒绝非法文本：{name}"
        );
    }
    assert!(
        load_local_file(
            Some(project.path().to_owned()),
            external.path().to_owned(),
            "directory.txt"
        )
        .is_err(),
        "目录不是普通文件"
    );
}

/// 场景：macOS 的 /tmp 别名、项目内指向外部的显式文件链接和来源项目失效。
/// 数据构造：临时外部文件、项目内 symlink、被删除的来源目录。
/// 执行过程：按普通标签路径加载，再删除项目来源后重新读取绝对路径。
/// 预期结果：用户指定的外部文件可读取，不因来源项目已消失而失败。
#[cfg(unix)]
#[test]
fn external_file_symlink_and_missing_source_open_selected_file() {
    let project = tempfile::tempdir().expect("项目目录");
    let external = tempfile::tempdir().expect("外部目录");
    let path = external.path().join("note.txt");
    std::fs::write(&path, "SELECTED FILE").expect("所选文件");
    let link = project.path().join("note.txt");
    std::os::unix::fs::symlink(&path, &link).expect("显式链接");
    for candidate in [link, path.clone()] {
        let LoadedContent::Text(text) =
            load_local_file(Some(project.path().to_owned()), candidate, "note.txt")
                .expect("所选链接或绝对路径应加载")
        else {
            panic!("应显示文本")
        };
        assert_eq!(text, "SELECTED FILE", "只读取所选目标");
    }
    let missing_root = project.path().to_owned();
    project.close().expect("移除来源目录");
    let LoadedContent::Text(text) = load_local_file(Some(missing_root), path, "note.txt")
        .expect("失效来源不应阻断仍存在的外部文件")
    else {
        panic!("应显示文本")
    };
    assert_eq!(text, "SELECTED FILE", "绝对路径不依赖原项目目录存在");
}
