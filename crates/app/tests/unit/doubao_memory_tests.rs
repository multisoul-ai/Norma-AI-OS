use super::plan::chat;
use serde_json::json;

/// 豆包请求扣除目录、工具与回复预留，并以完整回合裁剪旧历史。
///
/// 数据构造：旧 user 正文26000字节；旧工具配对；当前 user 为“本轮”。
///   16K tokens×2字节 - reply(1024×2) - tool(8192) - system/schema < 26000。
/// 执行过程：构造有记忆目录的个人请求→裁剪完整旧回合→读取最终HTTP正文。
/// 预期结果：本轮存在；旧长文和孤立tool不存在，个人只有4种记忆工具。
#[test]
fn voice_memory_request_prunes_complete_turns() {
    let mut history = vec![
        json!({"role":"user","content":"旧".repeat(9000)}),
        json!({"role":"assistant","tool_calls":[{"id":"old","function":{"name":"memory_read","arguments":"{}"}}]}),
        json!({"role":"tool","tool_call_id":"old","content":"旧结果"}),
        json!({"role":"user","content":"本轮"}),
    ];
    let body = chat::bounded_request("当前记忆目录", &mut history, true, true).expect("应容纳本轮");
    let messages = body["messages"].to_string();
    assert!(messages.contains("本轮"), "当前回合不可裁掉");
    assert!(!messages.contains("旧结果"), "旧工具结果必须随回合删除");
    assert!(!messages.contains("old"), "不应留下孤立tool_call");
    assert_eq!(
        body["tools"].as_array().expect("工具").len(),
        4,
        "个人仅限记忆工具"
    );
}
