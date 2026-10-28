use super::*;
use crate::workspace::desktop::overview_rows::{OverviewAction, OverviewScreen};
use norma::protocol::{
    AgentProfile, PendingQuestion, QuestionItem, QuestionOption, QuestionRef, QuestionRequestId,
};

pub(super) fn question() -> PendingQuestion {
    PendingQuestion {
        target: QuestionRef {
            workspace_id: "desktop:w:a".into(),
            project_id: "p".into(),
            node_id: 1,
            provider: AgentProfile::Claude,
            run_id: "r".into(),
            connection_id: "c".into(),
            session_id: "s".into(),
            turn_id: None,
            request_id: QuestionRequestId::String("tool".into()),
            revision: 1,
        },
        items: [false, true]
            .into_iter()
            .enumerate()
            .map(|(index, multiple)| QuestionItem {
                key: format!("q{index}"),
                provider_id: None,
                prompt: "Choose".into(),
                options: ["Red", "Green"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, label)| QuestionOption {
                        key: format!("o{i}"),
                        label: label.into(),
                        description: None,
                    })
                    .collect(),
                multiple,
                allow_custom: true,
                secret: false,
            })
            .collect(),
    }
}

/// 场景：概览编辑多题时保持原题顺序，单选互斥，多选允许组合自定义文本。
/// 构造：一题单选、一题多选；先留空，再选择多个选项并切换自定义答案。
/// 执行：使用生产编辑模型的choose/custom/answers入口并调用协议完整性校验。
/// 预期：不完整答案不能发送，单选只保留最后一次选择，多选及自由文本完整提交。
#[test]
fn question_editor_preserves_cardinality_and_original_order() {
    let question = question();
    let mut draft = AnswerDraft::new(&question, &[]);
    assert!(
        question
            .validate_answers(&draft.answers(&[String::new(), String::new()]))
            .is_err(),
        "空题不得发送"
    );
    draft.choose(0, "o0");
    draft.choose(0, "o1");
    draft.choose(1, "o0");
    draft.choose(1, "o1");
    draft.custom(1, true);
    let answers = draft.answers(&[String::new(), "Blue".into()]);
    assert_eq!(answers[0].selected, ["o1"], "单选替换已有值");
    assert_eq!(answers[1].selected, ["o0", "o1"], "多选保留全部选择");
    assert_eq!(
        answers[1].text.as_deref(),
        Some("Blue"),
        "多选保留自定义文本"
    );
    assert!(
        question.validate_answers(&answers).is_ok(),
        "完整原题答案可发送"
    );
    draft.custom(0, true);
    assert!(
        draft.answers(&["Custom".into(), "Blue".into()])[0]
            .selected
            .is_empty(),
        "单选自定义撤销选项"
    );
    draft.choose(0, "o0");
    assert!(
        draft.answers(&["stale custom".into(), "Blue".into()])[0]
            .text
            .is_none(),
        "选择选项不携带旧自定义文本"
    );
}

/// 场景：原题替代同卡片的状态行，但不能隐藏独立审批或另一个项目的同编号卡片。
/// 构造：A/B两区域相同node编号，以及A的独立审批；原题属于A。
/// 执行：使用生产投影合并原题到概览统计。
/// 预期：A状态行去重，B和审批保留，原题只增加一次待处理计数。
#[test]
fn question_projection_deduplicates_only_same_project_status_rows() {
    let mut screen = OverviewScreen::empty();
    let make = |action| crate::workspace::desktop::overview_rows::OverviewRow {
        primary: "row".into(),
        secondary: String::new(),
        detail: None,
        task_line: None,
        project_tag: None,
        is_agent: true,
        action_label: "Locate".into(),
        action,
        status_label: None,
        lead_label: None,
        status_tone: crate::workspace::desktop::overview_rows::OverviewStatusTone::Idle,
        approval_badge: None,
        progress: None,
        prelight: None,
        enabled: true,
        height: 48.,
    };
    screen.sections[1].rows = ["a", "b"]
        .map(|area| {
            make(OverviewAction::LocateAgent {
                area_id: area.into(),
                node_id: 1,
            })
        })
        .into();
    screen.sections[1].count = Some(2);
    screen.sections[0]
        .rows
        .push(make(OverviewAction::LocateApproval {
            area_id: "a".into(),
            node_id: 1,
            run_id: "r".into(),
            turn_id: "t".into(),
            request_id: Some("approval".into()),
        }));
    screen.sections[0].count = Some(1);
    merge_question_counts(&mut screen, &[("a".into(), 1)]);
    assert_eq!(
        screen.summary(),
        [2, 1, 0],
        "原题与独立审批各一项，另一项目仍运行"
    );
    assert_eq!(screen.sections[1].rows.len(), 1, "同卡片不重复显示运行行");
    assert!(
        matches!(&screen.sections[1].rows[0].action, OverviewAction::LocateAgent { area_id, .. } if area_id == "b"),
        "同编号另一项目不得移除"
    );
    assert_eq!(screen.sections[0].rows.len(), 1, "独立审批行仍存在");
}
