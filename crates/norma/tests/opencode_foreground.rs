use norma::opencode::{
    client::OpenCodeQuestionClient,
    native_questions::{OpenCodeNativeEvent, OpenCodeQuestionHost},
};
use serde_json::json;
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
#[path = "support/opencode_foreground.rs"]
mod support;
use support::*;

/// 场景：查询 A 的 pending 期间，用户已在原 TUI 切到有问题的 B。
/// 构造：A 无问题、B 有问题；分别暂停首次查询和已确认 A 空闲后的查询。
/// 执行：确认 A 查询挂起，写 B 路由；在响应前后持续检查输入锁直到 B 出题。
/// 预期：旧快照不能放行普通输入，不能登记 A 问题；只发现 B 且全程零 POST。
#[test]
fn foreground_change_during_pending_query_cannot_unlock_input() {
    for initially_blocked in [true, false] {
        let server = Server::new(false);
        server.hide_a.store(true, Ordering::Release);
        server
            .pending_gate
            .store(usize::from(initially_blocked), Ordering::Release);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("route.json");
        route(&path, 1, Some("ses_a"));
        let client = OpenCodeQuestionClient::new(
            &server.endpoint,
            "/tmp/original",
            "ses_a",
            "opencode",
            "secret",
        )
        .unwrap();
        let host = OpenCodeQuestionHost::start_following(
            client,
            7,
            "process".into(),
            path.clone(),
            "owned".into(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        if !initially_blocked {
            while host.input_pending() {
                assert!(Instant::now() < deadline, "先确认 A 无题且普通输入已开放");
                std::thread::sleep(Duration::from_millis(1));
            }
            server.pending_gate.store(1, Ordering::Release);
        }
        while server.pending_gate.load(Ordering::Acquire) != 2 {
            assert!(Instant::now() < deadline, "必须实际挂起 A 的 HTTP 查询");
            std::thread::sleep(Duration::from_millis(1));
        }
        route(&path, 2, Some("ses_b"));
        assert!(
            host.input_pending(),
            "查询挂起期间切换到 B，也必须锁定普通输入"
        );
        server.pending_gate.store(3, Ordering::Release);
        loop {
            assert!(
                host.input_pending(),
                "A 的迟到空结果不能解锁正在等待回答的 B"
            );
            if let Ok(OpenCodeNativeEvent::Question(q)) = host.events().try_recv() {
                assert_eq!(q.adapter().session_id(), "ses_b", "只登记当前 B 的问题");
                break;
            }
            assert!(Instant::now() < deadline, "最终应发现 B 的原题");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            server.posts.load(Ordering::Acquire),
            0,
            "切换查询不提交答案"
        );
    }
}

/// 场景：OpenCode 原 TUI 从 A 切 B，A 的 pending 仍在 server；两题题面完全相同。
/// 构造：唯一鉴权实例同时返回 A/B；私有 route 文件由原 TUI 报告 revision 和当前 session。
/// 执行：A 出题后写 B 路由，取得 B 的题；尝试迟到提交 A，再离开 session。
/// 预期：只发现当前会话，旧 ticket 立即失效；离开会话撤销 B，整个过程零 POST。
#[test]
fn foreground_switch_revokes_old_question_and_follows_current_session() {
    let server = Server::new(false);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("route.json");
    route(&path, 1, Some("ses_a"));
    let client = OpenCodeQuestionClient::new(
        &server.endpoint,
        "/tmp/original",
        "ses_a",
        "opencode",
        "secret",
    )
    .unwrap();
    let host = OpenCodeQuestionHost::start_following(
        client,
        7,
        "process".into(),
        path.clone(),
        "owned".into(),
    )
    .unwrap();
    let a = question(&host);
    assert_eq!(a.adapter().session_id(), "ses_a", "先观察前台 A");
    route(&path, 2, Some("ses_b"));
    let b = question(&host);
    assert_eq!(
        b.adapter().session_id(),
        "ses_b",
        "切换后只能提出前台 B 的答案建议"
    );
    assert_eq!(
        a.status(Instant::now()),
        norma::agent_hook::QuestionHookStatus::Cancelled,
        "A 仍在 server 也不能继续代答"
    );
    route(&path, 3, None);
    let deadline = Instant::now() + Duration::from_secs(2);
    while b.status(Instant::now()) == norma::agent_hook::QuestionHookStatus::Pending
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        b.status(Instant::now()),
        norma::agent_hook::QuestionHookStatus::Cancelled,
        "回首页撤销 B，不能沿用旧会话"
    );
    assert_eq!(
        server.posts.load(Ordering::Acquire),
        0,
        "路由变化不触发任何答题或派发"
    );
}

/// 场景：原答案 POST 已发送但接纳回包丢失，切换前台或心跳间断后原题仍在 pending。
/// 构造：两会话同题，服务丢弃 POST 回包且继续返回原 request；分别切 B/回 A 和撤销/恢复路由。
/// 执行：给 A 提交一次，确认发送归属 Unknown，然后重建 A 的前台资格。
/// 预期：可继续核对原消费，但不产生能再次回答 A 的新 ticket，POST 总数保持一。
#[test]
fn foreground_return_does_not_reopen_an_uncertain_original_delivery() {
    for next in [Some("ses_b"), None] {
        let server = Server::new(true);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("route.json");
        route(&path, 1, Some("ses_a"));
        let client = OpenCodeQuestionClient::new(
            &server.endpoint,
            "/tmp/original",
            "ses_a",
            "opencode",
            "secret",
        )
        .unwrap();
        let host = OpenCodeQuestionHost::start_following(
            client,
            7,
            "process".into(),
            path.clone(),
            "owned".into(),
        )
        .unwrap();
        let a = question(&host);
        let answer = vec![
            norma::protocol::QuestionAnswer {
                key: "question-0".into(),
                selected: vec!["option-1".into()],
                text: None,
            },
            norma::protocol::QuestionAnswer {
                key: "question-1".into(),
                selected: vec!["option-0".into()],
                text: None,
            },
            norma::protocol::QuestionAnswer {
                key: "question-2".into(),
                selected: vec![],
                text: Some("marker".into()),
            },
        ];
        assert!(
            a.submit_delivery(&answer, "once", Instant::now()).unwrap(),
            "首次回答已获资格"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while a.native_send_status() != norma::agent_hook::NativeSendStatus::Unknown
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            a.native_send_status(),
            norma::agent_hook::NativeSendStatus::Unknown,
            "必须确实复现 POST 回包丢失"
        );
        route(&path, 2, next);
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if let Ok(OpenCodeNativeEvent::ForegroundChanged { session, .. }) =
                host.events().recv_timeout(Duration::from_millis(50))
            {
                if session.as_deref() == next {
                    break;
                }
            }
            assert!(Instant::now() < deadline, "必须观察实际路由撤销/切换");
        }
        route(&path, 3, Some("ses_a"));
        let deadline = Instant::now() + Duration::from_millis(600);
        while Instant::now() < deadline {
            if let Ok(OpenCodeNativeEvent::Question(q)) =
                host.events().recv_timeout(Duration::from_millis(50))
            {
                assert_ne!(
                    q.adapter().session_id(),
                    "ses_a",
                    "同一原题已有未知发送，切回不能产生新的可发送 ticket"
                );
            }
        }
        assert_eq!(
            server.posts.load(Ordering::Acquire),
            1,
            "任何连接代次都不能重发原 POST"
        );
        server.hide_a.store(true, Ordering::Release);
        // Two 100ms polls plus scheduling margin, within the 1500ms route lease.
        std::thread::sleep(Duration::from_millis(250));
        assert!(
            host.input_pending(),
            "当前 A 的原题消失但消费未确认时，Unknown 发送仍占有输入"
        );
        server.consumed.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(OpenCodeNativeEvent::Changed {
                ticket_id,
                state,
                evidence,
                ..
            }) = host.events().recv_timeout(Duration::from_millis(50))
            {
                if ticket_id == a.id()
                    && state == norma::protocol::QuestionState::ConsumedUnattributed
                {
                    assert!(evidence.is_some(), "离开前台后仍须核对原 call 的真实消费");
                    break;
                }
            }
            assert!(Instant::now() < deadline, "切换不能丢失旧发送的消费对账");
        }
    }
}

/// 场景：原 TUI 的心跳过期，不能误认为用户离开了仍在等待的问题。
/// 构造：先发布有效 A 路由并取得问题，再把心跳时间设为零模拟观察通道断开。
/// 执行：等待 ticket 撤销并读取宿主实时输入占有状态。
/// 预期：代答资格撤销，但普通任务仍被阻断；没有可信前台证据不能把下一步粘贴进问答控件。
#[test]
fn missing_foreground_evidence_keeps_ordinary_input_blocked() {
    let server = Server::new(false);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("route.json");
    route(&path, 1, Some("ses_a"));
    let client = OpenCodeQuestionClient::new(
        &server.endpoint,
        "/tmp/original",
        "ses_a",
        "opencode",
        "secret",
    )
    .unwrap();
    let host = OpenCodeQuestionHost::start_following(
        client,
        7,
        "process".into(),
        path.clone(),
        "owned".into(),
    )
    .unwrap();
    let a = question(&host);
    std::fs::write(
        &path,
        json!({"nonce":"owned","revision":1,"session":"ses_a","updatedAt":0}).to_string(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while a.status(Instant::now()) == norma::agent_hook::QuestionHookStatus::Pending
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        a.status(Instant::now()),
        norma::agent_hook::QuestionHookStatus::Cancelled,
        "失去前台证据就停止代答"
    );
    assert!(
        host.input_pending(),
        "失联不等于离开问题，必须继续拦截普通任务输入"
    );
}

/// 场景：当前原生会话的 HTTP GET 暂时不可用，恢复后只能重新提供未发送的问题。
/// 构造：同一 server/session 的原题和可控 503；当前 route 保持 A，不执行任何 POST。
/// 执行：取得旧 ticket、令 GET 失败，确认输入锁保留；恢复服务并等待新的精确连接 ticket。
/// 预期：旧 ticket 永久取消，新 ticket 可供用户重新采用，前台会话不被替换、零 POST。
#[test]
fn reconnect_reconciles_pending_before_restoring_unsent_question() {
    let server = Server::new(false);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("route.json");
    route(&path, 1, Some("ses_a"));
    let client = OpenCodeQuestionClient::new(
        &server.endpoint,
        "/tmp/original",
        "ses_a",
        "opencode",
        "secret",
    )
    .unwrap();
    let host = OpenCodeQuestionHost::start_following(
        client,
        7,
        "process".into(),
        path.clone(),
        "owned".into(),
    )
    .unwrap();
    let a = question(&host);
    server.offline.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(1);
    while a.status(Instant::now()) == norma::agent_hook::QuestionHookStatus::Pending
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        a.status(Instant::now()),
        norma::agent_hook::QuestionHookStatus::Cancelled,
        "网络断连撤销旧 ticket"
    );
    assert!(host.input_pending(), "失联期间不开放普通输入");
    route(&path, 1, Some("ses_a"));
    server.offline.store(false, Ordering::Release);
    let recovered = question(&host);
    assert_eq!(
        recovered.adapter().session_id(),
        "ses_a",
        "恢复仍针对同一原生会话"
    );
    assert_ne!(
        recovered.adapter().connection_id("process"),
        a.adapter().connection_id("process"),
        "旧发送资格不能跨恢复复用"
    );
    assert_eq!(
        server.posts.load(Ordering::Acquire),
        0,
        "恢复查询不能自动提交"
    );
}

/// 场景：OpenCode发送前GET挂起期间撤权，不得沿用准备前的许可POST。
/// 构造：真实本地HTTP宿主、原题、可控GET阻塞与内存授权gate。
/// 执行：排队答案、确认GET挂起、撤权再放行GET，等待宿主终态事件。
/// 预期：明确Failed及原因、POST为零，原生题仍存在且不能重复代答。
#[test]
fn final_send_gate_rechecks_after_slow_prepare_get() {
    use norma::protocol::{QuestionAnswer, QuestionState};
    use std::sync::{atomic::AtomicBool, Arc};
    let server = Server::new(false);
    let client = OpenCodeQuestionClient::new(
        &server.endpoint,
        "/tmp/original",
        "ses_a",
        "opencode",
        "secret",
    )
    .unwrap();
    let host = OpenCodeQuestionHost::start(client, 7, "process".into()).unwrap();
    let ticket = question(&host);
    let revoked = Arc::new(AtomicBool::new(false));
    let gate = revoked.clone();
    ticket.set_send_guard(Arc::new(move |send| {
        if gate.load(Ordering::Acquire) {
            return Err("context_revoked");
        }
        send();
        Ok(())
    }));
    server.pending_gate.store(1, Ordering::Release);
    let answers = vec![
        QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-1".into(),
            selected: vec!["option-0".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-2".into(),
            selected: vec![],
            text: Some("test".into()),
        },
    ];
    ticket
        .submit_delivery(&answers, "once", Instant::now())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while server.pending_gate.load(Ordering::Acquire) != 2 {
        assert!(Instant::now() < deadline, "GET必须实际挂起");
        std::thread::sleep(Duration::from_millis(1));
    }
    revoked.store(true, Ordering::Release);
    server.pending_gate.store(3, Ordering::Release);
    loop {
        if let Ok(OpenCodeNativeEvent::Changed { state, error, .. }) = host.events().try_recv() {
            if state == QuestionState::Failed {
                assert_eq!(
                    error.as_deref(),
                    Some("context_revoked"),
                    "失败显式报告原因"
                );
                break;
            }
        }
        assert!(Instant::now() < deadline, "拒绝不能永久Submitting");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        server.posts.load(Ordering::Acquire),
        0,
        "撤权后POST必须为零"
    );
    assert!(host.input_pending(), "原题仍在等待人工处理");
    assert!(
        !ticket
            .submit_delivery(&answers, "again", Instant::now())
            .unwrap(),
        "拒绝后的票据不可重放"
    );
}
