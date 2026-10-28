use norma::codex::questions::CodexQuestion;
use norma::protocol::{AgentProfile, QuestionAnswer, QuestionRef, QuestionRequestId};
use serde_json::{json, Value};

fn fixture(name: &str) -> Value {
    serde_json::from_str(match name {
        "request" => include_str!("fixtures/questions/codex-request.json"),
        "source" => include_str!("fixtures/questions/codex-source.json"),
        "consumed" => include_str!("fixtures/questions/codex-consumed.json"),
        _ => panic!("unknown test fixture"),
    })
    .unwrap()
}
fn target(id: QuestionRequestId) -> QuestionRef {
    QuestionRef {
        workspace_id: "workspace".into(),
        project_id: "project".into(),
        node_id: 1,
        provider: AgentProfile::Codex,
        run_id: "run".into(),
        connection_id: "connection".into(),
        session_id: "codex-question-test-thread".into(),
        turn_id: Some("codex-question-test-turn".into()),
        request_id: id,
        revision: 1,
    }
}

/// 场景：真实原生Codex RPC必须由同工具来源核验，答案回到原RPC并等待关联工具结果。
/// 构造：M0真实待答/原始函数调用/消费消息的去标识夹具；分别使用数值0与字符串"0"。
/// 执行：转换原题、选择Green、生成响应；再核验真实消费事件及普通resolved事件。
/// 预期：RPC ID类型不变，答案使用provider题ID及label；只有同call_id的精确结果确认消费。
#[test]
fn codex_question_preserves_rpc_identity_and_confirms_exact_original_consumption() {
    for id in [json!(0), json!("0")] {
        let mut request = fixture("request");
        request["id"] = id.clone();
        let adapter = CodexQuestion::from_events(&request, &fixture("source")).unwrap();
        let rpc: QuestionRequestId = serde_json::from_value(id.clone()).unwrap();
        let question = adapter.question(target(rpc)).unwrap();
        assert_eq!(
            question.items[0].provider_id.as_deref(),
            Some("label"),
            "原provider题ID被保留"
        );
        let answers = vec![QuestionAnswer {
            key: question.items[0].key.clone(),
            selected: vec![question.items[0].options[1].key.clone()],
            text: None,
        }];
        assert_eq!(adapter.rpc_id(), id, "数值与字符串RPC ID不能混淆");
        assert_eq!(
            adapter.response(&answers).unwrap(),
            json!({"answers":{"label":{"answers":["Green"]}}}),
            "原题按provider格式回填"
        );
        assert!(
            adapter.confirms_answer(&fixture("consumed"), &answers),
            "同原始函数的精确Green结果确认消费"
        );
        assert!(
            !adapter.confirms_answer(
                &json!({"method":"serverRequest/resolved","params":{"requestId":id}}),
                &answers
            ),
            "RPC已解决不等于答案已消费"
        );
        let mut wrong = fixture("consumed");
        wrong["params"]["item"]["call_id"] = json!("other-call");
        assert!(
            !adapter.confirms_answer(&wrong, &answers),
            "另一工具结果不能确认本题"
        );
    }
}

/// 场景：相同requestUserInput方法可能来自connector审批，题面变化或来源不明也不可代答。
/// 构造：同一真实RPC，分别变更原始工具名、会话、轮次、call_id、题面，及缺失来源。
/// 执行：逐个交给生产转换入口；另尝试把合法题绑定另一会话的QuestionRef。
/// 预期：全部拒绝，不能以相似题面或相同RPC ID替代真实来源关联。
#[test]
fn codex_question_rejects_connector_approval_unknown_and_mismatched_sources() {
    let request = fixture("request");
    let source = fixture("source");
    let mut bad_sources = vec![json!({})];
    for (field, value) in [
        ("name", "mcp__connector__delete_document"),
        ("call_id", "another-call"),
        ("type", "custom_tool_call"),
    ] {
        let mut changed = source.clone();
        changed["params"]["item"][field] = value.into();
        bad_sources.push(changed);
    }
    for field in ["threadId", "turnId"] {
        let mut changed = source.clone();
        changed["params"][field] = "another-session-or-turn".into();
        bad_sources.push(changed);
    }
    for changed in bad_sources {
        assert!(
            CodexQuestion::from_events(&request, &changed).is_err(),
            "没有同工具同会话同轮次的来源必须拒绝：{changed}"
        );
    }
    let mut changed = request.clone();
    changed["params"]["questions"][0]["options"][1]["description"] = "A different meaning".into();
    assert!(
        CodexQuestion::from_events(&changed, &source).is_err(),
        "选项解释变化意味着不是原题"
    );
    let adapter = CodexQuestion::from_events(&request, &source).unwrap();
    let mut wrong = target(QuestionRequestId::Number(0));
    wrong.session_id = "another-thread".into();
    assert!(
        adapter.question(wrong).is_err(),
        "合法RPC也不能错绑另一会话"
    );
    assert!(
        adapter
            .question(target(QuestionRequestId::String("0".into())))
            .is_err(),
        "数值0的请求不能错绑字符串0"
    );
}

/// 场景：原生单选/自由文本按题完整映射，重复label和未知多选能力不能猜。
/// 构造：真实单选夹具；自定义带引号、换行与shell符号的文本；再改为多选或重复label。
/// 执行：先提交自定义文本，检查精确消费；对不完整、多选、重复label输入执行验证。
/// 预期：文本原样返回JSON，不变成命令；每题必须完整，未声明能力全部拒绝。
#[test]
fn codex_question_maps_free_text_and_rejects_ambiguous_or_partial_answers() {
    let request = fixture("request");
    let source = fixture("source");
    let adapter = CodexQuestion::from_events(&request, &source).unwrap();
    let text = "Keep \"quoted\"\n$(not-a-command) ; value";
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec![],
        text: Some(text.into()),
    }];
    assert_eq!(
        adapter.response(&answers).unwrap(),
        json!({"answers":{"label":{"answers":[text]}}}),
        "自由文本原样保留在数据字段"
    );
    let mut consumed = fixture("consumed");
    consumed["params"]["item"]["output"] = adapter.response(&answers).unwrap().to_string().into();
    assert!(
        adapter.confirms_answer(&consumed, &answers),
        "精确文本结果确认消费"
    );
    assert!(
        !adapter.confirms_answer(&fixture("consumed"), &answers),
        "Green回执不确认自由文本答案"
    );
    assert!(adapter.response(&[]).is_err(), "空答案不发送");
    let selected = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-0".into(), "option-1".into()],
        text: None,
    }];
    assert!(adapter.response(&selected).is_err(), "单选不能发送多个值");
    for multiple in [false, true] {
        let mut request = request.clone();
        let mut source = source.clone();
        if multiple {
            request["params"]["questions"][0]["multiSelect"] = true.into();
        } else {
            request["params"]["questions"][0]["options"][1]["label"] = "Blue".into();
        }
        source["params"]["item"]["arguments"] = json!({"questions":request["params"]["questions"]})
            .to_string()
            .into();
        assert!(
            CodexQuestion::from_events(&request, &source).is_err(),
            "不支持多选或歧义label必须拒绝，multiple={multiple}"
        );
    }
}

/// 场景：两道子题必须完整映射，secret及禁用custom能力不能被归一化过程丢失。
/// 构造：第一题禁用自定义，第二题为secret文本；provider题ID分别为label与token。
/// 执行：答案倒序输入，检查provider键及原文，再尝试少答一题与禁用custom题的自由文本。
/// 预期：输出按原provider ID完整映射；secret保留，缺题或违反custom能力都拒绝。
#[test]
fn codex_multiple_questions_preserve_secret_and_custom_capabilities() {
    let mut request = fixture("request");
    request["params"]["questions"][0]["isOther"] = false.into();
    request["params"]["questions"].as_array_mut().unwrap().push(json!({
        "id":"token", "question":"Private token?", "header":"Token", "isOther":true, "isSecret":true, "options":null
    }));
    let mut source = fixture("source");
    source["params"]["item"]["arguments"] = json!({"questions":request["params"]["questions"]})
        .to_string()
        .into();
    let adapter = CodexQuestion::from_events(&request, &source).unwrap();
    let question = adapter
        .question(target(QuestionRequestId::Number(0)))
        .unwrap();
    assert!(question.items[1].secret, "secret必须被宿主保护");
    assert!(!question.items[0].allow_custom, "第一题不允许自定义");
    let mut answers = vec![
        QuestionAnswer {
            key: "question-1".into(),
            selected: vec![],
            text: Some("private test token".into()),
        },
        QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        },
    ];
    assert_eq!(
        adapter.response(&answers).unwrap(),
        json!({"answers":{
            "label":{"answers":["Green"]}, "token":{"answers":["private test token"]}
        }}),
        "输入答案顺序不改变provider对应关系"
    );
    assert!(
        adapter.response(&answers[..1]).is_err(),
        "少答第一题必须拒绝"
    );
    answers[1].selected.clear();
    answers[1].text = Some("custom".into());
    assert!(
        adapter.response(&answers).is_err(),
        "第一题禁用custom，不能发送自由文本"
    );
}

/// 场景：选项标签只有“方案A/B”，实际取舍在description，幕僚必须看到完整决策信息。
/// 构造：真实夹具的Green描述改为有独立含义的说明，并同步原始函数参数。
/// 执行：转换为共享题面并序列化，随后仍按原label生成Provider答案。
/// 预期：说明进入共享协议/模型上下文，响应仍仅包含原label，不能把说明拼入答案。
#[test]
fn codex_preserves_option_descriptions_without_changing_answer_labels() {
    let mut request = fixture("request");
    request["params"]["questions"][0]["options"][1]["description"] =
        "Preserve existing files and migrate only the index.".into();
    let mut source = fixture("source");
    source["params"]["item"]["arguments"] = json!({"questions":request["params"]["questions"]})
        .to_string()
        .into();
    let adapter = CodexQuestion::from_events(&request, &source).unwrap();
    let question = adapter
        .question(target(QuestionRequestId::Number(0)))
        .unwrap();
    assert_eq!(
        question.items[0].options[1].description.as_deref(),
        Some("Preserve existing files and migrate only the index."),
        "选项取舍说明不能在归一化时丢失"
    );
    assert_eq!(
        serde_json::to_value(&question).unwrap()["items"][0]["options"][1]["description"],
        "Preserve existing files and migrate only the index.",
        "questions与幕僚事件共享完整题面"
    );
    assert_eq!(
        adapter
            .response(&[QuestionAnswer {
                key: "question-0".into(),
                selected: vec!["option-1".into()],
                text: None
            }])
            .unwrap(),
        json!({"answers":{"label":{"answers":["Green"]}}}),
        "provider答案仍按label映射"
    );
}
