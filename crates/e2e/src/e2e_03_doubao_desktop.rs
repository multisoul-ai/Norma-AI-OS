use crate::harness::{drain_until, echo_factory, enter_canvas, make_app, read_runtime};
use audio::{PcmAudioChunk, PcmFormat};
use canvas::CanvasNodeKind;
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use std::{
    net::TcpListener,
    sync::mpsc,
    time::{Duration, Instant},
};
use tungstenite::Message;

mod reports;

static DOUBAO_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 桌面豆包模式：从 Voice Settings 切换并按 ⌘E 后，假上游收到现有提示词、音频与真实工具回执。
///
/// 数据构造（含关键数值的推导过程）：640 字节 = 16,000 采样/秒 × 2 字节 × 20 毫秒；
/// 两个相同 call_id 的 create_terminal 事件只应创建一张 Terminal 卡片；
/// 新一轮说话开始后，尚未收到该轮最终转写时不得沿用前一轮授权。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Settings 点击豆包模式，保存仅供测试的 Keychain 凭据；旧 ASR/TTS 均未配置。
///   2. 注入 20 毫秒 PCM，按 ⌘E，假豆包服务接收 session.create 与音频。
///   3. 假服务发最终转写和重复工具调用；桌面 Adapter 创建 Terminal 并回传 call_id。
///   4. 本轮回复结束后，用新 call_id 重放旧指令 → 拒绝。
///   5. 假服务开始播报，用户插话 → 取消旧回复；未发新转写的写操作被拒绝。
///
/// 预期结果：模式与独立就绪状态正确、提示词含项目、音频上传、工具只执行一次；
/// 旧 ASR/Agent 回合仍为空闲，重复调用和新一轮缺转写不能增加第二张卡片。
#[test]
fn desktop_doubao_settings_shortcut_audio_and_tool_dedup() {
    let _lock = DOUBAO_TEST_LOCK
        .lock()
        .expect("豆包 e2e 不应并行修改连接地址");
    let listener = TcpListener::bind("127.0.0.1:0").expect("假豆包服务应绑定本机端口");
    let address = listener.local_addr().expect("假服务应有本机地址");
    let (report_tx, report_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("桌面应连接假豆包服务");
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .expect("应设置有限等待");
        let mut socket = tungstenite::accept(stream).expect("本机握手应成功");
        let session: Value = serde_json::from_str(
            socket
                .read()
                .expect("应收到配置")
                .to_text()
                .expect("配置应是文本"),
        )
        .expect("配置应是 JSON");
        assert_eq!(session["type"], "session.create", "首条消息应建立豆包会话");
        assert!(
            session["session"]["instructions"]
                .as_str()
                .is_some_and(|text| text.contains("Norma Master Agent")),
            "应复用 Norma 人设"
        );
        assert!(
            session["session"]["instructions"]
                .as_str()
                .is_some_and(|text| text.contains("doubao-project")),
            "应含当前项目"
        );
        assert!(
            session["session"]["tools"]
                .as_array()
                .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == "create_terminal")),
            "应声明原有工具"
        );
        socket
            .send(Message::Text(
                json!({"type":"session.created","session":{"id":"fake-session"}})
                    .to_string()
                    .into(),
            ))
            .expect("应发送连接回执");
        socket.send(Message::Text(json!({"type":"response.function_call_arguments.done","items":[{"call_id":"no-transcript","name":"create_terminal","arguments":"{}"}]}).to_string().into()))
            .expect("应先发送缺少真实转写的写操作");
        let mut audio_seen = false;
        loop {
            let event: Value = serde_json::from_str(
                socket
                    .read()
                    .expect("应收到拒绝回执")
                    .to_text()
                    .expect("事件应是文本"),
            )
            .expect("事件应是 JSON");
            if event["type"] == "input_audio_buffer.append" {
                audio_seen = true;
                continue;
            }
            if event["type"] != "conversation.item.create" {
                continue;
            }
            assert_eq!(
                event["items"][0]["call_id"], "no-transcript",
                "缺少转写的回执必须关联原调用"
            );
            let rejected: Value = serde_json::from_str(
                event["items"][0]["content"][0]["text"]
                    .as_str()
                    .expect("应有拒绝文本"),
            )
            .expect("拒绝回执应是 JSON");
            assert_eq!(
                rejected["status"], "rejected",
                "没有用户最终转写时不得执行写操作"
            );
            assert!(
                rejected["node_id"].is_null(),
                "拒绝调用不得创建 Terminal node_id"
            );
            break;
        }
        socket.send(Message::Text(json!({"type":"conversation.item.input_audio_transcription.completed","transcript":"创建一个 Terminal 卡片"}).to_string().into())).expect("应发送真实转写");
        let call = json!({"type":"response.function_call_arguments.done","items":[{"call_id":"terminal-one","name":"create_terminal","arguments":"{}"},{"call_id":"terminal-one","name":"create_terminal","arguments":"{}"}]});
        socket
            .send(Message::Text(call.to_string().into()))
            .expect("应发送重复工具事件");
        let mut result_count = 0;
        let mut accepted_node = None;
        while result_count < 2 || !audio_seen {
            let message = socket.read().expect("应收到音频或工具回执");
            let event: Value = serde_json::from_str(message.to_text().expect("事件应是文本"))
                .expect("事件应是 JSON");
            match event["type"].as_str() {
                Some("input_audio_buffer.append") => {
                    audio_seen = true;
                    assert!(
                        event["audio"]
                            .as_str()
                            .is_some_and(|audio| !audio.is_empty()),
                        "20 毫秒 PCM 应上传"
                    );
                }
                Some("conversation.item.create") => {
                    result_count += 1;
                    assert_eq!(
                        event["items"][0]["call_id"], "terminal-one",
                        "回执必须关联原调用 ID"
                    );
                    assert_eq!(
                        event["items"][0]["role"], "tool",
                        "豆包回执必须使用 tool 角色"
                    );
                    assert_eq!(
                        event["items"][0]["content"][0]["type"], "input_text",
                        "豆包回执必须使用 input_text 内容"
                    );
                    let receipt: Value = serde_json::from_str(
                        event["items"][0]["content"][0]["text"]
                            .as_str()
                            .expect("回执应为 JSON 字符串"),
                    )
                    .expect("回执应可解析");
                    if receipt["status"] == "succeeded" {
                        accepted_node = receipt["node_id"].as_u64();
                    }
                }
                _ => {}
            }
        }
        socket
            .send(Message::Text(
                json!({"type":"response.done"}).to_string().into(),
            ))
            .expect("本轮回复应结束");
        socket.send(Message::Text(json!({"type":"response.function_call_arguments.done","items":[{"call_id":"post-done","name":"create_terminal","arguments":"{}"}]}).to_string().into()))
            .expect("应发送本轮结束后的迟到调用");
        let post_done_rejected = loop {
            let event: Value = serde_json::from_str(
                socket
                    .read()
                    .expect("应收到迟到调用回执")
                    .to_text()
                    .expect("回执应为文本"),
            )
            .expect("回执应为 JSON");
            if event["type"] != "conversation.item.create" {
                continue;
            }
            assert_eq!(
                event["items"][0]["call_id"], "post-done",
                "迟到回执应关联原调用"
            );
            let receipt: Value = serde_json::from_str(
                event["items"][0]["content"][0]["text"]
                    .as_str()
                    .expect("应有回执文本"),
            )
            .expect("回执应为 JSON");
            break receipt["status"] == "rejected";
        };
        socket
            .send(Message::Text(
                json!({"type":"response.output_audio.started","response_id":"old-reply"})
                    .to_string()
                    .into(),
            ))
            .expect("应开始旧回复");
        socket
            .send(Message::Text(
                json!({"type":"input_audio_buffer.speech_started"})
                    .to_string()
                    .into(),
            ))
            .expect("应发起下一轮语音");
        loop {
            let event: Value = serde_json::from_str(
                socket
                    .read()
                    .expect("插话应取消上游回复")
                    .to_text()
                    .expect("取消事件应为文本"),
            )
            .expect("取消事件应为 JSON");
            if event["type"] == "input_audio_buffer.append" {
                continue;
            }
            assert_eq!(
                event["type"], "response.cancel",
                "插话应立即取消豆包旧回复生成"
            );
            break;
        }
        socket.send(Message::Text(json!({"type":"response.function_call_arguments.done","items":[{"call_id":"stale-transcript","name":"create_terminal","arguments":"{}"}]}).to_string().into()))
            .expect("应发送缺少新转写的写操作");
        let stale_rejected = loop {
            let event: Value = serde_json::from_str(
                socket
                    .read()
                    .expect("应收到新一轮拒绝回执")
                    .to_text()
                    .expect("回执应为文本"),
            )
            .expect("回执应为 JSON");
            if event["type"] != "conversation.item.create" {
                continue;
            }
            assert_eq!(
                event["items"][0]["call_id"], "stale-transcript",
                "拒绝回执应关联新调用"
            );
            let receipt: Value = serde_json::from_str(
                event["items"][0]["content"][0]["text"]
                    .as_str()
                    .expect("应有结果文本"),
            )
            .expect("工具回执应为 JSON");
            break receipt["status"] == "rejected";
        };
        report_tx
            .send((
                audio_seen,
                result_count,
                accepted_node,
                stale_rejected,
                post_done_rejected,
            ))
            .expect("应报告验收结果");
        socket
            .send(Message::Text(
                json!({"type":"session.closed"}).to_string().into(),
            ))
            .expect("应关闭假会话");
    });
    std::env::set_var("NORMA_DOUBAO_WS_URL", format!("ws://{address}"));
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let parent = tempfile::tempdir().expect("应建立隔离项目根目录");
    let project = parent.path().join("doubao-project");
    std::fs::create_dir(&project).expect("应建立项目目录");
    enter_canvas(&app, cx, &project);
    let before = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(before.desktop_mode, "legacy", "升级默认应仍是旧语音模式");
    click(cx, "toolbar-settings");
    click(cx, "settings-section-voice");
    click(cx, "settings-voice-desktop-mode-doubao");
    assert!(
        cx.debug_bounds("settings-voice-field-doubao-api-key")
            .is_some(),
        "豆包 Keychain 输入框应可见"
    );
    assert!(
        cx.debug_bounds("settings-voice-doubao-microphone-status")
            .is_some(),
        "豆包应展示麦克风状态"
    );
    assert!(
        cx.debug_bounds("settings-voice-doubao-readiness").is_some(),
        "豆包应展示当前配置就绪状态"
    );
    assert!(
        cx.debug_bounds("settings-voice-doubao-system-input")
            .is_some(),
        "豆包应明确使用 macOS 默认输入设备"
    );
    assert!(
        cx.debug_bounds("settings-voice-input-device-picker")
            .is_none(),
        "豆包不应展示未生效的设备选择器"
    );
    assert!(
        cx.debug_bounds("settings-voice-legacy-recognition-status")
            .is_none(),
        "豆包不能误显示旧语音识别就绪"
    );
    assert!(
        cx.debug_bounds("settings-doubao-api-key-clear").is_none(),
        "未配置凭据时不能显示 Remove 操作"
    );
    let unconfigured = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_ne!(
        unconfigured.entry_state, "ready",
        "未配置豆包凭据时不能误报就绪"
    );
    app.update(cx, |app, _| {
        app.configure_doubao_for_e2e("synthetic-doubao-key");
        app.install_doubao_pcm_for_e2e(vec![PcmAudioChunk::new(
            PcmFormat::Pcm16,
            16_000,
            1,
            vec![0; 640],
        )
        .expect("20 毫秒 PCM 应有效")]);
    });
    let selected = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(selected.desktop_mode, "doubao", "设置点击应切到豆包");
    assert_eq!(
        selected.entry_state, "ready",
        "豆包就绪不应被未配置的旧 ASR/TTS 阻断"
    );
    assert_eq!(selected.dialogue_phase, "idle", "切换时旧对话状态应为空闲");
    click(cx, "toolbar-settings");
    cx.simulate_keystrokes("cmd-e");
    let deadline = Instant::now() + Duration::from_secs(8);
    let report = loop {
        cx.run_until_parked();
        if let Ok(report) = report_rx.try_recv() {
            break report;
        }
        assert!(
            Instant::now() < deadline,
            "豆包假上游应在 8 秒内收到真实工具回执"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(report.0, "假上游应收到麦克风 PCM");
    assert_eq!(report.1, 2, "重复调用应有可关联的回执");
    assert!(
        report.2.is_some(),
        "首个工具调用应返回真实 Terminal node_id"
    );
    assert!(report.3, "新一轮尚无最终转写时必须拒绝写操作");
    assert!(
        report.4,
        "回复结束后新 call_id 不能重用上一轮转写执行写操作"
    );
    let terminals = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
            .count()
    });
    assert_eq!(terminals, 1, "重复 call_id 不得创建第二张 Terminal 卡片");
    assert_ne!(terminals, 2, "工具去重不能只复用回执而重复执行副作用");
    let after = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(
        after.dialogue_phase, "idle",
        "豆包工具执行不应启动第二个 Norma 模型回合"
    );
    std::env::remove_var("NORMA_DOUBAO_WS_URL");
    server.join().expect("假豆包服务不应失败");
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
