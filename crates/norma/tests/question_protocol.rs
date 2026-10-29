use norma::protocol::{PendingQuestion, QuestionAnswer, QuestionItem, QuestionOption, QuestionRef};
use serde_json::json;

fn question() -> PendingQuestion {
    PendingQuestion {
        target: serde_json::from_value::<QuestionRef>(json!({
            "workspace_id":"w", "project_id":"p", "node_id":1, "provider":"Claude",
            "run_id":"r", "connection_id":"c", "session_id":"s", "request_id":"tool-1", "revision":1
        }))
        .unwrap(),
        items: vec![
            QuestionItem {
                key: "label".into(),
                provider_id: None,
                prompt: "Which label?".into(),
                options: vec![QuestionOption {
                    key: "green".into(),
                    label: "Green".into(),
                    description: None,
                }],
                multiple: false,
                allow_custom: false,
                secret: false,
            },
            QuestionItem {
                key: "notes".into(),
                provider_id: None,
                prompt: "Any notes?".into(),
                options: vec![],
                multiple: false,
                allow_custom: true,
                secret: false,
            },
        ],
    }
}

/// 场景：多题必须逐题完整校验，不能漏题、重复题或静默丢弃非法选项。
/// 构造：第一题单选 Green 且禁止自定义，第二题自由输入；构造完整和不完整答案。
/// 执行：调用公开 PendingQuestion::validate_answers 校验每种输入。
/// 预期：完整答案通过，漏题、重复题、非法选项与非法自定义均被拒绝。
#[test]
fn answers_cover_every_question_without_loss() {
    let question = question();
    let complete = vec![
        QuestionAnswer {
            key: "label".into(),
            selected: vec!["green".into()],
            text: None,
        },
        QuestionAnswer {
            key: "notes".into(),
            selected: vec![],
            text: Some("保留 用户原文".into()),
        },
    ];
    assert_eq!(
        question.validate_answers(&complete),
        Ok(()),
        "完整有效答案应通过"
    );
    assert!(
        question.validate_answers(&complete[..1]).is_err(),
        "漏题不得提交"
    );
    let mut invalid = complete.clone();
    invalid[1] = invalid[0].clone();
    assert!(
        question.validate_answers(&invalid).is_err(),
        "重复题不得覆盖另一题"
    );
    invalid = complete.clone();
    invalid[0].selected = vec!["unknown".into()];
    assert!(
        question.validate_answers(&invalid).is_err(),
        "不存在的选项不得提交"
    );
    invalid[0].selected.clear();
    invalid[0].text = Some("Green".into());
    assert!(
        question.validate_answers(&invalid).is_err(),
        "禁止custom时不能以文字绕过选项身份"
    );
}
