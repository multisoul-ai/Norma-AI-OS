use super::*;
use crate::harness::create_terminal;

/// 豆包语音删除卡片：口语化指令直接删除指定目标，无二次授权。
///
/// 数据构造（含关键数值的推导过程）：2 张 Terminal；5 × 640 字节 = 100 ms PCM16；
/// 1 次 delete_canvas_card(position=2) 后应剩 2 - 1 = 1 张。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 建立两张真实 Canvas Terminal，并以 ⌘E 启动豆包。
///   2. 假 ASR 返回“把第二张卡片去掉”，假方舟调用位置 2 的删除工具。
///   3. 检查工具回执、画布节点和 TTS 回复。
///
/// 预期结果：第二张被删除，第一张保留；工具成功且没有授权拒绝。
#[test]
fn desktop_doubao_deletes_spoken_card_without_second_authorization() {
    let _lock = DOUBAO_TEST_LOCK.lock().expect("豆包测试锁不应中毒");
    let asr_listener = TcpListener::bind("127.0.0.1:0").expect("假 ASR 应绑定端口");
    let asr_addr = asr_listener.local_addr().expect("假 ASR 应有地址");
    let http_listener = TcpListener::bind("127.0.0.1:0").expect("假方舟应绑定端口");
    let http_addr = http_listener.local_addr().expect("假方舟应有地址");
    let asr_server = std::thread::spawn(move || {
        let (stream, _) = asr_listener.accept().expect("桌面应连接假 ASR");
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .expect("ASR 读取应有超时");
        let mut socket = tungstenite::accept(stream).expect("ASR 握手应成功");
        assert_eq!(
            socket.read().expect("应有 ASR 配置").into_data()[1],
            0x11,
            "首帧应为 ASR 配置"
        );
        socket
            .send(Message::Binary(
                asr_response(json!({"code":0,"result":{"utterances":[]}})).into(),
            ))
            .expect("假 ASR 应确认配置");
        assert_eq!(
            socket.read().expect("应有音频").into_data()[1],
            0x21,
            "桌面应发送麦克风音频"
        );
        socket
            .send(Message::Binary(
                asr_response(json!({"code":0,"result":{"utterances":[
                    {"text":"把第二张卡片去掉","end_time":1000,"definite":true}
                ]}}))
                .into(),
            ))
            .expect("假 ASR 应发送最终转写");
        let _ = socket.read();
    });
    let (receipt_tx, receipt_rx) = mpsc::channel();
    let http_server = std::thread::spawn(move || {
        let (path, first, stream) = read_request(&http_listener);
        assert_eq!(path, "/chat", "语音回合应请求方舟 Chat");
        assert!(
            first["messages"][0]["content"]
                .as_str()
                .is_some_and(|text| text.contains("无需再次询问授权")),
            "豆包提示词应允许用户当前指令直接调用工具"
        );
        write_json(
            stream,
            json!({"choices":[{"finish_reason":"tool_calls","message":{
                "role":"assistant","content":null,"tool_calls":[{"id":"delete-two","type":"function","function":{
                    "name":"delete_canvas_card","arguments":"{\"position\":2}"
                }}]
            }}]}),
        );
        let (path, second, stream) = read_request(&http_listener);
        assert_eq!(path, "/chat", "删除回执应送回方舟 Chat");
        let receipt = second["messages"]
            .as_array()
            .expect("应有消息数组")
            .iter()
            .find(|message| message["role"] == "tool")
            .expect("应有删除工具回执");
        let receipt: Value =
            serde_json::from_str(receipt["content"].as_str().expect("回执内容应为文本"))
                .expect("回执应为 JSON");
        receipt_tx.send(receipt).expect("应报告删除回执");
        write_json(
            stream,
            json!({"choices":[{"finish_reason":"stop","message":{
                "role":"assistant","content":"已删除第二张卡片。"
            }}]}),
        );
        let (path, _, stream) = read_request(&http_listener);
        assert_eq!(path, "/tts", "删除结果应交给 TTS 2.0");
        let pcm = base64::engine::general_purpose::STANDARD.encode(vec![0_u8; 960]);
        write_lines(
            stream,
            &format!(
                "{}\n{}\n",
                json!({"code":0,"data":pcm}),
                json!({"code":20_000_000})
            ),
        );
    });
    std::env::set_var("NORMA_DOUBAO_ASR_URL", format!("ws://{asr_addr}"));
    std::env::set_var("NORMA_DOUBAO_CHAT_URL", format!("http://{http_addr}/chat"));
    std::env::set_var("NORMA_DOUBAO_TTS_URL", format!("http://{http_addr}/tts"));
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应建立隔离项目");
    enter_canvas(&app, cx, workspace.path());
    let first = create_terminal(&app, cx);
    let second = create_terminal(&app, cx);
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
    });
    click(cx, "toolbar-settings");
    cx.simulate_keystrokes("cmd-e");
    let deadline = Instant::now() + Duration::from_secs(10);
    let receipt = loop {
        cx.run_until_parked();
        if let Ok(receipt) = receipt_rx.try_recv() {
            break receipt;
        }
        assert!(Instant::now() < deadline, "删除工具应在 10 秒内返回回执");
        std::thread::sleep(Duration::from_millis(20));
    };
    let (first_exists, second_exists, count) = read_runtime(&app, cx, |runtime| {
        (
            runtime.document.node(first).is_some(),
            runtime.document.node(second).is_some(),
            runtime.document.nodes().len(),
        )
    });
    assert_eq!(receipt["status"], "succeeded", "删除回执不应是授权拒绝");
    assert!(first_exists, "第一张卡片必须保留");
    assert!(!second_exists, "第二张卡片必须删除");
    assert_eq!(count, 1, "两张卡片删除一张后应只剩一张");
    cx.simulate_keystrokes("cmd-e");
    std::env::remove_var("NORMA_DOUBAO_ASR_URL");
    std::env::remove_var("NORMA_DOUBAO_CHAT_URL");
    std::env::remove_var("NORMA_DOUBAO_TTS_URL");
    asr_server.join().expect("假 ASR 不应失败");
    http_server.join().expect("假 HTTP 不应失败");
}
