#[path = "../support/brain_question.rs"]
mod support;
use crate::app::MultisoulApp;
use gpui::TestAppContext;
use norma::agent_hook::{parse_hook_event, HookQuestion, HookSource, QuestionHookStatus};
use norma::protocol::{QuestionAnswer, QuestionState};
use std::time::{Duration, Instant};

/// 场景：幕僚建议不能自行发送，用户采用后必须先持久化再回填原Hook。
/// 构造：隔离App/SQLite/echo PTY、真实Claude问答夹具和一条Green建议。
/// 执行：尝试无授权代答（拒绝），经用户采用入口提交并等待后台持久化，再投递消费回执。
/// 预期：建议不写Hook；一次提交产生durable intent；只有同题工具回执产生Answered持久记录。
#[test]
fn approved_question_uses_durable_original_hook_without_dispatching_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let (target, ticket, work) = source.update(cx, |app, cx| {
        let (target, ticket, answers) = make_question(app, cx, dir.path());
        app.propose_question_answer(
            target.clone(),
            answers.clone(),
            "Use the requested label".into(),
        )
        .unwrap();
        assert!(
            app.answer_question_from_brain(target.clone(), answers.clone(), cx)
                .is_err(),
            "模型建议和令牌不能自行生成用户授权"
        );
        assert_eq!(
            ticket.status(Instant::now()),
            QuestionHookStatus::Pending,
            "建议本身不触发原Hook"
        );
        let permit = app
            .authorize_question_answer(&target, answers.clone())
            .unwrap();
        let work = app.begin_authorized_answer(permit, cx).unwrap();
        assert!(
            app.authorize_question_answer(&target, answers).is_err(),
            "提交占有后禁止双击再次授权"
        );
        (target, ticket, work)
    });
    let (done, completed) = async_channel::bounded(1);
    cx.spawn(move |_| async move {
        work.await;
        done.send(()).await.unwrap();
    })
    .detach();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if completed.try_recv().is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "后台提交必须有界完成");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        ticket.status(Instant::now()),
        QuestionHookStatus::Ready,
        "持久化完成后才回填原Hook"
    );
    let store = storage::question_delivery_store::QuestionDeliveryStore::new(database);
    let receipt = store
        .get(&serde_json::to_value(&target).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        receipt.state,
        storage::question_delivery_store::QuestionDeliveryState::Unknown,
        "原Hook有答案时必须已有持久意图，不能提前宣称消费"
    );
    source.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.question(&target).unwrap().state,
            QuestionState::Submitted,
            "原题提交后仍等待消费证据"
        );
        assert!(
            app.agent_task_coordinator
                .as_ref()
                .is_none_or(|coordinator| coordinator.tasks().is_empty()),
            "采用答案不得派发新任务"
        );
    });
    ticket
        .take_response(Instant::now())
        .expect("原Hook应有回答");
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .unwrap();
    source.update(cx, |app, cx| {
        let raw = serde_json::from_str(include_str!(
            "../../../norma/tests/fixtures/questions/claude-post-tool-use.json"
        ))
        .unwrap();
        let mut post = parse_hook_event(HookSource::Claude, target.node_id, raw).unwrap();
        post.control_run_id = Some(target.run_id.clone());
        assert!(
            app.runtime.observe_native_question_result(&post),
            "真实同题消费回执应被接受"
        );
        app.brain_tick(&[], &[], cx);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        let receipt = store
            .get(&serde_json::to_value(&target).unwrap())
            .unwrap()
            .unwrap();
        if receipt.state == storage::question_delivery_store::QuestionDeliveryState::Answered {
            assert_eq!(
                receipt.evidence.as_deref(),
                Some("claude/PostToolUse:AskUserQuestion"),
                "回执只记录已关联的消费证据"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "原工具消费后必须异步持久化Answered，而非永久Unknown"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// 场景：宿主发现同一原题已有提交意图，不可当普通失败再试。
/// 构造：原题/答案相同的持久意图，模拟宿主已失去上一次发送资格。
/// 执行：用户再次采用，通过生产异步宿主服务检查旧意图。
/// 预期：不向Hook写入；状态为Unknown并保留首次delivery身份，等待原请求对账。
#[test]
fn prior_question_intent_is_unknown_and_never_resent() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let (target, ticket, answers) = source.update(cx, |app, cx| make_question(app, cx, dir.path()));
    let question = source.read_with(cx, |app, _| {
        app.runtime.question(&target).unwrap().question.clone()
    });
    let store = storage::question_delivery_store::QuestionDeliveryStore::new(database);
    store
        .reserve(&storage::question_delivery_store::QuestionDeliveryDraft {
            delivery_id: "original-delivery".into(),
            target: serde_json::to_value(&target).unwrap(),
            question: serde_json::to_value(question).unwrap(),
            answers: serde_json::to_value(&answers).unwrap(),
        })
        .unwrap();
    let work = source.update(cx, |app, cx| {
        let permit = app.authorize_question_answer(&target, answers).unwrap();
        app.begin_authorized_answer(permit, cx).unwrap()
    });
    let (done, completed) = async_channel::bounded(1);
    cx.spawn(move |_| async move {
        work.await;
        done.send(()).await.unwrap();
    })
    .detach();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if completed.try_recv().is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "检查已有意图必须有界完成");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        ticket.take_response(Instant::now()).is_none(),
        "已有持久意图不能重复写Provider"
    );
    source.read_with(cx, |app, _| {
        let snapshot = app.runtime.question(&target).unwrap();
        assert_eq!(
            snapshot.state,
            QuestionState::Unknown,
            "旧提交结果未知，不能冒充可重试失败"
        );
        assert_eq!(
            snapshot.delivery_id.as_deref(),
            Some("original-delivery"),
            "对账沿用原提交身份"
        );
        assert!(
            app.runtime.has_pending_question(target.node_id),
            "未知结果保持输入阻塞"
        );
    });
}

/// 场景：暂时失败且确认未调用Provider的准备操作，可在条件恢复后安全重试。
/// 构造：同题原Hook；分别让SQLite拒绝首次INSERT，或提交准备期间暂时切换scope。
/// 执行：等待首次失败、解除失败条件，再重新授权同一答案并等待提交。
/// 预期：首次失败不写Hook；重试只发送一次，复用已保留的未发送lease而不误用旧Unknown意图。
#[test]
fn unsent_question_preparation_can_retry_after_storage_or_scope_recovers() {
    for storage_fails in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
        let database = paths.app_db_path().to_path_buf();
        let mut test = TestAppContext::single();
        let (source, cx) = test.add_window_view(|_, cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        let (target, ticket, answers) =
            source.update(cx, |app, cx| make_question(app, cx, dir.path()));
        if storage_fails {
            rusqlite::Connection::open(&database).unwrap().execute_batch(
                "CREATE TRIGGER reject_question_intent BEFORE INSERT ON question_deliveries BEGIN SELECT RAISE(ABORT,'test storage failure'); END;"
            ).unwrap();
        }
        for retry in [false, true] {
            if retry {
                if storage_fails {
                    rusqlite::Connection::open(&database)
                        .unwrap()
                        .execute_batch("DROP TRIGGER reject_question_intent;")
                        .unwrap();
                }
                source.update(cx, |app, _| {
                    app.shared_context_scope.as_mut().unwrap().area_id = "a".into()
                });
            }
            let work = source.update(cx, |app, cx| {
                let permit = app
                    .authorize_question_answer(&target, answers.clone())
                    .expect("未发送且原Hook仍有效，应可重新授权");
                let work = app.begin_authorized_answer(permit, cx).unwrap();
                if !retry && !storage_fails {
                    app.shared_context_scope.as_mut().unwrap().area_id = "away".into();
                }
                work
            });
            let (done, completed) = async_channel::bounded(1);
            cx.spawn(move |_| async move {
                work.await;
                done.send(()).await.unwrap();
            })
            .detach();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                cx.run_until_parked();
                if completed.try_recv().is_ok() {
                    break;
                }
                assert!(Instant::now() < deadline, "准备或重试必须有界完成");
                std::thread::sleep(Duration::from_millis(1));
            }
            source.read_with(cx, |app, _| {
                assert_eq!(
                    app.runtime.question(&target).unwrap().state,
                    if retry {
                        QuestionState::Submitted
                    } else {
                        QuestionState::Failed
                    },
                    "首次已确认未发送，第二次安全提交"
                )
            });
            assert_eq!(
                ticket.status(Instant::now()),
                if retry {
                    QuestionHookStatus::Ready
                } else {
                    QuestionHookStatus::Pending
                },
                "首次失败绝不触发原Hook"
            );
        }
        assert!(
            ticket.take_response(Instant::now()).is_some(),
            "恢复条件后原Hook得到一次回答"
        );
        assert!(
            ticket.take_response(Instant::now()).is_none(),
            "重试不会生成第二份响应"
        );
        let receipt = storage::question_delivery_store::QuestionDeliveryStore::new(database)
            .get(&serde_json::to_value(&target).unwrap())
            .unwrap()
            .unwrap();
        source.read_with(cx, |app, _| {
            assert_eq!(
                app.runtime
                    .question(&target)
                    .unwrap()
                    .delivery_id
                    .as_deref(),
                Some(receipt.delivery_id.as_str()),
                "发送使用当前持久化的唯一意图身份"
            )
        });
    }
}

/// 场景：用户采用后离开当前界面，调用者丢弃等待句柄。
/// 构造：隔离App与原生问答，经过用户授权入口开始提交。
/// 执行：立即drop返回Task，运行前台调度器并读取原题状态和持久意图。
/// 预期：宿主持有已授权工作，仍只提交一次；不能永久卡在Submitting。
#[test]
fn authorized_question_work_outlives_its_waiter() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let (target, ticket) = source.update(cx, |app, cx| {
        let (target, ticket, answers) = make_question(app, cx, dir.path());
        let permit = app.authorize_question_answer(&target, answers).unwrap();
        drop(app.begin_authorized_answer(permit, cx).unwrap());
        (target, ticket)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        let state = source.read_with(cx, |app, _| app.runtime.question(&target).unwrap().state);
        if state == QuestionState::Submitted {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "已授权工作不能随等待者drop而永久卡在{state:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        ticket.status(Instant::now()),
        QuestionHookStatus::Ready,
        "离开界面仍完成原Hook的一次提交"
    );
    let store = storage::question_delivery_store::QuestionDeliveryStore::new(database);
    assert!(
        store
            .get(&serde_json::to_value(&target).unwrap())
            .unwrap()
            .is_some(),
        "离开界面也不能越过持久化前置条件"
    );
    assert!(
        ticket.take_response(Instant::now()).is_some(),
        "存在唯一回答"
    );
    assert!(
        ticket.take_response(Instant::now()).is_none(),
        "观察者取消不产生额外回答"
    );
}

pub(super) fn make_question(
    app: &mut MultisoulApp,
    cx: &mut gpui::Context<MultisoulApp>,
    root: &std::path::Path,
) -> (
    norma::protocol::QuestionRef,
    HookQuestion,
    Vec<QuestionAnswer>,
) {
    let raw = serde_json::from_str(include_str!(
        "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
    ))
    .unwrap();
    support::make_question_from_raw(app, cx, root, raw)
}
