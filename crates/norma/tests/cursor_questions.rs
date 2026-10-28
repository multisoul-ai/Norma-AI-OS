use norma::cursor::questions::CursorQuestion;
use norma::protocol::{AgentProfile, QuestionAnswer, QuestionRef, QuestionRequestId};
use serde_json::Value;
fn tool() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/cursor-tool-call.json")).unwrap()
}
fn result() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/cursor-tool-result.json")).unwrap()
}
fn target() -> QuestionRef {
    QuestionRef {
        workspace_id: "w".into(),
        project_id: "p".into(),
        node_id: 7,
        provider: AgentProfile::Cursor,
        run_id: "run".into(),
        connection_id: "cursor-native:owned".into(),
        session_id: "original-session".into(),
        turn_id: None,
        request_id: QuestionRequestId::String(tool()["toolCallId"].as_str().unwrap().into()),
        revision: 1,
    }
}
fn answers() -> Vec<QuestionAnswer> {
    vec![
        QuestionAnswer {
            key: "question-2".into(),
            selected: vec![],
            text: Some("NATIVE_ONLY_7F2".into()),
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

/// 场景：Cursor 原 AskQuestion 的本地 key 映射回原 id，消费证据必须匹配同一完整 toolCallId。
/// 构造：M0 真实三题与工具结果，包含多选、自定义和带换行的原 call ID；答案故意乱序。
/// 执行：归一化原题，校验精确答案文本，分别污染 call ID、结果和目标 session。
/// 预期：三题能力保留，原工具精确消费成功，错误调用/会话或不同答案不能确认。
#[test]
fn native_cursor_preserves_question_identity_and_exact_consumption() {
    let question = CursorQuestion::from_tool("original-session", &tool()).unwrap();
    let pending = question.question(target()).unwrap();
    assert_eq!(pending.items.len(), 3, "保留全部原问题");
    assert!(
        !pending.items[0].multiple && pending.items[1].multiple,
        "仅原 snake_case 能力生效"
    );
    assert!(pending.items[2].allow_custom, "原 TUI 支持 Other 自定义");
    assert!(
        question.confirms_answer(&result(), &answers()),
        "同工具结果确认实际三题答案"
    );
    let mut wrong = result();
    wrong["toolCallId"] = "other".into();
    assert!(
        !question.confirms_answer(&wrong, &answers()),
        "同答案不能跨工具调用"
    );
    let mut wrong = result();
    wrong["result"] =
        "User questions responses:\nQuestion q_single: Selected option(s) blue".into();
    assert!(
        !question.confirms_answer(&wrong, &answers()),
        "缺题或答案不同不能确认"
    );
    let mut wrong = target();
    wrong.session_id = "foreign".into();
    assert!(question.question(wrong).is_err(), "原题不能跨会话");
}

/// 场景：原题能力有歧义或计划答案不可由验证过的 TUI 路径表达时，输入前拒绝。
/// 构造：重复 option label、单选多个值、多选选项与自定义混用、驼峰伪能力。
/// 执行：解析各坏题面或验证坏答案，并保留一组合法三题正例。
/// 预期：不猜测控件、不静默丢弃选项，只有已验证的原题和完整答案可进入终端步骤。
#[test]
fn native_cursor_rejects_ambiguous_screen_mapping_and_incomplete_answers() {
    let mut raw = tool();
    raw["args"]["questions"][0]["options"][1]["label"] = "Blue".into();
    assert!(
        CursorQuestion::from_tool("original-session", &raw).is_err(),
        "重复 label 无法安全定位原控件"
    );
    let question = CursorQuestion::from_tool("original-session", &tool()).unwrap();
    let mut invalid = answers();
    invalid[1].selected.push("option-0".into());
    assert!(question.expected_result(&invalid).is_err(), "单选不能多选");
    let mut invalid = answers();
    invalid[2].text = Some("also custom".into());
    assert!(
        question.expected_result(&invalid).is_err(),
        "没有实证的选项加自定义组合不能猜操作"
    );
    assert!(
        question.expected_result(&answers()[..2]).is_err(),
        "不能跳过未回答题"
    );
    assert!(
        question.expected_result(&answers()).is_ok(),
        "合法三类完整答案可映射"
    );
}

/// 场景：原生多选按点击顺序返回，已有人工选择可能与原选项顺序相反。
/// 构造：把真实结果中的 mac, linux 反转，另构造重复 ID、漏选和含分隔符 ID。
/// 执行：按相同完整答案核对消费记录并解析歧义原题。
/// 预期：仅顺序不同仍确认为同答案，重复/缺失/歧义不确认。
#[test]
fn cursor_multiselect_consumption_compares_original_ids_without_assuming_click_order() {
    let question = CursorQuestion::from_tool("original-session", &tool()).unwrap();
    let mut reverse = result();
    reverse["result"] = reverse["result"]
        .as_str()
        .unwrap()
        .replace("mac, linux", "linux, mac")
        .into();
    assert!(
        question.confirms_answer(&reverse, &answers()),
        "原生点击顺序不改变答案语义"
    );
    for replacement in ["mac, mac", "mac", "mac, linux, linux"] {
        let mut wrong = result();
        wrong["result"] = wrong["result"]
            .as_str()
            .unwrap()
            .replace("mac, linux", replacement)
            .into();
        assert!(
            !question.confirms_answer(&wrong, &answers()),
            "重复或缺少原选项不得确认:{replacement}"
        );
    }
    let mut ambiguous = tool();
    ambiguous["args"]["questions"][1]["options"][0]["id"] = "mac, linux".into();
    assert!(
        CursorQuestion::from_tool("original-session", &ambiguous).is_err(),
        "原生未转义分隔符不能拆成多个ID"
    );
}

/// 场景：原生TUI将末尾Other类选项折叠到自由输入框，不存在对应普通选项行。
/// 构造：给真实单选题追加Other、Something else以及原生支持的前缀变体。
/// 执行：归一化显示选项，并与没有该占位选项的原题映射比较。
/// 预期：只保留Blue/Green两个普通选项，自定义仍可用，不出现永远无法定位的第三项。
#[test]
fn cursor_trailing_other_alias_is_collapsed_like_native_display_options() {
    for label in [
        "Other",
        "Something else",
        "Other: explain",
        "other - write",
        "Other (custom)",
        "Something else: text",
    ] {
        let mut raw = tool();
        raw["args"]["questions"][0]["options"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id":"fallback","label":label}));
        let question = CursorQuestion::from_tool("original-session", &raw).unwrap();
        assert_eq!(
            question.items()[0].options.len(),
            2,
            "原生末项{label}折叠为自由输入"
        );
        assert!(
            question.items()[0].allow_custom,
            "折叠后保留原生自由输入能力"
        );
        assert_eq!(
            question.items()[0].options[1].label,
            "Green",
            "前序普通选项身份不变"
        );
    }
}
