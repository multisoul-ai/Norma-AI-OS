use norma::cursor::{host::CursorQuestionHost, store::CursorQuestionStore};
use norma::protocol::{QuestionAnswer, QuestionState};
use runtime::{AgentHookRuntimeConfig, NodeInput, Runtime};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use terminal::{PtyBackend, PtyError, PtyEvent};
fn field(n: u8, bytes: &[u8]) -> Vec<u8> {
    let mut result = vec![n * 8 + 2];
    let mut length = bytes.len();
    while length >= 128 {
        result.push((length as u8 & 127) | 128);
        length >>= 7;
    }
    result.push(length as u8);
    result.extend(bytes);
    result
}
fn put(db: &Connection, bytes: &[u8]) -> String {
    let id = format!("{:x}", Sha256::digest(bytes));
    db.execute(
        "INSERT OR REPLACE INTO blobs VALUES(?1,?2)",
        params![id, bytes],
    )
    .unwrap();
    id
}
fn root(db: &Connection, bytes: &[u8]) {
    let id = put(db, bytes);
    let metadata = json!({"agentId":"session","latestRootBlobId":id}).to_string();
    let hex = metadata
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    db.execute("INSERT OR REPLACE INTO meta VALUES('0',?1)", [hex])
        .unwrap();
}
fn screen(focus: bool, selected: bool) -> Vec<u8> {
    format!("\x1b[2J\x1b[HQuestion 1 of 1\r\n1. Color?\r\n{} [ ] Blue\r\n{} [{}] Green\r\n  [ ] Other: (type to answer)\r\n↑/↓ option · ←/→ question · Space select · Enter next/submit · Esc to skip\r\n",if focus {" "}else{"›"},if focus {"›"}else{" "},if selected {"x"}else{" "}).into_bytes()
}
struct Form {
    sender: crossbeam_channel::Sender<PtyEvent>,
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
    swallow: bool,
}
impl PtyBackend for Form {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.writes.lock().unwrap().push(bytes.to_vec());
        let reply = match bytes {
            b"a" => b"\x1b[2J\x1b[HCursor Agent\r\n  -> ready\r\n  \xe2\x86\x92 Plan, search, build anything".to_vec(),
            b"\x1b[B" if self.swallow => {
                let mut output = screen(false, false);
                output.extend(b"outside status redrawn");
                output
            }
            b"\x1b[B" => screen(true, false),
            b" " => screen(true, true),
            b"\r" => b"\x1b[2J\x1b[HAnswer received".to_vec(),
            _ => screen(true, false),
        };
        self.sender.send(PtyEvent::Output(reply)).unwrap();
        Ok(())
    }
    fn resize(&mut self, _: u16, _: u16) -> Result<(), PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}

/// 场景：Cursor在同一PTY原题逐步选择，人工输入抢占后停止；最终状态依赖原store工具结果。
/// 构造：只含当前root的SQLite、原题单选、产生真实ANSI画面的可观察PTY；分别正常/首键后人工接管。
/// 执行：绑定受控host、采用Green、逐步drain；未写入结果前核对不为Answered，再提升原工具结果root。
/// 预期：正常分支只发Down/Space/Enter且原结果后Answered；人工分支只有首键加人工键，终态归于人工。
#[test]
fn cursor_original_terminal_answer_waits_for_store_consumption_and_yields_to_human() {
    for mode in 0..6 {
        let early_mobile = mode == 5;
        let takeover = mode == 1;
        let restrict = mode == 2;
        let cancel = mode == 3;
        let swallow = mode == 4;
        let config = tempfile::tempdir().unwrap();
        let workspace = "/tmp/cursor-runtime-test";
        let reader = CursorQuestionStore::for_session(config.path(), workspace, "session").unwrap();
        std::fs::create_dir_all(reader.path().parent().unwrap()).unwrap();
        let db = Connection::open(reader.path()).unwrap();
        db.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT);CREATE TABLE blobs(id TEXT PRIMARY KEY,data BLOB);").unwrap();
        let mut tool: Value = serde_json::from_str(include_str!(
            "../../norma/tests/fixtures/questions/cursor-tool-call.json"
        ))
        .unwrap();
        tool["args"]["questions"]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        root(
            &db,
            &field(
                4,
                json!({"role":"assistant","content":[tool.clone()]})
                    .to_string()
                    .as_bytes(),
            ),
        );
        let writes = Arc::new(Mutex::new(Vec::new()));
        let factory_writes = writes.clone();
        let mut runtime = Runtime::new_for_workspace(workspace);
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:9/agent-hooks".into(),
            token: "test".into(),
            workspace_id: "w".into(),
            workspace_route_id: "r".into(),
            norma_socket: "/tmp/unused.sock".into(),
        }));
        runtime.set_pty_factory(Box::new(move |_, _, _, sender| {
            sender.send(PtyEvent::Output(screen(false, false))).unwrap();
            Ok(Box::new(Form {
                sender,
                writes: factory_writes.clone(),
                swallow,
            }))
        }));
        let node = runtime
            .spawn_agent(
                canvas::AgentProfile::Cursor,
                canvas::CanvasRect::new(0., 0., 800., 400.),
                120,
                24,
            )
            .unwrap();
        let process = runtime.mobile_agent_run_id(node).unwrap().to_owned();
        let host =
            CursorQuestionHost::start(config.path(), workspace, "session", node, process).unwrap();
        runtime.adopt_cursor_question_host(host, "p").unwrap();
        let process = runtime.mobile_agent_run_id(node).unwrap().to_owned();
        assert!(
            runtime.observe_mobile_hook(&norma::agent_hook::AgentHookEvent {
                node_id: node,
                profile: norma::protocol::AgentProfile::Cursor,
                control_run_id: Some(process),
                provider_event: "UserPromptSubmit".into(),
                session_id: Some("session".into()),
                turn_id: Some("turn".into()),
                tool_name: None,
                message: None,
                raw_json: json!({}),
            }),
            "绑定原任务后回答其问题"
        );

        let deadline = Instant::now() + Duration::from_secs(6);
        while runtime.questions().is_empty() && Instant::now() < deadline {
            runtime.drain_all();
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(10));
        }
        let target = runtime.questions()[0].question.target.clone();
        assert!(runtime.native_question_available(&target), "当前原题可采用");
        assert!(
            runtime.send_agent_task_text(node, "wrong task").is_err(),
            "问答不能被新任务顶替"
        );
        let submission = runtime
            .claim_question_answer(
                &target,
                vec![QuestionAnswer {
                    key: "question-0".into(),
                    selected: vec!["option-1".into()],
                    text: None,
                }],
                "delivery",
            )
            .unwrap()
            .unwrap();
        let permitted = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let gate = permitted.clone();
        runtime
            .set_question_terminal_guard(
                &submission,
                Arc::new(move |write| {
                    if !gate.load(std::sync::atomic::Ordering::Acquire) {
                        return Err("question_context_restricted");
                    }
                    write()
                }),
            )
            .unwrap();
        runtime.deliver_native_question(&submission).unwrap();
        if early_mobile {
            let key = runtime
                .mobile_latest_turn_key(node)
                .expect("原题已有手机接管身份");
            let lease = runtime::TerminalInputLease::new(key.clone()).unwrap();
            runtime
                .claim_mobile_terminal_input(&key, &lease)
                .expect("首键前允许用户接管");
            runtime.release_mobile_terminal_input(&key, &lease).unwrap();
        }
        let wanted = if early_mobile {
            0
        } else if takeover || restrict || cancel || swallow {
            1
        } else {
            3
        };
        while writes.lock().unwrap().len() < wanted && Instant::now() < deadline {
            runtime.drain_all();
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            writes.lock().unwrap().len(),
            wanted,
            "正常流程有三个步骤，人工分支仅执行首键"
        );
        if takeover {
            runtime
                .send_node_input(
                    node,
                    NodeInput::Key(terminal::KeyInput {
                        key: terminal::Key::Up,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    }),
                )
                .unwrap();
        }
        if restrict || swallow || early_mobile {
            if restrict {
                permitted.store(false, std::sync::atomic::Ordering::Release);
            }
            let until = Instant::now() + Duration::from_millis(850);
            while Instant::now() < until {
                runtime.drain_all();
                runtime.drain_native_question_hosts("p");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(
                writes.lock().unwrap().len(),
                if early_mobile { 0 } else { 1 },
                "首键前手机短暂接管也永久撤销；约束/重绘不能触发额外输入"
            );
        }
        assert_ne!(
            runtime.question_snapshot(&target).unwrap().state,
            QuestionState::Answered,
            "终端提交不是消费证据"
        );
        let mut result = json!({"type":"tool-result","toolName":"AskQuestion","toolCallId":tool["toolCallId"],"result":"User questions responses:\nQuestion q_single: Selected option(s) green"});
        if cancel {
            result["result"] = "Questions skipped by user".into();
        }
        let id = put(
            &db,
            json!({"role":"tool","content":[result]})
                .to_string()
                .as_bytes(),
        );
        let bytes = (0..id.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&id[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        root(&db, &field(1, &bytes));
        let expected = if early_mobile {
            QuestionState::AnsweredElsewhere
        } else if cancel {
            QuestionState::Cancelled
        } else if takeover || restrict || swallow {
            QuestionState::ConsumedUnattributed
        } else {
            QuestionState::Answered
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        while runtime.question_snapshot(&target).unwrap().state != expected
            && Instant::now() < deadline
        {
            runtime.drain_all();
            runtime.drain_native_question_hosts("p");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            runtime.question_snapshot(&target).unwrap().state,
            expected,
            "只按同原题工具结果关闭并区分交付归属"
        );
        let expected_writes = if early_mobile {
            vec![]
        } else if restrict || cancel || swallow {
            vec![b"\x1b[B".to_vec()]
        } else if takeover {
            vec![b"\x1b[B".to_vec(), b"\x1b[A".to_vec()]
        } else {
            vec![b"\x1b[B".to_vec(), b" ".to_vec(), b"\r".to_vec()]
        };
        assert_eq!(
            *writes.lock().unwrap(),
            expected_writes,
            "一次一键且Space不是paste；人工接管无迟到输入"
        );
        assert!(!runtime.has_pending_question(node), "原题消费后解除占有");
        assert_ne!(
            runtime.agent_task_state(node),
            Some(runtime::AgentTaskState::WaitingQuestion),
            "已关闭题目不能仍投影等待问答"
        );
        if mode == 0 || takeover {
            let key = runtime
                .mobile_latest_turn_key(node)
                .expect("原任务有手机接管身份");
            let lease = runtime::TerminalInputLease::new(key.clone()).unwrap();
            let claim = runtime.claim_mobile_terminal_input(&key, &lease);
            if takeover {
                assert_eq!(
                    claim,
                    Err("desktop_input_pending"),
                    "后来人工输入不能被代答回执清空"
                );
            } else {
                assert!(
                    claim.is_ok(),
                    "代答成功应清理自有输入标记，手机可以接管:{claim:?}"
                );
            }
        }
        if cancel {
            tool["toolCallId"] = "next-original-call".into();
            root(
                &db,
                &field(
                    4,
                    json!({"role":"assistant","content":[tool]})
                        .to_string()
                        .as_bytes(),
                ),
            );
            let deadline = Instant::now() + Duration::from_secs(2);
            while runtime.questions().len() < 2 && Instant::now() < deadline {
                runtime.drain_native_question_hosts("p");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(
                runtime.questions().len(),
                2,
                "原题取消后新call必须可登记，旧active不能永久占槽"
            );
            assert_eq!(
                runtime.questions()[1].state,
                QuestionState::Pending,
                "新题独立等待"
            );
        }
    }
}

/// 场景：启动绑定文件只证明会话身份，原TUI仍停在工作区信任时不能接收自动任务。
/// 构造：受控Cursor宿主、信任界面PTY、尚不存在的新会话store。
/// 执行：尝试派任务；再经普通人工输入确认测试信任提示，观察原生空prompt后再派任务。
/// 预期：准备期无自动输入，人工入口可用；原TUI就绪后才允许一次任务粘贴。
#[test]
fn cursor_binding_does_not_treat_native_trust_prompt_as_task_ready() {
    let directory = tempfile::tempdir().unwrap();
    let writes = Arc::new(Mutex::new(Vec::new()));
    let observed = writes.clone();
    let workspace = "/tmp/cursor-startup";
    let mut runtime = Runtime::new_for_workspace(workspace);
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:9/agent-hooks".into(),
        token: "test".into(),
        workspace_id: "w".into(),
        workspace_route_id: "r".into(),
        norma_socket: "/tmp/unused.sock".into(),
    }));
    runtime.set_pty_factory(Box::new(move |_, _, _, sender| {
        sender
            .send(PtyEvent::Output(
                b"Workspace Trust Required\r\n[a] Trust this workspace".to_vec(),
            ))
            .unwrap();
        Ok(Box::new(Form {
            sender,
            writes: observed.clone(),
            swallow: false,
        }))
    }));
    let node = runtime
        .spawn_agent(
            canvas::AgentProfile::Cursor,
            canvas::CanvasRect::new(0., 0., 800., 400.),
            120,
            24,
        )
        .unwrap();
    let process = runtime.mobile_agent_run_id(node).unwrap().to_owned();
    let host =
        CursorQuestionHost::start(directory.path(), workspace, "fresh", node, process).unwrap();
    runtime.adopt_cursor_question_host(host, "p").unwrap();
    runtime.drain_all();
    runtime.drain_native_question_hosts("p");
    assert_eq!(
        runtime
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == node)
            .unwrap()
            .state,
        "preparing",
        "启动信任页面不能向手机投影Ready"
    );
    assert!(
        runtime.agent_task_input_held(node),
        "已绑定宿主不能代表原TUI输入就绪"
    );
    assert!(
        runtime
            .send_agent_task_text(node, "do not type into trust")
            .is_err(),
        "信任提示不接收自动任务"
    );
    assert!(writes.lock().unwrap().is_empty(), "准备期间没有误发字节");
    runtime
        .send_node_input(node, NodeInput::Text("a".into()))
        .unwrap();
    runtime.drain_all();
    runtime.drain_native_question_hosts("p");
    assert!(
        !runtime.agent_task_input_held(node),
        "实际原生prompt可用后才解除启动阻塞"
    );
    runtime.send_agent_task_text(node, "bounded task").unwrap();
    assert_eq!(
        writes.lock().unwrap().len(),
        2,
        "只有用户确认与就绪后的一个任务"
    );
}
