use norma::codex::questions::CodexQuestions;
use norma::protocol::{QuestionAnswer, QuestionState};
use serde_json::{json, Value};

fn request() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/codex-request.json")).unwrap()
}
fn source() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/codex-source.json")).unwrap()
}
fn consumed() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/codex-consumed.json")).unwrap()
}
fn answers() -> Vec<QuestionAnswer> {
    vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }]
}

/// 场景：pending重放和来源事件可乱序；提交原RPC后必须等待原工具的精确消费。
/// 构造：真实去标识Codex夹具，重复pending两次，分别先来源或先pending。
/// 执行：观察两条消息、取一次可答事件、提交、观察resolved及真实结果。
/// 预期：只注册一道题且只产生一次响应；resolved不确认消费，精确结果才进入Answered。
#[test]
fn replayed_pending_is_one_question_and_resolved_is_not_consumption() {
    for source_first in [false, true] {
        let mut ledger = CodexQuestions::new("codex-question-test-thread".into());
        let pair = if source_first {
            [source(), request()]
        } else {
            [request(), source()]
        };
        ledger.observe(&pair[0]).unwrap();
        assert!(ledger.take_changes().is_empty(), "仅一侧证据不能开放代答");
        ledger.observe(&pair[1]).unwrap();
        ledger.observe(&request()).unwrap();
        let changes = ledger.take_changes();
        assert_eq!(changes.len(), 1, "重复pending只有一个注册事件");
        assert_eq!(changes[0].state, QuestionState::Pending, "已核验原题可答");
        let question = changes[0].question.clone();
        assert_eq!(
            ledger.reply(&question, &answers()).unwrap(),
            json!({"id":0,"result":{"answers":{"label":{"answers":["Green"]}}}}),
            "只回原RPC，既不启动新turn也不派发普通任务"
        );
        assert!(
            ledger.reply(&question, &answers()).is_err(),
            "本地重复提交被拒绝"
        );
        ledger
            .observe(&json!({"method":"serverRequest/resolved","params":{
                "threadId":"codex-question-test-thread","requestId":0
            }}))
            .unwrap();
        assert!(
            ledger
                .take_changes()
                .iter()
                .all(|change| change.state != QuestionState::Answered),
            "写出及resolved都不是消费证据"
        );
        ledger.observe(&consumed()).unwrap();
        let confirmed = ledger.take_changes();
        assert_eq!(confirmed.len(), 1, "只有一次消费事件");
        assert_eq!(
            confirmed[0].state,
            QuestionState::Answered,
            "精确工具结果确认本次答案"
        );
        assert!(confirmed[0].evidence.is_some(), "保留原始消费证据供持久化");
        ledger.observe(&request()).unwrap();
        assert!(
            ledger.take_changes().is_empty(),
            "完成后的迟到pending不能重新开放"
        );
        assert!(
            ledger.reply(&question, &answers()).is_err(),
            "完成后不能再次提交"
        );
    }
}

/// 场景：原终端先答或取消，不能让已显示的旧建议继续发送；发送中断不能自动重试。
/// 构造：同一真实题面；原终端Blue响应、取消轮次、连接断开三种互斥事件。
/// 执行：每次重新登记问题，施加事件后尝试Green；另在发送Green后断开并观察迟到消费。
/// 预期：人工答案立即关闭本地入口；取消不可重开；断连保留Unknown，精确迟到回执可对账。
#[test]
fn native_answer_cancellation_and_disconnect_revoke_stale_submission() {
    for action in ["native", "cancel", "disconnect"] {
        let mut ledger = CodexQuestions::new("codex-question-test-thread".into());
        ledger.observe(&source()).unwrap();
        ledger.observe(&request()).unwrap();
        let question = ledger.take_changes().remove(0).question;
        match action {
            "native" => ledger.observe_native_response(&json!({"id":0,"result":{
                "answers":{"label":{"answers":["Blue"]}}
            }})).unwrap(),
            "cancel" => ledger.observe(&json!({"method":"turn/completed","params":{
                "threadId":"codex-question-test-thread","turn":{"id":"codex-question-test-turn","status":"interrupted"}
            }})).unwrap(),
            _ => ledger.disconnect(),
        }
        assert!(
            ledger.reply(&question, &answers()).is_err(),
            "{action}后旧答案不得写出"
        );
        ledger.observe(&request()).unwrap();
        let changes = ledger.take_changes();
        let expected = match action {
            "native" => QuestionState::Unknown,
            "cancel" => QuestionState::Cancelled,
            _ => QuestionState::Expired,
        };
        assert_eq!(changes.len(), 1, "{action}后只有一个终止事件");
        assert_eq!(changes[0].state, expected, "保留具体终止原因");
    }
    let mut ledger = CodexQuestions::new("codex-question-test-thread".into());
    ledger.observe(&source()).unwrap();
    ledger.observe(&request()).unwrap();
    let question = ledger.take_changes().remove(0).question;
    ledger.reply(&question, &answers()).unwrap();
    ledger.take_changes();
    ledger.disconnect();
    assert_eq!(
        ledger.take_changes()[0].state,
        QuestionState::Unknown,
        "可能已发送，不能宣称失败可重发"
    );
    assert!(
        ledger.reply(&question, &answers()).is_err(),
        "断连不能重复提交"
    );
    ledger.observe(&consumed()).unwrap();
    assert_eq!(
        ledger.take_changes()[0].state,
        QuestionState::Answered,
        "迟到精确结果用于对账而非重发"
    );
}

/// 场景：resolved或原生响应只能撤销抢答资格，不能替代精确消费证据。
/// 构造：分别发送resolved、原生Blue结果、原生RPC错误；之后补同工具Blue消费。
/// 执行：检查中间态、拒绝本地Green，再观察消费事件及证据。
/// 预期：中间态Unknown而非假称AnsweredElsewhere；后续工具结果仍可对账并保留证据。
#[test]
fn native_resolution_waits_for_consumption_instead_of_fabricating_an_answer() {
    for frame in [
        json!({"method":"serverRequest/resolved","params":{"threadId":"codex-question-test-thread","requestId":0}}),
        json!({"id":0,"result":{"answers":{"label":{"answers":["Blue"]}}}}),
        json!({"id":0,"error":{"code":-32000,"message":"cancelled"}}),
    ] {
        let mut ledger = CodexQuestions::new("codex-question-test-thread".into());
        ledger.observe(&source()).unwrap();
        ledger.observe(&request()).unwrap();
        let question = ledger.take_changes().remove(0).question;
        if frame.get("method").is_some() {
            ledger.observe(&frame).unwrap();
        } else {
            ledger.observe_native_response(&frame).unwrap();
        }
        assert_eq!(
            ledger.take_changes()[0].state,
            QuestionState::Unknown,
            "原生响应或resolved不证明答案已消费"
        );
        assert!(
            ledger.reply(&question, &answers()).is_err(),
            "原RPC已处理，必须禁止旧建议发送"
        );
        let mut blue = consumed();
        blue["params"]["item"]["output"] = json!({"answers":{"label":{"answers":["Blue"]}}})
            .to_string()
            .into();
        ledger.observe(&blue).unwrap();
        let changes = ledger.take_changes();
        assert_eq!(changes.len(), 1, "后来的消费不能因为中间态被丢弃");
        assert_eq!(
            changes[0].state,
            QuestionState::AnsweredElsewhere,
            "仅工具结果证明由别处处理"
        );
        assert_eq!(
            changes[0].evidence.as_deref(),
            Some(blue.to_string().as_str()),
            "完整保留消费证据"
        );
    }
}

/// 场景：原工具被别处回答、旧turn在结束后重放、同call来源改变都不得继续用旧授权。
/// 构造：Green已发送但工具回Blue；turn先结束后收到原pending；同call改为connector。
/// 执行：观察冲突证据并检查可答入口，确保终态不被迟到source/pending重开。
/// 预期：不同答案明确记为别处处理；旧轮次和冲突来源均不能再答。
#[test]
fn conflicting_consumption_and_retired_turn_never_reopen_a_request() {
    let mut ledger = CodexQuestions::new("codex-question-test-thread".into());
    ledger.observe(&source()).unwrap();
    ledger.observe(&request()).unwrap();
    let question = ledger.take_changes().remove(0).question;
    ledger.reply(&question, &answers()).unwrap();
    ledger.take_changes();
    let mut other = consumed();
    other["params"]["item"]["output"] = json!({"answers":{"label":{"answers":["Blue"]}}})
        .to_string()
        .into();
    ledger.observe(&other).unwrap();
    assert_eq!(
        ledger.take_changes().first().map(|change| change.state),
        Some(QuestionState::AnsweredElsewhere),
        "同call的另一答案不能永远留在Submitted，也不能冒充本次成功"
    );

    let mut retired = CodexQuestions::new("codex-question-test-thread".into());
    retired.observe(&json!({"method":"turn/completed","params":{
        "threadId":"codex-question-test-thread","turn":{"id":"codex-question-test-turn","status":"interrupted"}
    }})).unwrap();
    retired.observe(&source()).unwrap();
    retired.observe(&request()).unwrap();
    assert!(
        retired.take_changes().is_empty(),
        "结束后重放不能重新登记可答题"
    );
    assert!(
        retired.reply(&question, &answers()).is_err(),
        "已结束轮次不可答"
    );

    let mut conflicting = CodexQuestions::new("codex-question-test-thread".into());
    conflicting.observe(&source()).unwrap();
    conflicting.observe(&request()).unwrap();
    conflicting.take_changes();
    let mut changed = source();
    changed["params"]["item"]["name"] = "mcp__connector__delete_document".into();
    assert!(
        conflicting.observe(&changed).is_err(),
        "同call出现矛盾来源时停用观察器"
    );
    assert!(
        conflicting.reply(&question, &answers()).is_err(),
        "不能继续使用已经失去可信来源的原题"
    );
    let mut reverse = CodexQuestions::new("codex-question-test-thread".into());
    reverse.observe(&changed).unwrap();
    reverse.observe(&request()).unwrap();
    assert!(
        reverse.take_changes().is_empty(),
        "connector调用本身不可当问答"
    );
    assert!(
        reverse.observe(&source()).is_err(),
        "先connector后问答也必须发现矛盾来源"
    );
    assert!(
        reverse.reply(&question, &answers()).is_err(),
        "来源冲突不能因消息顺序而漏判"
    );
}

/// 场景：Codex重放原pending/source时emittedAtMs变化，语义相同不能误判来源冲突。
/// 构造：真实夹具分别附加两个不同发送时间；原method/id/params保持一致。
/// 执行：登记原题，重放变化时间的两类事件，再答原题。
/// 预期：只登记一次，连接仍可提交；改变工具来源的既有负例仍拒绝。
#[test]
fn replay_timestamps_do_not_change_question_identity() {
    let mut ledger = CodexQuestions::new("codex-question-test-thread".into());
    let mut pending = request();
    let mut origin = source();
    pending["emittedAtMs"] = 1000.into();
    origin["emittedAtMs"] = 1001.into();
    ledger.observe(&origin).unwrap();
    ledger.observe(&pending).unwrap();
    let question = ledger.take_changes().remove(0).question;
    pending["emittedAtMs"] = 2000.into();
    origin["emittedAtMs"] = 2001.into();
    assert!(
        ledger.observe(&origin).is_ok(),
        "来源发出时间变化不改变同一工具调用"
    );
    assert!(
        ledger.observe(&pending).is_ok(),
        "pending发出时间变化不改变同一RPC"
    );
    assert!(
        ledger.take_changes().is_empty(),
        "重放不能产生第二道题或关闭原题"
    );
    assert!(
        ledger.reply(&question, &answers()).is_ok(),
        "仍只回答原连接上的原RPC"
    );
}
