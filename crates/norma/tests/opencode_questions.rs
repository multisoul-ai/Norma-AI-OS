use norma::opencode::questions::OpenCodeQuestion;
use norma::protocol::{AgentProfile, QuestionAnswer, QuestionRef, QuestionRequestId};
use serde_json::{json, Value};

fn request() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/opencode-request.json")).unwrap()
}
fn consumed() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/opencode-consumed.json")).unwrap()
}
fn answers() -> Vec<QuestionAnswer> {
    vec![
        QuestionAnswer {
            key: "question-2".into(),
            selected: vec![],
            text: Some("NATIVE_OPENCODE_7F2".into()),
        },
        QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-1".into(),
            selected: vec!["option-0".into(), "option-2".into()],
            text: None,
        },
    ]
}
fn target(raw: &Value) -> QuestionRef {
    QuestionRef {
        workspace_id: "w".into(),
        project_id: "p".into(),
        node_id: 7,
        provider: AgentProfile::OpenCode,
        run_id: "run".into(),
        connection_id: "owned-instance".into(),
        session_id: raw["sessionID"].as_str().unwrap().into(),
        turn_id: None,
        request_id: QuestionRequestId::String(raw["id"].as_str().unwrap().into()),
        revision: 1,
    }
}

/// 场景：OpenCode以原题顺序接受单选、多选与自定义，消费凭据必须来自原工具。
/// 构造：M0真实binary录制的3题和同messageID/callID完成part；规范答案刻意打乱题序。
/// 执行：转换题面、生成reply，再分别检查精确消费、错误工具、错误会话与仅HTTP成功。
/// 预期：输出严格按原题顺序，原完成part确认答案，错投和仅提交成功不能确认消费。
#[test]
fn native_opencode_maps_ordered_answers_and_requires_exact_consumption() {
    let raw = request();
    let question = OpenCodeQuestion::from_request(&raw).unwrap();
    let pending = question.question(target(&raw)).unwrap();
    assert_eq!(pending.items.len(), 3, "必须保留全部原题");
    assert!(
        !pending.items[0].multiple && pending.items[1].multiple,
        "单选和多选能力来自原题"
    );
    assert!(pending.items[2].allow_custom, "原版本默认允许自定义答案");
    assert_eq!(
        question.response(&answers()).unwrap(),
        json!({"answers":[["Green"],["macOS","Linux"],["NATIVE_OPENCODE_7F2"]]}),
        "答案按原题顺序映射，不依赖调用方数组顺序"
    );
    assert!(
        question.confirms_answer(&consumed(), &answers()),
        "同工具completed及metadata确认三题答案"
    );
    assert!(
        !question.confirms_answer(&json!(true), &answers()),
        "HTTP 200/true只是提交，不是消费"
    );
    for field in ["sessionID", "messageID", "callID", "tool"] {
        let mut wrong = consumed();
        wrong[field] = "foreign".into();
        assert!(
            !question.confirms_answer(&wrong, &answers()),
            "错误{field}不能确认原工具消费"
        );
    }
    let mut wrong = consumed();
    wrong["state"]["status"] = "running".into();
    assert!(
        !question.confirms_answer(&wrong, &answers()),
        "运行中的metadata不能伪装完成"
    );
    let mut wrong = target(&raw);
    wrong.session_id = "foreign".into();
    assert!(
        question.question(wrong).is_err(),
        "另一个session不能复用原问题"
    );
}

/// 场景：坏题面或不满足原题约束的答案应在任何HTTP请求之前拒绝。
/// 构造：在M0题面分别加入重复label、关闭custom、缺工具关联；对单选给两个值。
/// 执行：解析或映射上述负例；另给合法题面错误消费答案。
/// 预期：不猜重复label、不接受越权自定义/多选、不把不同答案标为本次消费。
#[test]
fn native_opencode_rejects_ambiguous_or_unsupported_answers() {
    let mut raw = request();
    raw["questions"][0]["options"][1]["label"] = "Blue".into();
    assert!(
        OpenCodeQuestion::from_request(&raw).is_err(),
        "相同label不能对应两个本地选项"
    );
    let mut raw = request();
    raw["questions"][2]["custom"] = false.into();
    let question = OpenCodeQuestion::from_request(&raw).unwrap();
    assert!(
        question.response(&answers()).is_err(),
        "custom=false不能回自由文本"
    );
    let question = OpenCodeQuestion::from_request(&request()).unwrap();
    let mut invalid = answers();
    invalid[1].selected.push("option-0".into());
    assert!(question.response(&invalid).is_err(), "单选不能提交两个值");
    let mut raw = request();
    raw["tool"] = Value::Null;
    assert!(
        OpenCodeQuestion::from_request(&raw).is_err(),
        "缺原工具关联不能提供可消费确认的代答能力"
    );
    let mut wrong = consumed();
    wrong["state"]["metadata"]["answers"][0] = json!(["Blue"]);
    assert!(
        !question.confirms_answer(&wrong, &answers()),
        "相同工具不同答案不能被计为本次提交"
    );
}
