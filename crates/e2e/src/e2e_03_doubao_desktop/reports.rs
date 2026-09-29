use super::*;

/// 桌面豆包长任务：接收回执之后保持通话，只有可信 Agent Stop 才播报终态。
///
/// 数据构造（含关键数值的推导过程）：一个 create_agent(task) 生成一张 Codex 卡片
/// 和一个 task_id；SessionStart → UserPromptSubmit → Stop 是同一任务的三个真实 Hook。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 假上游发用户明确派工转写和 create_agent 函数调用。
///   2. 应用返回 accepted/task_id，通话继续；此时不得产生终态播报。
///   3. 注入可信 Codex Hook 终态，应用向同一豆包会话发送播报文本。
///   4. 检查原有独立 TTS 播报没有同时启动。
///
/// 预期结果：播报必须包含真实 task_id 与已完成，不能在仅 accepted 时发送或双重播报。
#[test]
fn desktop_doubao_announces_only_verified_agent_completion() {
    let _lock = DOUBAO_TEST_LOCK
        .lock()
        .expect("豆包 e2e 不应并行修改连接地址");
    let listener = TcpListener::bind("127.0.0.1:0").expect("假豆包服务应绑定本机端口");
    let address = listener.local_addr().expect("假服务应有本机地址");
    let (accepted_tx, accepted_rx) = mpsc::channel();
    let (spoken_tx, spoken_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("桌面应连接假豆包服务");
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .expect("应设置有限等待");
        let mut socket = tungstenite::accept(stream).expect("本机握手应成功");
        let created: Value = serde_json::from_str(
            socket
                .read()
                .expect("应收到会话配置")
                .to_text()
                .expect("配置应为文本"),
        )
        .expect("配置应是 JSON");
        assert_eq!(created["type"], "session.create", "应先发送豆包会话配置");
        socket
            .send(Message::Text(
                json!({"type":"session.created","session":{"id":"report-session"}})
                    .to_string()
                    .into(),
            ))
            .expect("应确认会话");
        socket.send(Message::Text(json!({"type":"conversation.item.input_audio_transcription.completed","transcript":"创建 Codex Agent 并派任务检查当前项目"}).to_string().into()))
            .expect("应发送真实派工转写");
        socket.send(Message::Text(json!({"type":"response.function_call_arguments.done","items":[{"call_id":"agent-one","name":"create_agent","arguments":"{\"profile\":\"codex\",\"task\":\"检查当前项目\"}"}]}).to_string().into()))
            .expect("应发送豆包工具调用");
        loop {
            let event: Value = serde_json::from_str(
                socket
                    .read()
                    .expect("应收到接收回执")
                    .to_text()
                    .expect("回执应为文本"),
            )
            .expect("回执应是 JSON");
            if event["type"] != "conversation.item.create" {
                continue;
            }
            assert_eq!(
                event["items"][0]["call_id"], "agent-one",
                "接收回执应绑定原调用 ID"
            );
            let receipt: Value = serde_json::from_str(
                event["items"][0]["content"][0]["text"]
                    .as_str()
                    .expect("应有回执文本"),
            )
            .expect("工具回执应是 JSON");
            assert_eq!(receipt["status"], "accepted", "Agent 只能先报告已接收");
            let task_id = receipt["task_id"].as_u64().expect("真实回执应有 task_id");
            let node_id = receipt["node_id"].as_u64().expect("真实回执应有 node_id");
            accepted_tx
                .send((task_id, node_id))
                .expect("应报告任务身份");
            break;
        }
        loop {
            let event: Value = serde_json::from_str(
                socket
                    .read()
                    .expect("应收到可信终态播报")
                    .to_text()
                    .expect("播报应为文本"),
            )
            .expect("播报应是 JSON");
            if event["type"] == "speech_text_buffer.commit" {
                spoken_tx
                    .send(event["text"].as_str().unwrap_or_default().to_string())
                    .expect("应报告播报内容");
                break;
            }
        }
        socket
            .send(Message::Text(
                json!({"type":"session.closed"}).to_string().into(),
            ))
            .expect("应关闭会话");
    });
    std::env::set_var("NORMA_DOUBAO_WS_URL", format!("ws://{address}"));
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let parent = tempfile::tempdir().expect("应建立隔离项目根目录");
    let project = parent.path().join("report-project");
    std::fs::create_dir(&project).expect("应建立项目目录");
    enter_canvas(&app, cx, &project);
    click(cx, "toolbar-settings");
    click(cx, "settings-section-voice");
    click(cx, "settings-voice-desktop-mode-doubao");
    app.update(cx, |app, _| {
        app.configure_doubao_for_e2e("synthetic-doubao-key");
        app.install_doubao_pcm_for_e2e(Vec::new());
        app.enable_agent_announcement_test_audio_for_e2e();
    });
    click(cx, "toolbar-settings");
    cx.simulate_keystrokes("cmd-e");
    let deadline = Instant::now() + Duration::from_secs(8);
    let (task_id, node_id) = loop {
        cx.run_until_parked();
        if let Ok(identity) = accepted_rx.try_recv() {
            break identity;
        }
        assert!(Instant::now() < deadline, "派工应在 8 秒内返回真实身份");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        spoken_rx.try_recv().is_err(),
        "仅有 accepted 回执时不得播报完成"
    );
    let hook = |name: &str, extra: Value| {
        let mut payload = json!({"hook_event_name":name,"session_id":"codex-doubao-report"});
        payload
            .as_object_mut()
            .expect("Hook 应为对象")
            .extend(extra.as_object().expect("附加字段应为对象").clone());
        norma::agent_hook::parse_hook_event(norma::agent_hook::HookSource::Codex, node_id, payload)
            .expect("可信 Hook 应可解析")
    };
    app.update(cx, |app, _| {
        app.enqueue_agent_hook_event_for_e2e(hook("SessionStart", json!({})))
            .expect("SessionStart 应进入队列")
    });
    drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_session_id(node_id).as_deref() == Some("codex-doubao-report"))
                .then_some(())
        },
        Duration::from_millis(500),
    )
    .expect("SessionStart 应建立 Agent 会话");
    std::thread::sleep(Duration::from_millis(120));
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(16));
    cx.run_until_parked();
    app.update(cx, |app, _| {
        app.enqueue_agent_hook_event_for_e2e(hook(
            "UserPromptSubmit",
            json!({"prompt":"检查当前项目"}),
        ))
        .expect("提交事件应进入队列")
    });
    drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node_id) == Some(runtime::AgentTaskState::Thinking))
                .then_some(())
        },
        Duration::from_millis(500),
    )
    .expect("Agent 应进入运行状态");
    assert!(spoken_rx.try_recv().is_err(), "运行中仍不得播报完成");
    app.update(cx, |app, _| {
        app.enqueue_agent_hook_event_for_e2e(hook("Stop", json!({"final_response":"检查完成"})))
            .expect("Stop 应进入队列")
    });
    drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node_id) == Some(runtime::AgentTaskState::Done)).then_some(())
        },
        Duration::from_millis(500),
    )
    .expect("可信 Stop 应推进到 Done");
    let spoken = loop {
        cx.run_until_parked();
        if let Ok(spoken) = spoken_rx.try_recv() {
            break spoken;
        }
        assert!(
            Instant::now() < deadline,
            "可信 Stop 后应向豆包发送终态播报"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        spoken.contains(&format!("#{task_id} 已完成")),
        "播报应包含真实 task_id 与完成状态"
    );
    assert!(!spoken.contains("已接收"), "终态播报不得把接收回执冒充结果");
    let legacy_spoken = app.read_with(cx, |app, _| app.spoken_agent_announcements_for_e2e());
    assert!(
        legacy_spoken.is_empty(),
        "豆包模式不得同时启动旧 TTS Agent 播报"
    );
    assert!(
        !app.read_with(cx, |app, _| app.agent_announcement_active_for_e2e()),
        "豆包模式不得遗留旧播报播放状态"
    );
    std::env::remove_var("NORMA_DOUBAO_WS_URL");
    server.join().expect("假豆包服务不应失败");
}
