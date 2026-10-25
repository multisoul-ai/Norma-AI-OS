//! Token-scoped local preview resources. No filesystem/Git bridge is exposed to pages.
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};
use workspace::content::Project;
use wry::http::{Request, Response};

type Registry = Mutex<HashMap<String, Arc<Source>>>;
static SOURCES: OnceLock<Registry> = OnceLock::new();

struct Source {
    project: Project,
    html: String,
    only_file: Option<String>,
    image_mime: Option<(String, &'static str)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Text,
    Markdown,
    Html,
    Image,
    Video,
    Other,
}

pub(crate) fn kind(path: &str) -> Kind {
    match Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "md" | "markdown" => Kind::Markdown,
        "html" | "htm" => Kind::Html,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "bmp" | "tif" | "tiff" => Kind::Image,
        "mp4" | "m4v" | "mov" | "webm" => Kind::Video,
        "pdf" | "zip" | "docx" | "xlsx" | "pptx" | "dmg" | "exe" | "a" | "o" => Kind::Other,
        _ => Kind::Text,
    }
}

pub(crate) fn preview_kind(path: &str, explicit: Option<canvas::PreviewType>) -> Kind {
    match explicit {
        Some(canvas::PreviewType::Markdown) => Kind::Markdown,
        Some(canvas::PreviewType::Text) => Kind::Text,
        Some(canvas::PreviewType::Image) => Kind::Image,
        None => kind(path),
    }
}

pub(crate) struct LocalPreview {
    token: String,
    pub url: String,
}

impl LocalPreview {
    pub fn new(root: &Path, path: &str, text: Option<&str>) -> Result<Self, String> {
        Self::create(root, path, text, false, kind(path))
    }

    pub fn snapshot(path: &Path, text: Option<&str>) -> Result<Self, String> {
        let parent = path.parent().ok_or("版本文件缺少父目录")?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("版本文件名无效")?;
        Self::create(parent, name, text, true, kind(name))
    }

    /// A Present target is already canonical and authorized as a file, including outside the project.
    pub(crate) fn presented(
        path: &Path,
        text: Option<&str>,
        preview_type: canvas::PreviewType,
    ) -> Result<Self, String> {
        let parent = path.parent().ok_or("预览文件缺少父目录")?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("预览文件名无效")?;
        Self::create(
            parent,
            name,
            text,
            true,
            preview_kind(name, Some(preview_type)),
        )
    }

    fn create(
        root: &Path,
        path: &str,
        text: Option<&str>,
        isolated: bool,
        preview_kind: Kind,
    ) -> Result<Self, String> {
        let project = Project::open(root)?;
        // Validate before registering a URL, including symlink targets.
        let target = project.resolve(Path::new(path))?;
        if !target.starts_with(project.root()) {
            return Err("文件位于项目外部".into());
        }
        let image_mime = if preview_kind == Kind::Image {
            let mut prefix = Vec::new();
            std::fs::File::open(&target)
                .map_err(|error| error.to_string())?
                .take(512)
                .read_to_end(&mut prefix)
                .map_err(|error| error.to_string())?;
            let mime = image::guess_format(&prefix)
                .ok()
                .map(|format| format.to_mime_type())
                .or_else(|| {
                    std::str::from_utf8(&prefix)
                        .ok()
                        .filter(|text| text.contains("<svg"))
                        .map(|_| "image/svg+xml")
                });
            mime.map(|mime| (path.to_string(), mime))
        } else {
            None
        };
        let token = uuid::Uuid::new_v4().simple().to_string();
        let mut url =
            url::Url::parse(&format!("norma-content://{token}/")).map_err(|e| e.to_string())?;
        url.path_segments_mut().map_err(|_| "本地地址无效")?.extend(
            Path::new(path)
                .iter()
                .map(|s| s.to_string_lossy().into_owned()),
        );
        let asset_url = url.to_string();
        let body = match preview_kind {
            Kind::Markdown => {
                let options = pulldown_cmark::Options::ENABLE_TABLES
                    | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
                    | pulldown_cmark::Options::ENABLE_TASKLISTS;
                let parser = pulldown_cmark::Parser::new_ext(text.unwrap_or_default(), options)
                    .map(|e| match e {
                        pulldown_cmark::Event::Html(t) | pulldown_cmark::Event::InlineHtml(t) => {
                            pulldown_cmark::Event::Text(t)
                        }
                        other => other,
                    });
                let mut html = String::new();
                pulldown_cmark::html::push_html(&mut html, parser);
                format!("<article>{html}</article>")
            }
            Kind::Html => text.unwrap_or_default().to_string(),
            Kind::Image => format!(
                r#"<nav><button onclick="fit()">适应窗口</button><button onclick="actual()">实际尺寸</button><button onclick="scale(1.25)">放大</button><button onclick="scale(.8)">缩小</button></nav><main id="viewport"><img id="image" src="{}"></main><script>let z=1;function fit(){{z=1;image.style='max-width:100%;max-height:100%;object-fit:contain'}}function actual(){{z=1;image.style='max-width:none;max-height:none;width:auto;height:auto'}}function scale(f){{z*=f;image.style='max-width:none;max-height:none;width:'+(image.naturalWidth*z)+'px'}}let down=null;viewport.onpointerdown=e=>{{down=[e.clientX,e.clientY,viewport.scrollLeft,viewport.scrollTop];viewport.setPointerCapture(e.pointerId)}};viewport.onpointermove=e=>{{if(down){{viewport.scrollLeft=down[2]+down[0]-e.clientX;viewport.scrollTop=down[3]+down[1]-e.clientY}}}};viewport.onpointerup=()=>down=null;</script>"#,
                escape(&asset_url)
            ),
            Kind::Video => format!(
                r#"<video controls playsinline preload="metadata" src="{}"></video><script>document.addEventListener('visibilitychange',()=>{{if(document.hidden)document.querySelector('video').pause()}});window.addEventListener('blur',()=>document.querySelector('video').pause());</script>"#,
                escape(&asset_url)
            ),
            Kind::Other => return Err("此格式暂不支持内嵌预览，请使用系统应用打开".into()),
            Kind::Text => return Err("纯文本使用编辑器查看".into()),
        };
        let base = url.join(".").map_err(|e| e.to_string())?;
        let html = format!(
            r#"<!doctype html><html><head><meta charset="utf-8"><meta name="color-scheme" content="light dark"><base href="{}"><style>body{{margin:0;font:14px -apple-system;color-scheme:light dark}}article{{padding:20px;line-height:1.65;overflow-wrap:anywhere}}pre{{overflow:auto;padding:12px;background:#8882}}table{{border-collapse:collapse}}td,th{{border:1px solid #8886;padding:6px}}article img{{max-width:100%}}video{{width:100vw;height:100vh;background:#111}}nav{{height:36px;display:flex;gap:6px;align-items:center;padding:0 8px}}main{{height:calc(100vh - 36px);overflow:auto;display:grid;place-items:center;cursor:grab}}main img{{max-width:100%;max-height:100%;user-select:none;-webkit-user-drag:none}}button{{font:inherit}}</style></head><body>{body}</body></html>"#,
            escape(base.as_str())
        );
        SOURCES
            .get_or_init(Default::default)
            .lock()
            .map_err(|e| e.to_string())?
            .insert(
                token.clone(),
                Arc::new(Source {
                    project,
                    html,
                    only_file: isolated.then(|| path.to_string()),
                    image_mime,
                }),
            );
        Ok(Self {
            url: format!("norma-content://{token}/__norma_preview__"),
            token,
        })
    }
}

impl Drop for LocalPreview {
    fn drop(&mut self) {
        if let Some(sources) = SOURCES.get() {
            if let Ok(mut sources) = sources.lock() {
                sources.remove(&self.token);
            }
        }
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) fn respond(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    match resource(&request) {
        Ok(response) => response,
        Err(error) => Response::builder()
            .status(404)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(error.into_bytes())
            .expect("valid response"),
    }
}

fn resource(request: &Request<Vec<u8>>) -> Result<Response<Vec<u8>>, String> {
    if !matches!(request.method().as_str(), "GET" | "HEAD") {
        return Err("只允许读取".into());
    }
    let url = url::Url::parse(&request.uri().to_string()).map_err(|e| e.to_string())?;
    let source = SOURCES
        .get_or_init(Default::default)
        .lock()
        .map_err(|e| e.to_string())?
        .get(url.host_str().unwrap_or_default())
        .cloned()
        .ok_or("预览已关闭")?;
    if url.path() == "/__norma_preview__" {
        return Response::builder().header("Content-Type", "text/html; charset=utf-8").header("Cache-Control", "no-store").header("Content-Security-Policy", "default-src 'self' data: blob: http: https:; script-src 'self' 'unsafe-inline' http: https:; style-src 'self' 'unsafe-inline' http: https:; object-src 'none'; frame-src 'none'").body(source.html.as_bytes().to_vec()).map_err(|e| e.to_string());
    }
    // Decode exactly once using file URL conversion; Project rejects traversal and .git.
    let decoded = url::Url::parse(&format!("file://{}", url.path()))
        .map_err(|e| e.to_string())?
        .to_file_path()
        .map_err(|_| "无效资源地址")?;
    let relative = decoded.strip_prefix("/").map_err(|_| "无效资源路径")?;
    if source
        .only_file
        .as_ref()
        .is_some_and(|allowed| relative.to_string_lossy() != allowed.as_str())
    {
        return Err("成果版本不包含此附属资源".into());
    }
    let target = source.project.resolve(relative)?;
    if !target.starts_with(source.project.root())
        || relative.components().any(|c| c.as_os_str() == ".git")
    {
        return Err("资源不在项目内".into());
    }
    let mut file = std::fs::File::open(&target).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("不是普通文件".into());
    }
    let size = metadata.len();
    let mut status = 200;
    let mut start = 0;
    let mut end = size.saturating_sub(1);
    if let Some(range) = request.headers().get("Range").and_then(|h| h.to_str().ok()) {
        let (from, to) = range
            .strip_prefix("bytes=")
            .and_then(|s| s.split_once('-'))
            .ok_or("无效范围")?;
        if from.is_empty() {
            let suffix = to.parse::<u64>().map_err(|_| "无效范围")?;
            if suffix == 0 {
                return Err("无效范围".into());
            }
            start = size.saturating_sub(suffix);
        } else {
            start = from.parse::<u64>().map_err(|_| "无效范围")?;
            if !to.is_empty() {
                end = to.parse::<u64>().map_err(|_| "无效范围")?.min(end);
            }
        }
        end = end.min(start.saturating_add(4 * 1024 * 1024 - 1));
        if start >= size || start > end {
            return Err("资源范围已改变".into());
        }
        status = 206;
    } else if request.method() != "HEAD" && size > 32 * 1024 * 1024 {
        return Err("资源过大，请使用系统应用打开".into());
    }
    let length = if size == 0 { 0 } else { end - start + 1 };
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    if request.method() != "HEAD" {
        file.take(length)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
    }
    let ext = target
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    };
    let mime = source
        .image_mime
        .as_ref()
        .filter(|(path, _)| relative.to_string_lossy() == path.as_str())
        .map(|(_, mime)| *mime)
        .unwrap_or(mime);
    let mut response = Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Content-Length", length)
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff");
    if status == 206 {
        response = response.header("Content-Range", format!("bytes {start}-{end}/{size}"));
    }
    response.body(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "../../tests/unit/local_content_tests.rs"]
mod tests;
