use super::{session_prompt, PromptImage};

/// 场景：幕僚把已上传图片作为 ACP image 块发给模型，文本块保持原样。
/// 数据构造：会话 s、文本 hi、一张 image/png 的 base64 `YQ==`。
/// 执行过程：组装 session/prompt 参数。
/// 预期结果：第一块仍是原文；第二块才是图片；文本块不含 base64。
#[test]
fn session_prompt_keeps_image_bytes_out_of_the_text_block() {
    let value = session_prompt(
        "s",
        "hi",
        &[PromptImage {
            mime: "image/png",
            data: "YQ==".into(),
        }],
    );
    assert_eq!(value["sessionId"], "s", "会话 id 不能丢");
    assert_eq!(value["prompt"][0]["type"], "text", "第一条必须是文本");
    assert_eq!(value["prompt"][0]["text"], "hi", "文本块应保持原文");
    assert_eq!(value["prompt"][1]["type"], "image", "图片必须是独立内容块");
    assert_eq!(value["prompt"][1]["mimeType"], "image/png", "要标明 png");
    assert_eq!(value["prompt"][1]["data"], "YQ==", "base64 只出现在图片块");
    assert!(
        !value["prompt"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("YQ=="),
        "文本轨迹不能夹带图片字节"
    );
}
