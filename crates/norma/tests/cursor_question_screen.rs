use norma::cursor::{
    questions::CursorQuestion,
    screen::{CursorQuestionFrame, CursorQuestionStep},
};
use norma::protocol::QuestionAnswer;
fn question() -> CursorQuestion {
    CursorQuestion::from_tool(
        "original",
        &serde_json::from_str(include_str!("fixtures/questions/cursor-tool-call.json")).unwrap(),
    )
    .unwrap()
}
fn rows(value: &str) -> Vec<String> {
    value.lines().map(str::to_owned).collect()
}

/// 场景：只根据原题与当前完整原生控件计算一个步骤，不把已写入等同于消费。
/// 构造：真实单选、多选、自定义三张 grid 夹具，分别已有所需选中结果。
/// 执行：解析当前子题与焦点，校验全部选中项，计算下一键；污染题序或选项时重试解析。
/// 预期：已选结果只推进一次 Enter；Space 是单独步骤；题面变化与不完整渲染不能继续输入。
#[test]
fn cursor_native_frame_matches_original_items_before_each_step() {
    let question = question();
    for (raw, index, answer) in [
        (
            include_str!("fixtures/questions/cursor-single-selected.txt"),
            0,
            QuestionAnswer {
                key: "question-0".into(),
                selected: vec!["option-1".into()],
                text: None,
            },
        ),
        (
            include_str!("fixtures/questions/cursor-multiple-selected.txt"),
            1,
            QuestionAnswer {
                key: "question-1".into(),
                selected: vec!["option-0".into(), "option-2".into()],
                text: None,
            },
        ),
        (
            include_str!("fixtures/questions/cursor-custom-typed.txt"),
            2,
            QuestionAnswer {
                key: "question-2".into(),
                selected: vec![],
                text: Some("NATIVE_ONLY_7F2".into()),
            },
        ),
    ] {
        let frame =
            CursorQuestionFrame::parse(&question, &rows(raw)).expect("真实完整原控件必须能识别");
        assert_eq!(frame.question_index(), index, "子题位置来自实际grid");
        assert_eq!(
            frame.next_step(&question, &answer).unwrap(),
            CursorQuestionStep::Enter,
            "所有选中值逐项一致才推进"
        );
        assert!(
            CursorQuestionFrame::parse(&question, &rows(&raw.replace("Color?", "Other question?")))
                .is_none()
                || index != 0,
            "题面改变不继续旧脚本"
        );
        assert!(
            CursorQuestionFrame::parse(
                &question,
                &rows(&raw.replace("Space select", "unknown layout"))
            )
            .is_none(),
            "未知版本布局不能猜按键"
        );
    }
    let raw = include_str!("fixtures/questions/cursor-single-selected.txt")
        .replace("[x] Green", "[ ] Green");
    let frame = CursorQuestionFrame::parse(&question, &rows(&raw)).unwrap();
    let answer = QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    };
    assert_eq!(
        frame.next_step(&question, &answer).unwrap(),
        CursorQuestionStep::Space,
        "焦点在未选项时只发按键Space，不paste空格"
    );
}

/// 场景：人工已有自定义草稿或多选未满足时，不得直接提交。
/// 构造：原第三题已有另一个草稿；第二题丢失macOS选择，焦点在Linux。
/// 执行：计算单步计划，并对截断footer的临时帧解析。
/// 预期：保留人工草稿、回到漏选项；没有完整控件时不生成任何输入。
#[test]
fn cursor_native_steps_preserve_existing_custom_draft_and_check_all_selections() {
    let question = question();
    let frame = CursorQuestionFrame::parse(
        &question,
        &rows(include_str!("fixtures/questions/cursor-custom-typed.txt")),
    )
    .unwrap();
    let custom = QuestionAnswer {
        key: "question-2".into(),
        selected: vec![],
        text: Some("different".into()),
    };
    assert!(
        frame.next_step(&question, &custom).is_err(),
        "不覆盖原Other草稿"
    );
    let raw = include_str!("fixtures/questions/cursor-multiple-selected.txt")
        .replace("[x] macOS", "[ ] macOS");
    let frame = CursorQuestionFrame::parse(&question, &rows(&raw)).unwrap();
    let answer = QuestionAnswer {
        key: "question-1".into(),
        selected: vec!["option-0".into(), "option-2".into()],
        text: None,
    };
    assert_eq!(
        frame.next_step(&question, &answer).unwrap(),
        CursorQuestionStep::Up,
        "漏选必须回到目标，不提交半份答案"
    );
    let partial = raw.split("↑/↓ option").next().unwrap();
    assert!(
        CursorQuestionFrame::parse(&question, &rows(partial)).is_none(),
        "部分渲染不应可输入"
    );
}

/// 场景：多选题已有普通选项时，仅自由文本答案不能带入旧选择。
/// 构造：真实多选帧保留macOS，Other聚焦且已有FreeBSD草稿，Linux取消。
/// 执行：为仅FreeBSD的答案计算下一步。
/// 预期：拒绝提交混合答案，不能输出Enter；正常仅文本第三题仍由已有正例覆盖。
#[test]
fn cursor_custom_answer_never_submits_existing_multiselect_options() {
    let raw = include_str!("fixtures/questions/cursor-multiple-selected.txt")
        .replace("› [x] Linux", "  [ ] Linux")
        .replace("[ ] Other: (type to answer)", "› [x] Other: FreeBSD");
    let question = question();
    let frame = CursorQuestionFrame::parse(&question, &rows(&raw)).unwrap();
    let answer = QuestionAnswer {
        key: "question-1".into(),
        selected: vec![],
        text: Some("FreeBSD".into()),
    };
    assert!(
        frame.next_step(&question, &answer).is_err(),
        "原生会保留macOS加自由文本，必须在Enter前阻止"
    );
}

/// 场景：原生占位文案与实际输入文本在无样式网格中可能完全相同。
/// 构造：原第三题Other勾选，画面显示(type to answer)，既可能是删空草稿也可能是字面文本。
/// 执行：解析歧义帧，并对同样文案但未勾选的初始空框做正例。
/// 预期：勾选歧义帧不生成操作，未勾选空框仍可识别，不追加或覆盖未知草稿。
#[test]
fn cursor_selected_placeholder_is_ambiguous_and_never_appended() {
    let raw = include_str!("fixtures/questions/cursor-custom-typed.txt")
        .replace("NATIVE_ONLY_7F2", "(type to answer)");
    let question = question();
    assert!(
        CursorQuestionFrame::parse(&question, &rows(&raw)).is_none(),
        "已选占位文本无法证明空草稿，禁止继续"
    );
    let empty = raw.replace("[x] Other:", "[ ] Other:");
    assert!(
        CursorQuestionFrame::parse(&question, &rows(&empty)).is_some(),
        "原生未选空框仍可解析"
    );
}

/// 场景：上一键是否生效必须由问答控件语义确认，不能靠外部重绘或窗口变化。
/// 构造：真实Green焦点未选帧、选中帧、以及只有标题变化的相同控件帧。
/// 执行：核对Space、Down、Text各自期望的后继控件。
/// 预期：Space只接受选中结果；原控件不变或焦点/选择变化不符时不能确认进展。
#[test]
fn cursor_step_acknowledges_only_the_expected_question_control_change() {
    let question = question();
    let selected = include_str!("fixtures/questions/cursor-single-selected.txt");
    let before = CursorQuestionFrame::parse(
        &question,
        &rows(&selected.replace("[x] Green", "[ ] Green")),
    )
    .unwrap();
    let after = CursorQuestionFrame::parse(&question, &rows(selected)).unwrap();
    assert!(
        before.confirms_step(&CursorQuestionStep::Space, &after),
        "原焦点Green实际选中才确认Space"
    );
    assert!(
        !before.confirms_step(&CursorQuestionStep::Space, &before),
        "控件不变不能用外部重绘确认Space"
    );
    assert!(
        !before.confirms_step(&CursorQuestionStep::Down, &after),
        "选择变化不能充当Down焦点变化"
    );
}

/// 场景：Cursor原生Enter先选择当前焦点再提交，正确勾选集合仍不足以保证答案不变。
/// 构造：单选Green已选但焦点Blue；多选仅macOS已选但焦点在刚取消的Linux。
/// 执行：计算下一步并验证不能直接Enter。
/// 预期：先把焦点移回一个目标已选项，避免Enter替换单选或重新加回多选。
#[test]
fn cursor_enter_must_focus_an_intended_selection() {
    let question = question();
    for (raw, key, selected, step) in [
        (
            include_str!("fixtures/questions/cursor-single-selected.txt")
                .replace("  [ ] Blue", "› [ ] Blue")
                .replace("› [x] Green", "  [x] Green"),
            "question-0",
            vec!["option-1"],
            CursorQuestionStep::Down,
        ),
        (
            include_str!("fixtures/questions/cursor-multiple-selected.txt")
                .replace("› [x] Linux", "› [ ] Linux"),
            "question-1",
            vec!["option-0"],
            CursorQuestionStep::Up,
        ),
    ] {
        let frame = CursorQuestionFrame::parse(&question, &rows(&raw))
            .expect("真实焦点变化后仍是完整原控件");
        let answer = QuestionAnswer {
            key: key.into(),
            selected: selected.into_iter().map(str::to_owned).collect(),
            text: None,
        };
        assert_eq!(
            frame.next_step(&question, &answer).unwrap(),
            step,
            "Enter会先选当前焦点，必须先移回目标"
        );
    }
}
