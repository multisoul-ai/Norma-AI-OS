use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// 豆包关闭后立即重开：第二次快捷键必须等旧 ASR 会话完成关闭，再建立新连接。
///
/// 数据构造（含关键数值的推导过程）：首次关闭确认延迟 200 ms；
/// 两次会话各注入 5 × 640 B = 3,200 B，即 100 ms 的 16 kHz 单声道 PCM16。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ⌘E 建立第一条假 ASR 连接并确认已连接。
///   2. 连续按两次 ⌘E：关闭第一条会话并立即请求第二条。
///   3. 假 ASR 在收到旧连接关闭后延迟确认；若新连接抢跑，则返回并发拒绝。
///
/// 预期结果：旧会话收到结束音频帧；第二条连接在关闭确认后建立，状态为已连接；
/// 新连接不得因旧会话仍活跃而被拒绝。
#[test]
fn desktop_doubao_shortcut_restart_waits_for_asr_shutdown() {
    let _lock = DOUBAO_TEST_LOCK
        .lock()
        .expect("豆包 e2e 不应并行修改连接地址");
    let listener = TcpListener::bind("127.0.0.1:0").expect("假 ASR 应绑定本机端口");
    let address = listener.local_addr().expect("假 ASR 应有地址");
    let active = Arc::new(AtomicBool::new(true));
    let server_active = Arc::clone(&active);
    let (first_closed_tx, first_closed_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (first, _) = listener.accept().expect("首次快捷键应连接 ASR");
        first
            .set_read_timeout(Some(Duration::from_secs(8)))
            .expect("首条连接应有限等待");
        let mut first = tungstenite::accept(first).expect("首次 ASR 握手应成功");
        assert_eq!(
            first.read().expect("首次应发送配置").into_data()[1],
            0x11,
            "首条连接必须发送 ASR 配置"
        );
        first
            .send(Message::Binary(
                asr_response(json!({"code":0,"result":{"utterances":[]}})).into(),
            ))
            .expect("首条连接应确认配置");
        let closing_active = Arc::clone(&server_active);
        let closed = std::thread::spawn(move || {
            let mut terminal = false;
            loop {
                match first.read() {
                    Ok(Message::Binary(frame)) if frame[1] == 0x23 => terminal = true,
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            first_closed_tx
                .send(terminal)
                .expect("应报告首条连接是否发送结束帧");
            std::thread::sleep(Duration::from_millis(200));
            closing_active.store(false, Ordering::SeqCst);
            let _ = first.close(None);
        });
        let (second, _) = listener.accept().expect("第二次快捷键应重新连接 ASR");
        second
            .set_read_timeout(Some(Duration::from_secs(8)))
            .expect("第二条连接应有限等待");
        let mut second = tungstenite::accept(second).expect("第二次 ASR 握手应成功");
        assert_eq!(
            second.read().expect("第二次应发送配置").into_data()[1],
            0x11,
            "第二条连接必须发送 ASR 配置"
        );
        let overlapped = server_active.load(Ordering::SeqCst);
        let response = if overlapped {
            json!({"code":45000000})
        } else {
            json!({"code":0,"result":{"utterances":[]}})
        };
        second
            .send(Message::Binary(asr_response(response).into()))
            .expect("应回复第二条连接");
        while let Ok(message) = second.read() {
            if matches!(message, Message::Close(_)) {
                break;
            }
        }
        closed.join().expect("首条连接应正常退出");
        overlapped
    });
    std::env::set_var("NORMA_DOUBAO_ASR_URL", format!("ws://{address}"));
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应建立豆包重连测试项目");
    enter_canvas(&app, cx, workspace.path());
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
            .expect("20 ms PCM 应有效");
            5
        ]);
    });
    click(cx, "toolbar-settings");
    cx.simulate_keystrokes("cmd-e");
    wait_for_doubao_status(&app, cx, "connected", "首次启动");
    cx.simulate_keystrokes("cmd-e");
    assert_eq!(
        app.read_with(cx, |app, _| app.voice_e2e_snapshot().doubao_status),
        "idle",
        "关闭快捷键应立即显示空闲"
    );
    app.update(cx, |app, _| {
        app.install_doubao_pcm_for_e2e(vec![
            PcmAudioChunk::new(
                PcmFormat::Pcm16,
                16_000,
                1,
                vec![0; 640]
            )
            .expect("20 ms PCM 应有效");
            5
        ])
    });
    cx.simulate_keystrokes("cmd-e");
    wait_for_doubao_status(&app, cx, "connected", "关闭后重启");
    assert!(
        first_closed_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("旧连接应在重启前结束"),
        "关闭时必须发送 ASR 终止音频帧"
    );
    cx.simulate_keystrokes("cmd-e");
    std::env::remove_var("NORMA_DOUBAO_ASR_URL");
    assert!(
        !server.join().expect("假 ASR 线程应正常结束"),
        "新 ASR 连接不得早于旧会话释放资源"
    );
}

fn wait_for_doubao_status(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut VisualTestContext,
    expected: &str,
    stage: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // 状态泵每 100 ms 读取一次网络事件；TestAppContext 的时钟需显式推进。
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        let snapshot = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
        let status = snapshot.doubao_status;
        if status == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{stage}：豆包状态应达到 {expected}，实际为 {status}（{}）",
            snapshot.label
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
