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

/// 场景：Claude已部分写出时撤销授权，后续字节不得继续，且不能伪称安全未发送。
/// 构造：原Hook、可切换内存gate和部分写入闭包。
/// 执行：首段允许、撤权、尝试补齐；检查票据状态及实际写入次数。
/// 预期：第二段零写入，状态Unknown，保留明确拒绝原因且不能重放。
#[test]
fn claude_final_gate_stops_partial_response_without_false_unsent() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let now = Instant::now();
    let ticket = HookQuestion::new(request(), now).unwrap();
    let revoked = Arc::new(AtomicBool::new(false));
    let gate = revoked.clone();
    ticket.set_send_guard(Arc::new(move |send| {
        if gate.load(Ordering::Acquire) {
            return Err("revoked");
        }
        send();
        Ok(())
    }));
    ticket.submit_answer(&answer(), now).unwrap();
    ticket.take_response(now).unwrap();
    assert!(
        !ticket.with_response_write(now, || Ok(false)).unwrap(),
        "首段不完整"
    );
    revoked.store(true, Ordering::Release);
    let mut wrote = false;
    assert!(
        ticket
            .with_response_write(now, || {
                wrote = true;
                Ok(true)
            })
            .is_err(),
        "最终gate阻止补齐"
    );
    assert!(!wrote, "撤销后没有副作用");
    assert_eq!(
        ticket.native_send_status(),
        norma::agent_hook::NativeSendStatus::Unknown,
        "部分写出不能伪称未发送"
    );
    assert_eq!(ticket.send_rejection(), Some("revoked"), "拒绝原因保留");
    assert!(!ticket.transport_sent(), "不完整写出不是已发送");
}

/// 场景：Claude socket正常背压不能吞掉Hook的120秒期限。
/// 构造：部分响应后模拟WouldBlock，再把时钟推进到Hook期限。
/// 执行：先两次有界写入，再尝试到期补齐。
/// 预期：背压保持Writing，到期取消且不执行补齐闭包。
#[test]
fn claude_backpressure_preserves_expiry_after_partial_write() {
    let now = Instant::now();
    let ticket = HookQuestion::new(request(), now).unwrap();
    ticket.submit_answer(&answer(), now).unwrap();
    ticket.take_response(now).unwrap();
    ticket.with_response_write(now, || Ok(false)).unwrap();
    let result = ticket.with_response_write(now, || Err(std::io::ErrorKind::WouldBlock.into()));
    assert_eq!(
        result.unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "背压向poll报告可重试"
    );
    assert_eq!(
        ticket.native_send_status(),
        norma::agent_hook::NativeSendStatus::Writing,
        "正常背压仍是部分写出"
    );
    let mut wrote = false;
    assert!(
        ticket
            .with_response_write(now + QUESTION_TIMEOUT, || {
                wrote = true;
                Ok(true)
            })
            .is_err(),
        "到期禁止补齐"
    );
    assert!(!wrote, "到期后的副作用为零");
    assert_eq!(
        ticket.status(now + QUESTION_TIMEOUT),
        QuestionHookStatus::Cancelled,
        "Hook期限仍生效"
    );
}
