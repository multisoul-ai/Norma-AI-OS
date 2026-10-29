use agent::context::*;

fn record() -> RecordedContextEvent {
    let scope = ContextScope {
        workspace_id: "w".into(),
        area_id: "a".into(),
        project: ContextProject::Personal,
        task_id: None,
        conversation_id: None,
    };
    RecordedContextEvent {
        reference: ContextRef {
            scope: scope.clone(),
            event_id: "r".into(),
            revision: 1,
        },
        event: ContextEvent {
            event_id: "r".into(),
            scope,
            source: ContextSource {
                origin: ContextOrigin::Voice,
                stream_id: "s".into(),
                sequence: 1,
            },
            created_at_ms: 1,
            related: vec![],
            payload: ContextPayload::UserInput {
                text: "引用\"\\你好".repeat(2000),
            },
        },
    }
}

/// 场景：带大量转义与中文的长来源必须分页，且每页完整回执不能超过预留。
/// 构造：已认证来源事件，正文包含引号、反斜杠与多字节中文。
/// 执行：连续使用 next_offset 补读，按实际 AdapterResult 序列化计费并重组事件。
/// 预期：每页不超过 8192 字节、来源一致、偏移递增，重组后原始事件完整相等。
#[test]
fn shared_source_pages_bound_the_entire_receipt_and_preserve_source() {
    let record = record();
    let mut offset = 0;
    let mut text = String::new();
    loop {
        let result = context_read_result(&record, offset).unwrap();
        let wire = serde_json::json!({"role":"tool","tool_call_id":"c".repeat(128),"content":serde_json::to_string(&result).unwrap()});
        assert!(
            wire.to_string().len() <= agent::MEMORY_TOOL_BYTES,
            "实际 tool.content 外层转义也必须计入回执预算"
        );
        let page = result.snapshot.unwrap();
        assert_eq!(
            page["reference"],
            serde_json::to_value(&record.reference).unwrap(),
            "每页保留精确来源"
        );
        assert_eq!(page["offset"], offset, "返回实际字节偏移");
        text.push_str(page["json_fragment"].as_str().unwrap());
        let Some(next) = page["next_offset"].as_u64() else {
            break;
        };
        assert!(next as usize > offset, "分页必须推进，不能陷入空页循环");
        offset = next as usize;
    }
    assert_eq!(
        serde_json::from_str::<RecordedContextEvent>(&text).unwrap(),
        record,
        "分页可重组为完整来源事件"
    );
}

/// 场景：模型传入超出正文或切进 UTF-8 字符的偏移不能读取错误片段。
/// 构造：已认证来源事件与无效的页偏移。
/// 执行：请求越界偏移和中文字符内部偏移。
/// 预期：两种无效页均明确失败，不返回误导性的空成功。
#[test]
fn shared_source_pages_reject_invalid_offsets() {
    let record = record();
    let text = serde_json::to_string(&record).unwrap();
    assert!(
        context_read_result(&record, text.len() + 1).is_err(),
        "越界偏移必须拒绝"
    );
    assert!(
        context_read_result(&record, text.find('你').unwrap() + 1).is_err(),
        "不能截断 UTF-8 字符"
    );
}

/// 场景：高转义膨胀的页面不能因为缩减过头而错误报告元数据超预算。
/// 构造：合法的 8192 字节反斜杠报告；从其序列化正文中间开始读取。
/// 执行：请求 offset=2000 的分页，并检查真实 tool 消息的大小。
/// 预期：返回非空且有界的页面，可用 next_offset 继续，不能因保守缩减误拒绝。
#[test]
fn highly_escaped_sources_still_produce_a_nonempty_page() {
    let mut record = record();
    record.event.payload = ContextPayload::Report {
        text: "\\".repeat(8192),
    };
    let result = context_read_result(&record, 2000);
    assert!(result.is_ok(), "合法长报告应可分页：{result:?}");
    let result = result.unwrap();
    let wire = serde_json::json!({"role":"tool","tool_call_id":"c".repeat(128),"content":serde_json::to_string(&result).unwrap()});
    assert!(
        wire.to_string().len() <= agent::MEMORY_TOOL_BYTES,
        "实际工具消息含所有转义后有界"
    );
    assert!(
        result.snapshot.unwrap()["next_offset"]
            .as_u64()
            .is_some_and(|next| next > 2000),
        "分页必须前进"
    );
}
