use norma::agent_hook::ClaudeQuestion;
use norma::protocol::{QuestionAnswer, QuestionRef};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/claude-pre-tool-use.json")).unwrap()
}
fn target(raw: &Value) -> QuestionRef {
    serde_json::from_value(json!({"workspace_id":"w","project_id":"p","node_id":42,
        "provider":"Claude","run_id":"r","connection_id":"c","session_id":raw["session_id"],
        "request_id":raw["tool_use_id"],"revision":1}))
    .unwrap()
}

/// 场景：原生Claude回填必须保留原输入，只有对应tool_use_id的消费回执才能确认。
/// 构造：从真实原生探针摘取Pre/PostToolUse，增加不应丢失的metadata字段。
/// 执行：归一化问题，选择第二项Green，再生成Hook输出和核验真实PostToolUse。
/// 预期：原questions/metadata保持、answers精确填入，单独allow或错调用ID不算消费。
#[test]
fn claude_hook_preserves_input_and_requires_matching_tool_result() {
    let mut raw = raw();
    raw["tool_input"]["metadata"] = json!({"keep":"verbatim"});
    let adapter = ClaudeQuestion::from_hook(&raw).expect("真实AskUserQuestion可解析");
    let question = adapter.question(target(&raw)).unwrap();
    let answers = vec![QuestionAnswer {
        key: question.items[0].key.clone(),
        selected: vec![question.items[0].options[1].key.clone()],
        text: None,
    }];
    let output = adapter.answer_output(&answers).unwrap();
    assert_eq!(
        output["hookSpecificOutput"]["updatedInput"]["questions"], raw["tool_input"]["questions"],
        "必须保留完整原问题"
    );
    assert_eq!(
        output["hookSpecificOutput"]["updatedInput"]["metadata"],
        json!({"keep":"verbatim"}),
        "未知输入字段不能丢失"
    );
    assert_eq!(
        output["hookSpecificOutput"]["updatedInput"]["answers"],
        json!({"Which label should the test use?":"Green"}),
        "键必须是完整问题全文"
    );
    let mut post: Value =
        serde_json::from_str(include_str!("fixtures/questions/claude-post-tool-use.json")).unwrap();
    assert!(
        adapter.confirms_answer(&post, &answers),
        "真实同调用同答案回执应确认"
    );
    assert!(
        !adapter.confirms_answer(&json!({"permissionDecision":"allow"}), &answers),
        "仅allow不等于完成问答"
    );
    post["tool_use_id"] = json!("another-invocation");
    assert!(
        !adapter.confirms_answer(&post, &answers),
        "另一次调用不能确认本题"
    );
}
