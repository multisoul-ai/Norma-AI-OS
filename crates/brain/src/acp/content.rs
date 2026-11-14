//! ACP `session/prompt` 内容块。图片字节只放在 image 块里，不写进文本轨迹。

use serde_json::{json, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptImage {
    pub mime: &'static str,
    pub data: String,
}

pub fn session_prompt(session_id: &str, text: &str, images: &[PromptImage]) -> Value {
    let mut prompt = vec![json!({"type": "text", "text": text})];
    for image in images {
        prompt.push(json!({
            "type": "image",
            "mimeType": image.mime,
            "data": image.data,
        }));
    }
    json!({"sessionId": session_id, "prompt": prompt})
}

#[cfg(test)]
#[path = "../../tests/unit/acp_prompt_content_tests.rs"]
mod tests;
