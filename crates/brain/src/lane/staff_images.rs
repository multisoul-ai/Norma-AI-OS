//! 从幕僚用户消息里取出已上传图片，读入后交给 ACP。只接受内容哈希相符的普通文件。

use crate::acp::PromptImage;
use norma::protocol::{image_id, parse_image_message, Task, TaskMessage};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const LIMIT: u64 = 2 * 1024 * 1024;

pub(super) struct Attachments {
    pub blocks: Vec<PromptImage>,
    pub missing: Vec<&'static str>,
}

pub(super) fn attachments(task: &Task) -> Attachments {
    let Some(text) = task
        .messages
        .iter()
        .rev()
        .find_map(|message| match message {
            TaskMessage::User { text } => Some(text.as_str()),
            _ => None,
        })
    else {
        return Attachments {
            blocks: Vec::new(),
            missing: Vec::new(),
        };
    };
    let Some((_, paths)) = parse_image_message(text) else {
        return Attachments {
            blocks: Vec::new(),
            missing: Vec::new(),
        };
    };
    let mut blocks = Vec::new();
    let mut missing = Vec::new();
    for path in paths {
        match load(&path) {
            Ok(block) => blocks.push(block),
            Err(error) => missing.push(error),
        }
    }
    Attachments { blocks, missing }
}

fn load(path: &str) -> Result<PromptImage, &'static str> {
    let id = image_id(path).ok_or("invalid_image")?;
    let file = Path::new(path);
    let meta = fs::symlink_metadata(file).map_err(|_| "invalid_image")?;
    if !meta.file_type().is_file() || meta.len() == 0 || meta.len() > LIMIT {
        return Err("invalid_image");
    }
    let bytes = fs::read(file).map_err(|_| "invalid_image")?;
    if bytes.len() as u64 != meta.len() || format!("{:x}", Sha256::digest(&bytes)) != id {
        return Err("invalid_image");
    }
    let mime = match file.extension().and_then(|ext| ext.to_str()) {
        Some("png") if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => "image/png",
        Some("jpg") if bytes.starts_with(&[0xff, 0xd8, 0xff]) => "image/jpeg",
        _ => return Err("invalid_image"),
    };
    Ok(PromptImage {
        mime,
        data: base64(&bytes),
    })
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let value = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | bytes[index + 2] as u32;
        for shift in [18, 12, 6, 0] {
            out.push(TABLE[((value >> shift) & 63) as usize] as char);
        }
        index += 3;
    }
    let rest = &bytes[index..];
    if rest.len() == 1 {
        let value = (rest[0] as u32) << 16;
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rest.len() == 2 {
        let value = ((rest[0] as u32) << 16) | ((rest[1] as u32) << 8);
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push(TABLE[((value >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

#[cfg(test)]
#[path = "../../tests/unit/staff_prompt_images_tests.rs"]
mod tests;
