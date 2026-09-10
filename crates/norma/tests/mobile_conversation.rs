use norma::mobile::ConversationPage;
use serde_json::json;

/// 手机历史保持消息身份、正文和工具内容；Codex 倒序回合须转为页内正序，不能用摘要替代正文。
///
/// 数据构造：Codex 两回合 newest/oldest，各一个回复，最新正文 2000 字超过旧 1200 字摘要；带工具输出及 older 游标。
/// 执行过程：解析 full 历史 → 检查顺序 / 完整正文 / 工具字段 → 重复消息 ID 与缺少 items 的不完整页。
/// 预期结果：稳定 ID 和完整正文保留；工具可展开；不完整或重复页必须报错，不能静默漏历史。
#[test]
fn codex_full_history_preserves_content_and_orders_turns() {
    let body = "长".repeat(2000);
    let raw = json!({"data":[
        {"id":"new","itemsView":"full","items":[{"type":"agentMessage","id":"b","text":body},{"type":"commandExecution","id":"tool","command":"pwd","aggregatedOutput":"/project"}]},
        {"id":"old","itemsView":"full","items":[{"type":"agentMessage","id":"a","text":"旧回复"}]}
    ],"nextCursor":"older"});
    let page = ConversationPage::from_codex(raw.clone()).expect("完整页");
    assert_eq!(page.items[0].id, "old:a", "旧回合在前");
    assert_eq!(page.items[1].text, body, "正文不能截成摘要");
    assert!(
        page.items[2]
            .details
            .as_deref()
            .is_some_and(|d| d.contains("/project")),
        "保留完整工具输出"
    );
    assert_eq!(page.cursor, Some("\"older\"".into()), "游标必须原样保留");
    let mut missing = raw.clone();
    missing["data"][0]["itemsView"] = json!("summary");
    assert!(
        ConversationPage::from_codex(missing).is_err(),
        "摘要页不能冒充完整历史"
    );
    let mut duplicate = raw;
    duplicate["data"][0]["items"][1]["id"] = json!("b");
    assert!(
        ConversationPage::from_codex(duplicate).is_err(),
        "同回合重复 ID 不能默默覆盖消息"
    );
}

/// Claude 对话转换保留完整消息、工具与半行游标；非法角色不能进入手机历史。
///
/// 数据构造：3000 个中文字 = 9000 UTF-8 字节；工具块与文本共存，第三行先缺换行再补齐。
/// 执行过程：实际读取两行加半行 → 转换 → 序列化游标增量读 → 补换行 → 再读 → 伪造未知角色。
/// 预期结果：正文不缩成摘要，工具 JSON 保留；半行不推进或重复旧消息；补齐仅返回第三行，非法角色拒绝。
#[test]
fn claude_full_history_keeps_tools_and_resumes_incomplete_tail() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("隔离对话目录");
    let path = dir.path().join("session-a.jsonl");
    let body = "长".repeat(3000);
    let message = |id: &str, role: &str, content| json!({"type":role,"uuid":id,"sessionId":"session-a","cwd":dir.path(),"message":{"role":role,"content":content}});
    let user = message("u1", "user", json!("任务"));
    let answer = message(
        "a1",
        "assistant",
        json!([{"type":"text","text":body},{"type":"tool_use","name":"Bash","input":{"command":"pwd"}}]),
    );
    let last = message("a2", "assistant", json!("后续"));
    std::fs::write(&path, format!("{user}\n{answer}\n{last}")).expect("完整 JSON 尾部缺换行");
    // 20 records per page matches the live mobile endpoint, exceeding our two complete records.
    let raw = norma::claude::read_transcript(&path, dir.path(), "session-a", dir.path(), None, 20)
        .expect("真实 JSONL");
    let page = ConversationPage::from_claude(raw.clone()).expect("手机完整页");
    assert_eq!(page.items.len(), 2, "不包含半行 a2");
    assert_eq!(page.items[0].id, "u1", "保留原消息顺序");
    assert_eq!(page.items[1].text, body, "长回复不能截短");
    assert!(
        page.items[1]
            .details
            .as_ref()
            .is_some_and(|value| value.contains("pwd")),
        "混合文本和工具内容均保留"
    );
    assert!(page.pending_tail, "手机应继续等待尾行补齐");
    assert!(!page.has_more, "半行不能触发无延迟翻页");
    assert_eq!(page.direction, "newer", "Claude 游标向后追加");
    let cursor =
        serde_json::from_str(page.cursor.as_deref().expect("不透明游标")).expect("解码游标");
    writeln!(std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("追加"))
    .expect("补换行");
    let next = ConversationPage::from_claude(
        norma::claude::read_transcript(
            &path,
            dir.path(),
            "session-a",
            dir.path(),
            Some(cursor),
            20,
        )
        .expect("增量读取"),
    )
    .expect("增量页");
    assert_eq!(next.items.len(), 1, "不能重放 u1/a1");
    assert_eq!(next.items[0].id, "a2", "只追加完整新消息");
    assert!(!next.pending_tail, "换行后不再等待");
    let mut invalid = raw;
    invalid.messages[0].role = "unknown".into();
    assert!(
        ConversationPage::from_claude(invalid).is_err(),
        "未知角色不能形成无效手机页"
    );
}
