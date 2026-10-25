use super::*;
use serde_json::json;

fn item(update: serde_json::Value) -> TraceItem {
    classify_update(&update).expect("应当被记录")
}

#[test]
fn reply_chunk_is_recorded_as_reply() {
    let got = item(
        json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "你好"}}),
    );
    assert_eq!(got.kind, TraceKind::Reply);
    assert_eq!(got.text, "你好");
}

#[test]
fn thought_chunk_is_recorded_as_thought() {
    let got = item(
        json!({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": "先看看有没有空闲的"}}),
    );
    assert_eq!(got.kind, TraceKind::Thought);
    assert_eq!(got.text, "先看看有没有空闲的");
}

#[test]
fn tool_call_keeps_title_and_input() {
    let got = item(json!({
        "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "norma brain agents",
        "kind": "execute", "status": "pending", "rawInput": {"command": "norma brain agents"}
    }));
    assert_eq!(got.kind, TraceKind::ToolCall);
    assert!(got.title.contains("norma brain agents"));
    assert!(
        got.text.contains("\"command\""),
        "参数要原样保留：{}",
        got.text
    );
}

#[test]
fn tool_result_keeps_status_and_output() {
    let got = item(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed",
        "content": [{"type": "content", "content": {"type": "text", "text": "{\"agents\":[]}"}}]
    }));
    assert_eq!(got.kind, TraceKind::ToolResult);
    assert!(got.title.contains("completed"));
    assert!(got.text.contains("agents"));
}

#[test]
fn plan_lists_every_entry_with_its_status() {
    let got = item(json!({
        "sessionUpdate": "plan",
        "entries": [{"content": "读目录", "status": "completed"}, {"content": "写总结", "status": "pending"}]
    }));
    assert_eq!(got.kind, TraceKind::Plan);
    assert!(got.text.contains("[completed] 读目录"));
    assert!(got.text.contains("[pending] 写总结"));
}

#[test]
fn unknown_update_is_kept_as_raw_json() {
    let got = item(json!({"sessionUpdate": "brand_new_kind", "payload": 7}));
    assert_eq!(got.kind, TraceKind::Unknown);
    assert!(got.title.contains("brand_new_kind"));
    assert!(got.text.contains("\"payload\""));
}

#[test]
fn unknown_update_text_is_capped() {
    let big = "x".repeat(10_000);
    let got = item(json!({"sessionUpdate": "brand_new_kind", "payload": big}));
    assert!(got.text.chars().count() <= RAW_LIMIT + 1, "太长要截断");
}

#[test]
fn update_without_a_type_is_ignored() {
    assert!(classify_update(&json!({"content": {"text": "x"}})).is_none());
}

fn reply(text: &str) -> TraceItem {
    TraceItem::new(TraceKind::Reply, "", text)
}

/// 一轮结束条目携带结构化的停止原因与用时，持久化不必再解析中文文案。
#[test]
fn turn_end_item_carries_stop_reason_and_elapsed() {
    let item = TraceItem::turn_end("end_turn", 1234);
    assert_eq!(item.kind, TraceKind::Turn);
    assert_eq!(
        item.end,
        Some(TurnEnd {
            stop_reason: "end_turn".into(),
            elapsed_ms: 1234
        })
    );
    assert!(item.text.contains("end_turn") && item.text.contains("1234"));
    assert_eq!(TraceItem::new(TraceKind::Reply, "", "x").end, None);
}

#[test]
fn consecutive_chunks_merge_into_one_item() {
    let mut merger = Merger::default();
    assert!(merger.push(reply("你")).is_empty());
    assert!(merger.push(reply("好")).is_empty());
    let flushed = merger.flush().expect("还有待输出的内容");
    assert_eq!(flushed.text, "你好");
    assert!(merger.flush().is_none());
}

#[test]
fn a_different_kind_flushes_the_pending_text_first() {
    let mut merger = Merger::default();
    merger.push(reply("先说一句"));
    let tool = TraceItem::new(TraceKind::ToolCall, "t", "");
    let out = merger.push(tool.clone());
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].text, "先说一句");
    assert_eq!(out[1], tool, "工具调用紧跟在前面的文字之后，顺序不变");
}

#[test]
fn reply_and_thought_do_not_merge_with_each_other() {
    let mut merger = Merger::default();
    merger.push(TraceItem::new(TraceKind::Thought, "", "想"));
    let out = merger.push(reply("说"));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, TraceKind::Thought);
}

fn unknown(title: &str, text: &str) -> TraceItem {
    TraceItem::new(TraceKind::Unknown, title, text)
}

/// 真实适配器每个 token 批次发一次 usage_update，一轮能有十几条；相邻的同类状态更新只留最新一条。
///
/// 数据构造：连续三条 usage_update，中间不夹别的内容。
/// 预期结果：
///   - 正断言：flush 得到一条，内容是最后一次
///   - 负断言：被替换的旧值不再出现
#[test]
fn consecutive_status_updates_keep_only_the_latest() {
    let mut merger = Merger::default();
    assert!(merger
        .push(unknown("usage_update", "{\"used\":1}"))
        .is_empty());
    assert!(merger
        .push(unknown("usage_update", "{\"used\":2}"))
        .is_empty());
    assert!(merger
        .push(unknown("usage_update", "{\"used\":3}"))
        .is_empty());
    let last = merger.flush().expect("还剩一条");
    assert_eq!(last.text, "{\"used\":3}");
    assert!(merger.flush().is_none());
}

/// 不同标题的未知更新互不替换，且按出现顺序输出。
#[test]
fn different_unknown_updates_are_not_collapsed() {
    let mut merger = Merger::default();
    merger.push(unknown("usage_update", "u"));
    let out = merger.push(unknown("available_commands_update", "c"));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].title, "usage_update");
    assert_eq!(merger.flush().unwrap().title, "available_commands_update");
}
