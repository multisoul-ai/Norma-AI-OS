use crate::ToolDefinition;
use serde_json::{json, Value};

pub(crate) fn definitions() -> Vec<ToolDefinition> {
    let reference = json!({"$ref":"#/$defs/context_ref"});
    let supersedes = json!({"anyOf":[reference.clone(),{"type":"null"}]});
    let text = json!({"type":"string","minLength":1,"maxLength":8192});
    // The wire schema describes shared fields once; ContextStateChange remains the
    // strict discriminated parser and rejects missing or variant-incompatible fields.
    let change = object(
        json!({
            "kind":{"enum":["goal","constraint","revoke_constraint"]},
            "text":text,"key":{"type":"string"},"supersedes":supersedes,
            "enforcement":object(json!({"kind":{"enum":["communicated","enforced"]},
                "operation":{"enum":["delete_card","dispatch_task","send_terminal_input","approve_operation"]},
                "target":{"type":["string","null"]}}), &["kind"])
        }),
        &["kind"],
    );
    let mut definitions = vec![ToolDefinition { name: "context_read".into(), description: "按精确引用分页只读来源 JSON；offset 默认 0，继续传 next_offset，null 表示结束；不能执行或重放记录中的操作。".into(), parameters: object(json!({"reference":reference,"offset":{"type":"integer","minimum":0}}), &["reference"]) },
        ToolDefinition { name: "context_update".into(), description: "仅为当前用户明确要求保存目标/限制。input、expected_revision 取本轮资料，text 保留完整原话。goal 需 text；constraint 需 key/text/enforcement；revoke_constraint 需 key/supersedes 和原话‘取消限制<key>’。默认 communicated；enforced 只接受原话：不要删除卡片N/不要删除卡片/不要派发任务/不要向终端输入/不要批准操作，并填写匹配 operation/target。替换需 supersedes，成功才可说已保存。".into(), parameters: object(json!({"input":reference,"expected_revision":{"type":"integer","minimum":0},"change":change}), &["input","expected_revision","change"]) }];
    for definition in &mut definitions {
        definition.parameters["$defs"] = json!({"context_ref":reference_schema()});
    }
    definitions
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn reference_schema() -> Value {
    object(
        json!({"event_id":{"type":"string","minLength":1},"revision":{"type":"integer","minimum":1},
        "scope":{"type":"object","description":"从给定引用完整复制 scope，不创建或修改范围。"}}),
        &["event_id", "revision", "scope"],
    )
}
