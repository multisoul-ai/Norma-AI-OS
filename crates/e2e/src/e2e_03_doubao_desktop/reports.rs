use super::*;

/// 豆包 2.0 长任务：方舟仅获已接收回执，可信 Agent Stop 后才向 TTS 2.0 发送完成播报。
///
/// 数据构造（含关键数值的推导过程）：5 × 640 字节 = 100 ms ASR 音频；
/// 一个 create_agent 调用产生一个 task_id；SessionStart、UserPromptSubmit、Stop 属于同一任务。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 假 ASR 给最终派工转写，假方舟 Chat 请求 create_agent。
///   2. 桌面返回 accepted/task_id，TTS 只朗读已接收；此时不得朗读完成。
///   3. 注入可信 Agent Stop，TTS 2.0 收到含原 task_id 的完成播报。
///
/// 预期结果：任务完成只播报一次，且不会同时启动旧模式的 TTS。
#[test]
fn desktop_doubao_2_announces_only_verified_agent_completion() {
    let _lock = DOUBAO_TEST_LOCK
        .lock()
        .expect("豆包 e2e 不应并行修改连接地址");
    let asr_listener = TcpListener::bind("127.0.0.1:0").expect("假 ASR 应绑定本机端口");
    let asr_addr = asr_listener.local_addr().expect("假 ASR 应有地址");
    let http_listener = TcpListener::bind("127.0.0.1:0").expect("假方舟 HTTP 应绑定本机端口");
    let http_addr = http_listener.local_addr().expect("假 HTTP 应有地址");
    let asr_server =
        std::thread::spawn(move || {
            let (stream, _) = asr_listener.accept().expect("桌面应连接假 ASR");
            stream
                .set_read_timeout(Some(Duration::from_secs(8)))
                .expect("应设置有限等待");
            let mut socket = tungstenite::accept(stream).expect("ASR 握手应成功");
            assert_eq!(
                socket.read().expect("应有 ASR 配置").into_data()[1],
                0x11,
                "首帧应为配置"
            );
            socket
                .send(Message::Binary(
                    asr_response(json!({"code":0,"result":{"utterances":[]}})).into(),
                ))
                .expect("假 ASR 应确认配置");
            assert_eq!(
                socket.read().expect("应有音频").into_data()[1],
                0x21,
                "应上传 ASR 音频"
            );
            socket.send(Message::Binary(asr_response(json!({"code":0,"result":{"utterances":[
            {"text":"创建 Codex Agent 并派任务检查当前项目","end_time":1000,"definite":true}
        ]}})).into())).expect("假 ASR 应发最终转写");
            let _ = socket.read();
        });
    let (accepted_tx, accepted_rx) = mpsc::channel();
    let (spoken_tx, spoken_rx) = mpsc::channel();
    let http_server = std::thread::spawn(move || {
        let (path, first, stream) = read_request(&http_listener);
        assert_eq!(path, "/chat", "派工应到方舟 Chat");
        assert!(
            first["messages"][0]["content"]
                .as_str()
                .is_some_and(|text| text.contains("report-project")),
            "派工请求应含当前项目"
        );
        write_json(
            stream,
            json!({"choices":[{"finish_reason":"tool_calls","message":{
                "role":"assistant","content":null,"tool_calls":[{"id":"agent-one","type":"function","function":{
                    "name":"create_agent","arguments":"{\"profile\":\"codex\",\"task\":\"检查当前项目\"}"
                }}]
            }}]}),
        );
        let (path, second, stream) = read_request(&http_listener);
        assert_eq!(path, "/chat", "工具回执应返回方舟 Chat");
        let receipt = second["messages"]
            .as_array()
            .expect("应有消息")
            .iter()
            .find(|message| message["role"] == "tool")
            .expect("应有工具回执");
        let receipt: Value =
            serde_json::from_str(receipt["content"].as_str().expect("回执应有内容"))
                .expect("回执应为 JSON");
        assert_eq!(receipt["status"], "accepted", "Agent 派工只能先报告已接收");
        let task_id = receipt["task_id"].as_u64().expect("应有真实 task_id");
        let node_id = receipt["node_id"].as_u64().expect("应有真实 node_id");
        accepted_tx
            .send((task_id, node_id))
            .expect("应报告任务身份");
        write_json(
            stream,
            json!({"choices":[{"finish_reason":"stop","message":{
                "role":"assistant","content":"任务已接收。"
            }}]}),
        );
        let (path, first_tts, stream) = read_request(&http_listener);
        assert_eq!(path, "/tts", "接收回执应交给 TTS 2.0");
        assert_eq!(
            first_tts["req_params"]["text"], "任务已接收。",
            "不能提前声称任务完成"
        );
        let pcm = base64::engine::general_purpose::STANDARD.encode(vec![0_u8; 960]);
        write_lines(
            stream,
            &format!(
                "{}\n{}\n",
                json!({"code":0,"data":pcm}),
                json!({"code":20_000_000})
            ),
        );
        let (path, report_tts, stream) = read_request(&http_listener);
        assert_eq!(path, "/tts", "可信终态应直接交给 TTS 2.0");
        let text = report_tts["req_params"]["text"]
            .as_str()
            .expect("播报应有文本")
            .to_string();
        write_lines(
            stream,
            &format!(
                "{}\n{}\n",
                json!({"code":0,"data":pcm}),
                json!({"code":20_000_000})
            ),
        );
        spoken_tx.send(text).expect("应报告真实完成播报");
    });
    std::env::set_var("NORMA_DOUBAO_ASR_URL", format!("ws://{asr_addr}"));
    std::env::set_var("NORMA_DOUBAO_CHAT_URL", format!("http://{http_addr}/chat"));
    std::env::set_var("NORMA_DOUBAO_TTS_URL", format!("http://{http_addr}/tts"));
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
        app.configure_doubao_for_e2e("synthetic-ark-key");
        app.install_doubao_pcm_for_e2e(vec![
            PcmAudioChunk::new(
                PcmFormat::Pcm16,
                16_000,
                1,
                vec![0; 640]
            )
            .expect("20 毫秒 PCM 应有效");
            5
        ]);
        app.enable_agent_announcement_test_audio_for_e2e();
    });
    click(cx, "toolbar-settings");
    cx.simulate_keystrokes("cmd-e");
    let deadline = Instant::now() + Duration::from_secs(10);
    let (task_id, node_id) = loop {
        cx.run_until_parked();
        if let Ok(identity) = accepted_rx.try_recv() {
            break identity;
        }
        assert!(Instant::now() < deadline, "派工应在 10 秒内返回真实身份");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(spoken_rx.try_recv().is_err(), "仅有 accepted 不得播报完成");
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
    assert!(spoken_rx.try_recv().is_err(), "Agent 运行中仍不得播报完成");
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
            "可信 Stop 后应向 TTS 2.0 发送终态播报"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        spoken.contains(&format!("#{task_id} 已完成")),
        "播报应包含真实 task_id 与完成状态"
    );
    assert!(!spoken.contains("已接收"), "终态播报不得冒充接收回执");
    let legacy = app.read_with(cx, |app, _| app.spoken_agent_announcements_for_e2e());
    assert!(legacy.is_empty(), "豆包模式不得同时启动旧 TTS Agent 播报");
    cx.simulate_keystrokes("cmd-e");
    std::env::remove_var("NORMA_DOUBAO_ASR_URL");
    std::env::remove_var("NORMA_DOUBAO_CHAT_URL");
    std::env::remove_var("NORMA_DOUBAO_TTS_URL");
    asr_server.join().expect("假 ASR 不应失败");
    http_server.join().expect("假 HTTP 不应失败");
}
