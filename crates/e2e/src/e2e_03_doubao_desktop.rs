use crate::harness::{drain_until, echo_factory, enter_canvas, make_app, read_runtime};
use audio::{PcmAudioChunk, PcmFormat};
use base64::Engine as _;
use canvas::CanvasNodeKind;
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    time::{Duration, Instant},
};
use tungstenite::Message;

static DOUBAO_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

mod delete;
mod reports;

/// 豆包首次使用：已保存 Key 但麦克风未授权时，设置页必须提供授权入口。
///
/// 数据构造（含关键数值的推导过程）：1 个合成 Key；麦克风 Ready 项数从 0 变为 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，选择豆包并设置合成凭据，再模拟未授权麦克风。
///   2. 检查设置页提供授权按钮，且语音入口仍不可用。
///   3. 点击授权入口（E2E 的麦克风请求返回成功），重新检查入口与按钮状态。
///
/// 预期结果：未授权时有授权按钮且入口不可用；授权后入口 Ready，授权按钮消失。
#[test]
fn doubao_settings_offer_microphone_access_until_authorized() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应建立豆包设置测试项目");
    enter_canvas(&app, cx, workspace.path());
    click(cx, "toolbar-settings");
    click(cx, "settings-section-voice");
    click(cx, "settings-voice-desktop-mode-doubao");
    app.update(cx, |app, cx| {
        app.configure_doubao_for_e2e("synthetic-ark-key");
        app.set_voice_setup_requirements_for_e2e(false, false);
        cx.notify();
    });

    assert_eq!(
        app.update(cx, |app, _| app.voice_e2e_snapshot().entry_state),
        "unavailable",
        "麦克风未授权时豆包语音不得显示为可用"
    );
    assert!(
        cx.debug_bounds("settings-voice-doubao-microphone-action")
            .is_some(),
        "麦克风未授权时豆包设置页必须提供授权入口"
    );

    click(cx, "settings-voice-doubao-microphone-action");
    assert_eq!(
        app.update(cx, |app, _| app.voice_e2e_snapshot().entry_state),
        "ready",
        "Key 已保存且麦克风授权后豆包语音应可用"
    );
    assert!(
        cx.debug_bounds("settings-voice-doubao-microphone-action")
            .is_none(),
        "授权完成后不应继续显示授权入口"
    );
}

/// 桌面语音状态：底部声纹按听、想、说切换，通话界面不展示供应商名称。
///
/// 数据构造（含关键数值的推导过程）：回复阶段推进 160 ms；
///   160 ms × 1.2 cycles/s = 0.192 周期，足以观测一次声纹相位变化。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，在设置中选择豆包并配置合成凭据。
///   2. 依次注入 Connected、Thinking、Speaking、Disconnected、Idle 状态，读取 Dock 与真实渲染树。
///   3. 在聆听状态推进动画，确认不依赖字幕也会更新。
///
/// 预期结果：活跃状态显示通用状态文案与声纹；Idle 隐藏声纹；
///   所有 Dock 文案均不含 Doubao/豆包，包括底层错误信息含供应商名称时。
#[test]
fn doubao_voice_dock_uses_generic_labels_and_stateful_waveform() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应建立语音动画测试项目");
    enter_canvas(&app, cx, workspace.path());
    click(cx, "toolbar-settings");
    click(cx, "settings-section-voice");
    click(cx, "settings-voice-desktop-mode-doubao");
    app.update(cx, |app, cx| {
        app.configure_doubao_for_e2e("synthetic-ark-key");
        cx.notify();
    });
    click(cx, "toolbar-settings");

    app.update(cx, |app, cx| {
        app.show_doubao_status_for_e2e("connected");
        cx.notify();
    });
    cx.run_until_parked();
    let listening = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(listening.dock_label, "Listening", "聆听阶段应显示通用状态");
    assert!(
        cx.debug_bounds("workspace-voice-waveform").is_some(),
        "聆听阶段应渲染五柱声纹"
    );
    assert!(
        !listening.dock_label.contains("Doubao") && !listening.dock_label.contains("豆包"),
        "聆听文案不应展示供应商名称"
    );

    app.update(cx, |app, cx| {
        app.show_doubao_status_for_e2e("thinking");
        cx.notify();
    });
    cx.run_until_parked();
    let thinking = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(thinking.dock_label, "Thinking…", "思考阶段应显示通用状态");
    assert!(
        cx.debug_bounds("workspace-voice-waveform").is_some(),
        "思考阶段应保留声纹反馈"
    );
    assert!(
        !thinking.dock_label.contains("Doubao") && !thinking.dock_label.contains("豆包"),
        "思考文案不应展示供应商名称"
    );

    app.update(cx, |app, cx| {
        app.show_doubao_status_for_e2e("speaking");
        cx.notify();
    });
    cx.run_until_parked();
    let speaking = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(speaking.dock_label, "Replying…", "回复阶段应显示通用状态");
    assert!(
        cx.debug_bounds("workspace-voice-waveform").is_some(),
        "回复阶段应保留声纹反馈"
    );
    assert!(
        !speaking.dock_label.contains("Doubao") && !speaking.dock_label.contains("豆包"),
        "回复文案不应展示供应商名称"
    );
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(160), cx);
    });
    assert_ne!(
        app.read_with(cx, |app, _| app
            .voice_e2e_snapshot()
            .caption_waveform_phase_milli),
        0,
        "豆包语音 Dock 应在无字幕时持续推进声纹相位"
    );
    app.update(cx, |app, cx| {
        app.show_doubao_status_for_e2e("disconnected");
        cx.notify();
    });
    cx.run_until_parked();
    let disconnected = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(
        disconnected.label, "Voice disconnected",
        "底层连接错误含供应商名称时，Dock 应显示通用错误文案"
    );
    assert!(
        cx.debug_bounds("workspace-voice-waveform").is_none(),
        "断线状态不应继续显示活跃声纹"
    );
    app.update(cx, |app, cx| {
        app.show_doubao_status_for_e2e("idle");
        cx.notify();
    });
    cx.run_until_parked();
    let idle = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(idle.dock_label, "Voice", "空闲入口应使用通用名称");
    assert!(
        cx.debug_bounds("workspace-voice-waveform").is_none(),
        "空闲状态不应显示活跃声纹"
    );
}

/// 桌面豆包 2.0：设置切换后走 ASR 2.0 → 方舟 Chat → TTS 2.0，工具仍经现有桌面 Adapter。
///
/// 数据构造（含关键数值的推导过程）：5 × 640 字节 = 100 ms 的 16 kHz PCM16；
/// 方舟返回两次相同 call_id 的 create_terminal，预期仅创建一张 Terminal。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Settings 选择豆包并配置隔离 Keychain 凭据，注入 100 ms PCM。
///   2. ⌘E 启动；假 ASR 接收二进制配置与音频，返回口语化建卡指令。
///   3. 假方舟 Chat 检查现有人设、项目与工具，提出重复工具调用。
///   4. 桌面只执行一次工具，回传真实结果；假 Chat 回复，假 TTS 输出 PCM。
///
/// 预期结果：旧 Voice 回合保持空闲；ASR/Chat/TTS 均经过 2.0 路由；
/// Terminal 恰好一张，重复调用不能产生第二次副作用。
#[test]
fn desktop_doubao_2_settings_shortcut_audio_and_tool_dedup() {
    let _lock = DOUBAO_TEST_LOCK
        .lock()
        .expect("豆包 e2e 不应并行修改连接地址");
    let asr_listener = TcpListener::bind("127.0.0.1:0").expect("假 ASR 应绑定本机端口");
    let asr_addr = asr_listener.local_addr().expect("假 ASR 应有地址");
    let http_listener = TcpListener::bind("127.0.0.1:0").expect("假方舟 HTTP 应绑定本机端口");
    let http_addr = http_listener.local_addr().expect("假 HTTP 应有地址");
    let (audio_tx, audio_rx) = mpsc::channel();
    let asr_server = std::thread::spawn(move || {
        let (stream, _) = asr_listener.accept().expect("桌面应连接假 ASR");
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .expect("应设置有限等待");
        let mut socket = tungstenite::accept(stream).expect("ASR 握手应成功");
        let config = socket.read().expect("应收到 ASR 配置").into_data();
        assert_eq!(config[0], 0x11, "ASR 配置必须使用 V3 二进制协议");
        assert_eq!(config[1], 0x11, "首帧必须是 ASR 配置");
        socket
            .send(Message::Binary(
                asr_response(json!({"code":0,"result":{"utterances":[]}})).into(),
            ))
            .expect("假 ASR 应确认配置");
        let audio = socket.read().expect("应收到 100 毫秒音频").into_data();
        assert_eq!(audio[1], 0x21, "输入应是 ASR 二进制音频帧");
        audio_tx.send(true).expect("应报告音频已上传");
        socket
            .send(Message::Binary(
                asr_response(json!({"code":0,"result":{"utterances":[
                    {"text":"给我开一个 Terminal 卡片","end_time":1000,"definite":true}
                ]}}))
                .into(),
            ))
            .expect("假 ASR 应返回最终转写");
        let _ = socket.read();
    });
    let (http_tx, http_rx) = mpsc::channel();
    let http_server = std::thread::spawn(move || {
        let (path, first, stream) = read_request(&http_listener);
        assert_eq!(path, "/chat", "首个请求应到方舟 Chat");
        assert_eq!(
            first["model"], "doubao-seed-2.0-mini",
            "应使用方舟 2.0 模型"
        );
        assert!(
            first["messages"][0]["content"]
                .as_str()
                .is_some_and(
                    |text| text.contains("Norma Master Agent") && text.contains("doubao-project")
                ),
            "方舟请求必须复用 Norma 人设与当前项目"
        );
        assert!(
            first["tools"].as_array().is_some_and(|tools| tools
                .iter()
                .any(|tool| tool["function"]["name"] == "create_terminal")),
            "方舟请求必须声明现有 Terminal 工具"
        );
        let call = json!({"id":"terminal-one","type":"function","function":{
            "name":"create_terminal","arguments":"{}"}});
        write_json(
            stream,
            json!({"choices":[{"finish_reason":"tool_calls","message":{
                "role":"assistant","content":null,"tool_calls":[call.clone(),call]
            }}]}),
        );
        let (path, second, stream) = read_request(&http_listener);
        assert_eq!(path, "/chat", "工具回执后应回到方舟 Chat");
        let receipts: Vec<Value> = second["messages"]
            .as_array()
            .expect("应有消息")
            .iter()
            .filter(|message| message["role"] == "tool")
            .cloned()
            .collect();
        assert_eq!(receipts.len(), 2, "重复调用均应有对应回执");
        let receipt: Value =
            serde_json::from_str(receipts[0]["content"].as_str().expect("工具回执应为文本"))
                .expect("工具回执应为 JSON");
        assert_eq!(receipt["status"], "succeeded", "首个工具调用应真实成功");
        assert!(
            receipt["node_id"].as_u64().is_some(),
            "首个调用必须产生真实 node_id"
        );
        write_json(
            stream,
            json!({"choices":[{"finish_reason":"stop","message":{
                "role":"assistant","content":"已创建。"
            }}]}),
        );
        let (path, tts, stream) = read_request(&http_listener);
        assert_eq!(path, "/tts", "回复应交给 TTS 2.0");
        assert_eq!(
            tts["req_params"]["text"], "已创建。",
            "TTS 应朗读方舟真实回复"
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
        http_tx
            .send(receipt["node_id"].as_u64())
            .expect("应报告工具身份");
    });
    std::env::set_var("NORMA_DOUBAO_ASR_URL", format!("ws://{asr_addr}"));
    std::env::set_var("NORMA_DOUBAO_CHAT_URL", format!("http://{http_addr}/chat"));
    std::env::set_var("NORMA_DOUBAO_TTS_URL", format!("http://{http_addr}/tts"));
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let parent = tempfile::tempdir().expect("应建立隔离项目根目录");
    let project = parent.path().join("doubao-project");
    std::fs::create_dir(&project).expect("应建立项目目录");
    enter_canvas(&app, cx, &project);
    let before = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(before.desktop_mode, "legacy", "升级默认仍应是旧模式");
    click(cx, "toolbar-settings");
    click(cx, "settings-section-voice");
    click(cx, "settings-voice-desktop-mode-doubao");
    assert!(
        cx.debug_bounds("settings-voice-field-doubao-api-key")
            .is_some(),
        "豆包 Key 输入应可见"
    );
    assert!(
        cx.debug_bounds("settings-voice-doubao-system-input")
            .is_some(),
        "应说明使用系统音频设备"
    );
    assert!(
        cx.debug_bounds("settings-voice-input-device-picker")
            .is_none(),
        "豆包不应显示旧设备选择器"
    );
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
    let selected = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(selected.desktop_mode, "doubao", "设置应切到豆包");
    assert_eq!(
        selected.entry_state, "ready",
        "旧 ASR/TTS 缺配置不应阻断豆包"
    );
    click(cx, "toolbar-settings");
    cx.simulate_keystrokes("cmd-e");
    let deadline = Instant::now() + Duration::from_secs(10);
    let node_id = loop {
        cx.run_until_parked();
        if let Ok(node_id) = http_rx.try_recv() {
            break node_id;
        }
        assert!(
            Instant::now() < deadline,
            "2.0 假上游应在 10 秒内完成工具与 TTS 回执"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(audio_rx.try_recv().is_ok(), "ASR 2.0 应收到真实麦克风 PCM");
    assert!(node_id.is_some(), "工具应返回真实 Terminal node_id");
    let terminals = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
            .count()
    });
    assert_eq!(terminals, 1, "重复 call_id 只应创建一张 Terminal");
    assert_ne!(terminals, 2, "不能重复执行副作用");
    let after = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(
        after.dialogue_phase, "idle",
        "豆包工具不应启动第二个 Norma 模型回合"
    );
    cx.simulate_keystrokes("cmd-e");
    std::env::remove_var("NORMA_DOUBAO_ASR_URL");
    std::env::remove_var("NORMA_DOUBAO_CHAT_URL");
    std::env::remove_var("NORMA_DOUBAO_TTS_URL");
    asr_server.join().expect("假 ASR 不应失败");
    http_server.join().expect("假 HTTP 不应失败");
}

fn asr_response(value: Value) -> Vec<u8> {
    let payload = value.to_string();
    let mut frame = vec![0x11, 0x90, 0x10, 0];
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload.as_bytes());
    frame
}

fn read_request(listener: &TcpListener) -> (String, Value, TcpStream) {
    let (mut stream, _) = listener.accept().expect("应收到方舟 HTTP 请求");
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .expect("HTTP 应有限等待");
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut chunk = [0_u8; 4096];
        let count = stream.read(&mut chunk).expect("HTTP 请求应可读");
        assert!(count > 0, "HTTP 请求不应提前结束");
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let header = String::from_utf8_lossy(&bytes[..header_end]);
    let path = header
        .lines()
        .next()
        .expect("应有 HTTP 首行")
        .split_whitespace()
        .nth(1)
        .expect("应有请求路径")
        .to_string();
    let length: usize = header
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|value| value.trim().parse().ok())
        })
        .expect("JSON 请求应包含 Content-Length");
    while bytes.len() - header_end < length {
        let mut chunk = [0_u8; 4096];
        let count = stream.read(&mut chunk).expect("HTTP 请求体应可读");
        assert!(count > 0, "HTTP 请求体不应提前结束");
        bytes.extend_from_slice(&chunk[..count]);
    }
    let body: Value = serde_json::from_slice(&bytes[header_end..header_end + length])
        .expect("HTTP 请求体应是 JSON");
    (path, body, stream)
}

fn write_json(stream: TcpStream, body: Value) {
    write_lines(stream, &body.to_string());
}
fn write_lines(mut stream: TcpStream, body: &str) {
    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
    stream
        .write_all(response.as_bytes())
        .expect("应写回假方舟响应");
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("应渲染 {selector}"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
