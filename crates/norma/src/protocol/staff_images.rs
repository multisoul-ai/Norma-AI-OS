//! 与手机聊天相同的图片消息：正文后附 `[[norma-images]]` 路径清单。
//! 路径必须落在既有 `.mobile-images` 目录，且文件名是小写内容哈希。

pub const MAX_CHAT_IMAGES: usize = 4;
const OPENING: &str = "[[norma-images]]\n请先读取并查看以下图片，再结合上面的文字回复：\n";
const CLOSING: &str = "\n[[/norma-images]]";

pub fn compose_image_message(text: &str, paths: &[String]) -> Result<String, &'static str> {
    if paths.is_empty() || paths.len() > MAX_CHAT_IMAGES {
        return Err(if paths.is_empty() {
            "invalid_image"
        } else {
            "too_many_images"
        });
    }
    if paths.iter().any(|path| image_id(path).is_none()) {
        return Err("invalid_image");
    }
    let references = serde_json::to_string(paths).map_err(|_| "invalid_image")?;
    Ok(format!("{text}\n\n{OPENING}{references}{CLOSING}"))
}

pub fn parse_image_message(text: &str) -> Option<(String, Vec<String>)> {
    if !text.ends_with(CLOSING) {
        return None;
    }
    let start = text.rfind(OPENING)?;
    let prefix = &text[..start];
    if !(prefix.is_empty() || prefix.ends_with("\n\n")) {
        return None;
    }
    let references = &text[start + OPENING.len()..text.len() - CLOSING.len()];
    let paths: Vec<String> = serde_json::from_str(references).unwrap_or_else(|_| {
        references
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    });
    if paths.is_empty()
        || paths.len() > MAX_CHAT_IMAGES
        || paths.iter().any(|path| image_id(path).is_none())
    {
        return None;
    }
    let visible = if prefix.is_empty() {
        String::new()
    } else {
        prefix[..prefix.len() - 2].to_owned()
    };
    Some((visible, paths))
}

/// 展示用正文。没有图片协议时原样返回，避免把普通消息误判成附件。
pub fn image_message_parts(text: &str) -> (String, Vec<String>) {
    parse_image_message(text).unwrap_or_else(|| (text.to_owned(), Vec::new()))
}

pub fn image_id(path: &str) -> Option<&str> {
    if !path.starts_with('/') || path.len() > 4096 || path.chars().any(char::is_control) {
        return None;
    }
    let mut parts = path.split('/');
    if parts.next() != Some("") {
        return None;
    }
    if !path.contains(".mobile-images/")
        || parts.any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    let file = path.rsplit('/').next()?;
    let (stem, ext) = file.rsplit_once('.')?;
    if ext != "jpg" && ext != "png" {
        return None;
    }
    (stem.len() == 64
        && stem
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then_some(stem)
}

#[cfg(test)]
#[path = "../../tests/unit/staff_image_message_tests.rs"]
mod tests;
