use super::*;

fn finish(runtime: &mut Runtime, question: &PendingQuestion, id: &str) {
    runtime.register_question(question.clone()).unwrap();
    let submission = runtime
        .claim_question_answer(&question.target, answers(), id)
        .unwrap()
        .unwrap();
    runtime.start_question_delivery(&submission).unwrap();
    runtime
        .confirm_question_answer(&submission, &answers(), "original tool result")
        .unwrap();
    runtime.acknowledge_question_receipt(&submission);
}

/// 场景：详情压缩仍需区别已消费请求与过期修订，且 RPC ID 可以在新 turn 复用。
/// 构造：一个已消费请求、一个从修订1升级到2的请求，再完成70题（超过64条保留量）。
/// 执行：压缩后尝试旧修订、修订3、跨连接重放和新turn复用ID。
/// 预期：旧版与已消费跨连接重放拒绝，新修订和不同turn仍可登记，详情有界。
#[test]
fn compressed_history_preserves_revision_and_turn_semantics() {
    let (mut runtime, original) = setup();
    finish(&mut runtime, &original, "first");
    let mut revision = original.clone();
    revision.target.request_id = QuestionRequestId::String("revision".into());
    runtime.register_question(revision.clone()).unwrap();
    let mut next = revision.clone();
    next.target.revision = 2;
    runtime.register_question(next.clone()).unwrap();
    for i in 0..70 {
        let mut question = original.clone();
        question.target.request_id = QuestionRequestId::String(format!("history-{i}"));
        finish(&mut runtime, &question, &format!("delivery-{i}"));
    }
    assert!(
        runtime.question(&original.target).is_none(),
        "旧消费详情已压缩"
    );
    assert!(
        runtime.question(&revision.target).is_none(),
        "旧过期修订已压缩"
    );
    assert!(
        runtime.questions().len() <= 66,
        "最多64条保留历史、一条待答及刚完成项"
    );
    assert!(
        runtime.register_question(revision).is_err(),
        "旧修订不能复活"
    );
    next.target.revision = 3;
    assert!(
        runtime.register_question(next).is_ok(),
        "压缩过期修订不能封禁新版"
    );
    let mut replay = original.clone();
    replay.target.connection_id = "reconnected".into();
    runtime.bind_question_connection(&replay.target).unwrap();
    assert!(
        runtime.register_question(replay.clone()).is_err(),
        "已消费身份跨连接仍防重放"
    );
    replay.target.turn_id = Some("new-turn".into());
    assert!(
        runtime.register_question(replay).is_ok(),
        "不同turn可以复用RPC ID"
    );
}
