use crate::preview::local_content::{respond, LocalPreview};
use wry::http::Request;

/// 场景：Markdown 引用同项目图片，同时有人尝试读取越界链接和 Git 文件。
/// 数据构造：临时项目、图片资源、越界符号链接和 .git 内部文件。
/// 执行过程：创建预览，分别请求正文、正常图片、越界路径和 .git。
/// 预期结果：正常资源可读；未注册来源和越界资源拒绝；关闭后 URL 失效。
#[test]
fn local_preview_scopes_resources_and_releases_access() {
    let temp = tempfile::tempdir().expect("目录");
    std::fs::write(temp.path().join("plan.md"), "# Hello").expect("文档");
    std::fs::write(temp.path().join("image.png"), b"image").expect("资源");
    std::fs::create_dir(temp.path().join(".git")).expect("内部目录");
    std::fs::write(temp.path().join(".git/config"), "private").expect("内部配置");
    let outside = tempfile::NamedTempFile::new().expect("外部文件");
    std::os::unix::fs::symlink(outside.path(), temp.path().join("escape")).expect("链接");
    let preview = LocalPreview::new(temp.path(), "plan.md", Some("# Hello")).expect("预览");
    let base = preview
        .url
        .trim_end_matches("__norma_preview__")
        .to_string();
    let get = |url: &str| respond(Request::builder().uri(url).body(Vec::new()).expect("请求"));
    assert_eq!(get(&preview.url).status(), 200, "正文可读");
    assert_eq!(
        get(&format!("{base}image.png")).body(),
        b"image",
        "项目资源可读"
    );
    assert_eq!(get(&format!("{base}escape")).status(), 404, "越界链接拒绝");
    assert_eq!(
        get(&format!("{base}.git/config")).status(),
        404,
        "Git 内部资源拒绝"
    );
    let old = preview.url.clone();
    drop(preview);
    assert_eq!(get(&old).status(), 404, "关闭后撤销资源访问");
}

/// 场景：WebKit 以 Range 请求视频的任意区间。
/// 数据构造：十字节假视频；请求第 2 到 5 字节，总长度为 4。
/// 执行过程：通过预览 URL 同源请求片段。
/// 预期结果：206、正确范围及四字节正文；不把视频解码到 CPU 图片。
#[test]
fn local_video_supports_bounded_range_requests() {
    let temp = tempfile::tempdir().expect("目录");
    std::fs::write(temp.path().join("clip.mp4"), b"0123456789").expect("视频");
    let preview = LocalPreview::new(temp.path(), "clip.mp4", None).expect("预览");
    let url = preview.url.replace("__norma_preview__", "clip.mp4");
    let request = Request::builder()
        .uri(url)
        .header("Range", "bytes=2-5")
        .body(Vec::new())
        .expect("请求");
    let response = respond(request);
    assert_eq!(response.status(), 206, "媒体必须支持分段读取");
    assert_eq!(
        response.headers()["Content-Range"],
        "bytes 2-5/10",
        "分段范围准确"
    );
    assert_eq!(response.body(), b"2345", "只返回请求片段");
}

/// 场景：大视频先以 HEAD 查询长度，再请求尾部索引。
/// 数据构造：40 MiB 稀疏视频；尾部写入四字节标记。
/// 执行过程：HEAD 后请求 bytes=-4。
/// 预期结果：HEAD 不把视频读入内存；尾部区间返回正确内容与范围。
#[test]
fn large_video_supports_head_and_suffix_range() {
    use std::io::{Seek, SeekFrom, Write};
    let temp = tempfile::tempdir().expect("目录");
    let mut file = std::fs::File::create(temp.path().join("large.mp4")).expect("视频");
    let size = 40 * 1024 * 1024;
    file.set_len(size).expect("稀疏文件");
    file.seek(SeekFrom::End(-4)).expect("尾部");
    file.write_all(b"tail").expect("尾部标记");
    let preview = LocalPreview::new(temp.path(), "large.mp4", None).expect("预览");
    let url = preview.url.replace("__norma_preview__", "large.mp4");
    let head = respond(
        Request::builder()
            .method("HEAD")
            .uri(&url)
            .body(Vec::new())
            .expect("HEAD"),
    );
    assert_eq!(head.status(), 200, "大视频支持 HEAD");
    assert_eq!(
        head.headers()["Content-Length"],
        size.to_string(),
        "长度准确"
    );
    assert!(head.body().is_empty(), "HEAD 不读取正文");
    let suffix = respond(
        Request::builder()
            .uri(url)
            .header("Range", "bytes=-4")
            .body(Vec::new())
            .expect("Range"),
    );
    assert_eq!(suffix.status(), 206, "支持尾部 Range");
    assert_eq!(suffix.body(), b"tail", "只读取尾部四字节");
}

/// 场景：成果版本存放在公共临时目录，HTML 不能读取同目录的其他文件。
/// 数据构造：一个临时 HTML 版本和旁边的私有文本。
/// 执行过程：注册 snapshot 预览，再请求邻居文件。
/// 预期结果：版本可查看，访问范围仅包含该版本文件。
#[test]
fn frozen_preview_cannot_read_neighboring_temporary_files() {
    let temp = tempfile::tempdir().expect("目录");
    let path = temp.path().join("result.html");
    std::fs::write(&path, "<h1>frozen</h1>").expect("版本");
    std::fs::write(temp.path().join("private.txt"), "secret").expect("邻居文件");
    let preview = LocalPreview::snapshot(&path, Some("<h1>frozen</h1>")).expect("版本预览");
    let url = preview.url.replace("__norma_preview__", "private.txt");
    let response = respond(Request::builder().uri(url).body(Vec::new()).expect("请求"));
    assert_eq!(response.status(), 404, "版本预览不能扩大为临时目录读取能力");
}
