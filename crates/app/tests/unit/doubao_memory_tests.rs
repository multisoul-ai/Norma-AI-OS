use super::plan::chat;
use serde_json::json;

/// 豆包请求扣除目录、工具与回复预留，并以完整回合裁剪旧历史。
///
/// 数据构造（含关键数值的推导过程）：
///   request_budget = 224,000 tokens × 2 B - 8,192 B = 439,808 B。
///   旧 user = (request_budget / 3 + 1) 个“旧” × 3 B = 439,809 B，正文已超过预算。
///   历史 = 旧 user + assistant(old) + tool(old) + 当前 user“本轮”，共 4 条。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造个人请求，完整旧回合使序列化请求超过模型输入预算。
///   2. 删除旧 user 及其 assistant/tool 配对，保留当前 user。
///   3. 读取最终 HTTP 正文和个人工具列表。
///
/// 预期结果：
///   - 正断言：本轮保留，个人只声明 4 种记忆工具和 2 种上下文工具。
///   - 负断言：旧长文、旧 tool_call 与旧 tool_result 都不存在。
#[test]
fn voice_memory_request_prunes_complete_turns() {
    // “旧”占 3 B，向下取整后 +1，保证正文严格大于输入预算。
    let old_chars = chat::REQUEST_INPUT_BYTES / "旧".len() + 1;
    let mut history = vec![
        json!({"role":"user","content":"旧".repeat(old_chars)}),
        json!({"role":"assistant","tool_calls":[{"id":"old","function":{"name":"memory_read","arguments":"{}"}}]}),
        json!({"role":"tool","tool_call_id":"old","content":"旧结果"}),
        json!({"role":"user","content":"本轮"}),
    ];
    let body = chat::bounded_request("当前记忆目录", &mut history, true, true).expect("应容纳本轮");
    let messages = body["messages"].to_string();
    assert!(messages.contains("本轮"), "当前回合不可裁掉");
    assert!(!messages.contains("旧结果"), "旧工具结果必须随回合删除");
    assert!(!messages.contains("old"), "不应留下孤立tool_call");
    assert!(!messages.contains("旧旧"), "旧 user 长文必须随整个回合删除");
    let mut names: Vec<_> = body["tools"]
        .as_array()
        .expect("工具")
        .iter()
        .map(|tool| tool["function"]["name"].as_str().expect("工具名称"))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "context_read",
            "context_update",
            "memory_list",
            "memory_read",
            "memory_search",
            "memory_write"
        ],
        "个人只允许记忆与共享上下文，不能开放项目工具"
    );
}
