use norma::agent_hook::{
    parse_hook_event, HookQuestion, HookQuestionRequest, HookSource, QuestionHookStatus,
    QUESTION_TIMEOUT,
};
use norma::protocol::QuestionAnswer;
use std::time::Instant;

fn request() -> HookQuestionRequest {
    let raw =
        serde_json::from_str(include_str!("fixtures/questions/claude-pre-tool-use.json")).unwrap();
    HookQuestionRequest {
        run_id: "owned-run".into(),
        event: parse_hook_event(HookSource::Claude, 7, raw).unwrap(),
    }
}

fn answer() -> Vec<QuestionAnswer> {
    vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }]
}

/// 场景：一个原生Hook只接受一次完整答案，取消或超时后不得响应迟到答案。
/// 构造：真实PreToolUse夹具对应的三个独立ticket，时间由Instant参数控制。
/// 执行：首个提交并取两次响应；第二个取消再提交；第三个越过120秒期限。
/// 预期：仅第一个取到一次Green输出；超时和取消没有输出，也不变成Responded。
#[test]
fn hook_ticket_is_single_use_and_revocable() {
    let now = Instant::now();
    let ticket = HookQuestion::new(request(), now).unwrap();
    assert_eq!(
        ticket.status(now),
        QuestionHookStatus::Pending,
        "新Hook等待宿主答题"
    );
    assert!(
        ticket.submit_answer(&answer(), now).unwrap(),
        "首次完整答案可提交"
    );
    assert!(
        !ticket.submit_answer(&answer(), now).unwrap(),
        "双击不能替换已占有答案"
    );
    let response = ticket.take_response(now).expect("首次应有回答");
    assert_eq!(
        response["hookSpecificOutput"]["updatedInput"]["answers"]
            ["Which label should the test use?"],
        "Green",
        "输出应对应当前题的选项"
    );
    assert!(ticket.take_response(now).is_none(), "同Hook只能回复一次");
    assert_eq!(
        ticket.status(now),
        QuestionHookStatus::Responding,
        "取走回答不代表HTTP写入完成"
    );
    assert!(
        ticket.with_response_write(now, || Ok(true)).unwrap(),
        "完整写入响应后才允许标记已传输"
    );
    assert_eq!(
        ticket.status(now),
        QuestionHookStatus::Responded,
        "完整传输状态可供回执归因"
    );
    let cancelled = HookQuestion::new(request(), now).unwrap();
    cancelled.cancel();
    assert!(
        !cancelled.submit_answer(&answer(), now).unwrap(),
        "取消赢过迟到答题"
    );
    assert!(
        cancelled.take_response(now).is_none(),
        "取消不生成allow输出"
    );
    let timed_out = HookQuestion::new(request(), now).unwrap();
    assert!(
        !timed_out
            .submit_answer(&answer(), now + QUESTION_TIMEOUT)
            .unwrap(),
        "期限边界禁止提交"
    );
    assert_eq!(
        timed_out.status(now + QUESTION_TIMEOUT),
        QuestionHookStatus::Cancelled,
        "超时取消等待句柄"
    );
}

/// 场景：HTTP部分写入后撤销，不得继续补齐旧问题的答案。
/// 构造：独立Hook与可控写入闭包；第一次模拟部分写入，第二次记录是否实际调用。
/// 执行：取走响应、部分写入、取消，再尝试写完。
/// 预期：未写完仍可取消；取消后的闭包不执行，状态不成为Responded。
#[test]
fn partial_response_remains_revocable_until_last_byte() {
    let now = Instant::now();
    let ticket = HookQuestion::new(request(), now).unwrap();
    ticket.submit_answer(&answer(), now).unwrap();
    assert!(ticket.take_response(now).is_some(), "传输可取走已授权答案");
    assert!(
        !ticket.with_response_write(now, || Ok(false)).unwrap(),
        "部分写入不能标记完成"
    );
    ticket.cancel();
    let mut wrote = false;
    assert!(
        ticket
            .with_response_write(now, || {
                wrote = true;
                Ok(true)
            })
            .is_err(),
        "撤销阻止后续写入"
    );
    assert!(!wrote, "撤销之后不能再执行写socket的副作用");
    assert_eq!(
        ticket.status(now),
        QuestionHookStatus::Cancelled,
        "部分响应不能成为已传输"
    );
}
