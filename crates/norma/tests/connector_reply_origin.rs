use norma::agent_hook::{extract_prompt_fingerprint, extract_prompt_text};
use serde_json::json;
/// 场景：Hook展示标题截断到60字，来源指纹必须区分同前缀的不同完整消息。
/// 构造：60个相同字符后分别加A/B；执行标题/指纹提取与备用键读取。
/// 预期：标题相同、指纹不同，备用键同正文产生同指纹，不存在正文返回None。
#[test]
fn reply_origin_fingerprints_use_full_prompt_not_display_title() {
    let a = json!({"prompt":format!("{}A","前".repeat(60))});
    let b = json!({"prompt":format!("{}B","前".repeat(60))});
    assert_eq!(
        extract_prompt_text(&a),
        extract_prompt_text(&b),
        "确实构造了同展示标题"
    );
    assert_ne!(
        extract_prompt_fingerprint(&a),
        extract_prompt_fingerprint(&b),
        "完整消息不同不能吞掉用户输入"
    );
    assert_eq!(
        extract_prompt_fingerprint(&a),
        extract_prompt_fingerprint(&json!({"user_message":a["prompt"]})),
        "已支持的备用键保持来源"
    );
    assert_eq!(
        extract_prompt_fingerprint(&json!({})),
        None,
        "缺失正文不猜测来源"
    );
}
