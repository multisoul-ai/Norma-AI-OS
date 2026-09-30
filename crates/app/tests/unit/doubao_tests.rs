use super::playback::start_playback_with;
use super::*;
use audio::{PcmAudioChunk, PcmFormat};
use flate2::read::GzDecoder;
use serde_json::json;
use std::io::Read;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

fn playback_events() -> crate::app::voice_window_inset::VoiceWindowInsetEventSink {
    let (events, _wake) = crate::app::voice_window_inset::VoiceWindowInsetEventBus::new();
    events.sink(crate::app::voice_window_inset::TtsPlaybackSource::Doubao)
}

/// DB-9：方舟 ASR 2.0 使用二进制 gzip 帧，首帧必须声明 16 kHz 单声道 PCM。
///
/// 数据构造（含关键数值的推导过程）：100 毫秒 PCM = 16,000 × 2 × 0.1 = 3,200 字节；
/// 首帧序号 1，首个音频帧序号 2；结束帧使用 -3（下一个序号取负）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 编码配置帧 → 解析 gzip JSON。
///   2. 编码 3,200 字节音频 → 核对普通帧类型和正序号。
///   3. 编码结束音频帧 → 核对 last flag 和负序号。
///
/// 预期结果：配置准确声明采样率；普通帧保留长度且无 last flag；
/// 结束帧有 last flag 和负序号，且不是旧 3.0 JSON 音频事件。
#[test]
fn test_doubao_plan_asr_uses_binary_pcm_frames() {
    let start = plan::asr::encode_start().expect("ASR 配置应可编码");
    let frame = plan::asr::encode_audio(2, &vec![0; 3_200]).expect("100 毫秒 PCM 应可编码");
    let last = plan::asr::encode_audio(-3, &vec![0; 3_200]).expect("结束音频帧应可编码");
    assert_eq!(start[0], 0x11, "ASR 配置应使用 V3 二进制头");
    assert_eq!(frame[1], 0x21, "音频应使用二进制音频请求类型");
    assert_eq!(
        i32::from_be_bytes(frame[4..8].try_into().expect("普通序号应为 4 字节")),
        2,
        "普通音频帧必须保持正序号 2"
    );
    assert_eq!(last[1], 0x23, "最后一帧必须设置 last flag");
    assert_eq!(
        i32::from_be_bytes(last[4..8].try_into().expect("结束序号应为 4 字节")),
        -3,
        "结束帧必须使用下一个序号的负值"
    );
    let mut config = String::new();
    GzDecoder::new(&start[12..])
        .read_to_string(&mut config)
        .expect("配置 gzip 应可解码");
    let config: serde_json::Value = serde_json::from_str(&config).expect("配置应为 JSON");
    assert_eq!(config["audio"]["rate"], 16_000, "ASR 必须接收 16 kHz PCM");
    assert_eq!(config["audio"]["channel"], 1, "ASR 必须接收单声道");
    let mut pcm = Vec::new();
    GzDecoder::new(&frame[12..])
        .read_to_end(&mut pcm)
        .expect("音频 gzip 应可解码");
    assert_eq!(pcm.len(), 3_200, "上传帧应覆盖恰好 100 毫秒音频");
    assert_ne!(frame[0], b'{', "不得继续发送 Seeduplex JSON 音频事件");
}

/// ASR 错误帧：协议头的错误码必须进入诊断，不能被 JSON 缺失的 code 覆盖为 0。
///
/// 数据构造（含关键数值的推导过程）：4 B 基础头 + 4 B 错误码 + 4 B 长度 + JSON；
/// 错误码 45,000,000 来自测试构造，JSON 只有 message 而没有 code。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 0xF 服务端错误帧，在头部写入 45,000,000。
///   2. 解码错误帧并提取供界面及日志使用的原因。
///
/// 预期结果：原因包含真实协议错误码 45,000,000；不得报告虚假的错误码 0。
#[test]
fn test_doubao_asr_error_frame_uses_header_code() {
    let payload = br#"{"message":"synthetic error"}"#;
    let mut frame = vec![0x11, 0xf0, 0x10, 0];
    frame.extend_from_slice(&45_000_000_u32.to_be_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    let error = plan::asr::decode(&frame).expect_err("ASR 错误帧不得被当作成功结果");
    assert!(
        error.contains("45000000"),
        "诊断必须包含协议头中的真实错误码：{error}"
    );
    assert!(
        !error.contains("错误码 0"),
        "缺失 JSON code 时不得伪造错误码 0：{error}"
    );
}

struct BlockingSink {
    written: Arc<AtomicUsize>,
    first_started: mpsc::Sender<()>,
    release_first: mpsc::Receiver<()>,
    stopped: mpsc::Sender<()>,
}

impl super::super::voice::playback::AssistantAudioSink for BlockingSink {
    fn write_chunk(&mut self, _chunk: &PcmAudioChunk) -> Result<Duration, String> {
        if self.written.fetch_add(1, Ordering::SeqCst) == 0 {
            let _ = self.first_started.send(());
            let _ = self.release_first.recv_timeout(Duration::from_secs(2));
        }
        Ok(Duration::from_millis(20))
    }

    fn stop(&mut self) {
        let _ = self.stopped.send(());
    }
}

/// DB-2：插话优先停止播放并丢弃已排队的旧音频。
///
/// 数据构造（含关键数值的推导过程）：每段 960 字节 = 24 kHz × 2 字节 × 20 ms；
/// 第一段写入被阻塞，第二段已排队，插话使 epoch 从 0 变为 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一段开始写入，第二段进入音频队列。
///   2. 插话经独立控制队列发出 Stop，随后释放第一段。
///   3. 播放线程先处理 Stop，再发现第二段属于旧 epoch。
///
/// 预期结果：确实收到 Stop；只有第一段被提交播放，第二段不能续播；
///   白色边框收到中断事件且不得收到正常结束事件。
#[test]
fn test_doubao_interruption_discards_queued_audio() {
    use crate::app::voice_window_inset::{
        TtsPlaybackEvent, TtsPlaybackSource, VoiceWindowInsetEventBus,
    };

    let (events, _wake) = VoiceWindowInsetEventBus::new();
    let (first_tx, first_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (stopped_tx, stopped_rx) = mpsc::channel();
    let written = Arc::new(AtomicUsize::new(0));
    let recorded = Arc::clone(&written);
    let playback = start_playback_with(
        move || {
            Box::new(BlockingSink {
                written: recorded,
                first_started: first_tx,
                release_first: release_rx,
                stopped: stopped_tx,
            })
        },
        events.sink(TtsPlaybackSource::Doubao),
    );
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960])
        .expect("20 ms output PCM should be valid");
    playback
        .enqueue_for(playback.generation(), chunk.clone())
        .expect("第一段应进入播放队列");
    first_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("第一段应开始播放");
    playback
        .enqueue_for(playback.generation(), chunk)
        .expect("第二段应进入播放队列");
    playback.interrupt();
    release_tx.send(()).expect("应释放第一段的模拟写入");
    stopped_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("插话后应停止播放器");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut visual_events = Vec::new();
    while !visual_events
        .iter()
        .any(|(_, event)| matches!(event, TtsPlaybackEvent::Interrupted))
        && std::time::Instant::now() < deadline
    {
        visual_events.extend(events.drain());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        visual_events
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Interrupted)),
        "插话必须立即让白色边框进入中断淡出"
    );
    assert!(
        !visual_events
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Finished)),
        "插话不得把未播完的旧语音标为正常完成"
    );
    playback.end();
    stopped_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("结束会话也应停止播放器");
    assert_eq!(
        written.load(Ordering::SeqCst),
        1,
        "插话后旧队列的第二段不得播放"
    );
    assert_ne!(
        written.load(Ordering::SeqCst),
        2,
        "播放线程不能先消费旧队列再处理停止命令"
    );
}

/// DB-10：TTS 网络响应在插话后迟到，也不能进入新一代播放队列。
///
/// 数据构造（含关键数值的推导过程）：每段 960 字节 = 24 kHz × 2 字节 × 20 ms；
/// 第一段属于 epoch 0，插话后队列进入 epoch 1，迟到段仍携带 epoch 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一段进入播放 sink 并阻塞。
///   2. 插话使 epoch 变化；旧 TTS 随后提交第二段。
///   3. 释放 sink，核对实际提交数量。
///
/// 预期结果：第一段确实开始，迟到段被拒绝；不能在新话轮继续播旧音频。
#[test]
fn test_doubao_late_tts_chunk_is_rejected_after_interruption() {
    let (first_tx, first_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (stopped_tx, stopped_rx) = mpsc::channel();
    let written = Arc::new(AtomicUsize::new(0));
    let recorded = Arc::clone(&written);
    let playback = start_playback_with(
        move || {
            Box::new(BlockingSink {
                written: recorded,
                first_started: first_tx,
                release_first: release_rx,
                stopped: stopped_tx,
            })
        },
        playback_events(),
    );
    let pcm =
        PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).expect("20 ms PCM 应有效");
    let old_generation = playback.generation();
    playback
        .enqueue_for(old_generation, pcm.clone())
        .expect("第一段应进入队列");
    first_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("第一段应开始播放");
    playback.interrupt();
    playback
        .enqueue_for(old_generation, pcm)
        .expect("迟到音频应被安全忽略");
    release_tx.send(()).expect("应释放第一段");
    stopped_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("插话应停止播放器");
    playback.end();
    assert_eq!(
        written.load(Ordering::SeqCst),
        1,
        "只能提交插话前已开始的一段"
    );
    assert_ne!(written.load(Ordering::SeqCst), 2, "迟到旧 TTS 音频不得续播");
}

/// DB-7：播放端落后超过 16 段缓存时明确报错，不静默丢字。
///
/// 数据构造（含关键数值的推导过程）：每段 960 字节 = 24 kHz × 2 字节 × 20 ms；
/// 第一段被播放线程取走并阻塞，队列容量 16 段 = 320 ms，第 17 个待播段超出容量。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一段进入 sink 并阻塞，连续放入 16 段。
///   2. 再放入一段 → 返回可见错误，而非返回成功后丢弃内容。
///   3. 释放 sink 并结束队列。
///
/// 预期结果：容量内写入成功，超出容量失败；不能把丢帧伪装成成功。
#[test]
fn test_doubao_playback_overflow_is_reported() {
    let (first_tx, first_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (stopped_tx, _stopped_rx) = mpsc::channel();
    let playback = start_playback_with(
        move || {
            Box::new(BlockingSink {
                written: Arc::new(AtomicUsize::new(0)),
                first_started: first_tx,
                release_first: release_rx,
                stopped: stopped_tx,
            })
        },
        playback_events(),
    );
    let chunk =
        PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).expect("20 ms PCM 应有效");
    playback
        .enqueue_for(playback.generation(), chunk.clone())
        .expect("首段应进入 sink");
    first_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("首段应被播放线程取走");
    for _ in 0..16 {
        playback
            .enqueue_for(playback.generation(), chunk.clone())
            .expect("容量内的 16 段均应入队");
    }
    assert!(
        playback.enqueue_for(playback.generation(), chunk).is_err(),
        "第 17 个待播段必须明确报告溢出"
    );
    release_tx.send(()).expect("应释放第一段");
    playback.end();
}

/// DB-8：输出设备不可用时停止会话并显示音频错误，不保持假连接状态。
///
/// 数据构造（含关键数值的推导过程）：一段 960 字节 = 24 kHz × 2 字节 × 20 ms；
/// Noop sink 代表 macOS 播放后端无法创建，第一段写入即失败。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建会拒绝 PCM 的 sink，提交一段有效音频。
///   2. 播放线程收到后端错误并转交会话错误通道。
///
/// 预期结果：明确得到输出失败；不能静默丢帧却保持“已连接”。
#[test]
fn test_doubao_playback_backend_error_is_reported() {
    let playback = start_playback_with(
        || Box::<crate::app::voice::playback::NoopAssistantAudioSink>::default(),
        playback_events(),
    );
    let chunk =
        PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).expect("20 ms PCM 应有效");
    playback
        .enqueue_for(playback.generation(), chunk)
        .expect("有效音频应进入队列");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let error = loop {
        if let Ok(error) = playback.errors.try_recv() {
            break error;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "播放错误应及时回传会话"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(error.contains("unavailable"), "后端不可用必须有明确错误");
    assert!(!error.is_empty(), "播放错误不能被空消息吞掉");
    playback.end();
}

/// DB-3：ASR 2.0 握手拒绝方舟 Key 时，界面给出资源权限指引而不泄露服务响应。
///
/// 数据构造（含关键数值的推导过程）：401 表示鉴权拒绝，403 表示权限拒绝；
/// 两种状态均来自零音频上传的 WebSocket 握手，响应体故意含敏感测试文本。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为 401 和 403 构造 HTTP 握手错误。
///   2. 通过连接错误映射生成用户可见文案。
///
/// 预期结果：401 指向方舟 Key、403 指向 ASR 2.0 资源授权；响应体不出现在文案中。
#[test]
fn test_doubao_handshake_error_identifies_voice_key() {
    use tokio_tungstenite::tungstenite::Error;
    let unauthorized = Error::Http(Box::new(
        tokio_tungstenite::tungstenite::http::Response::builder()
            .status(401)
            .body(Some(b"sensitive-provider-body".to_vec()))
            .expect("401 response should be constructible"),
    ));
    let forbidden = Error::Http(Box::new(
        tokio_tungstenite::tungstenite::http::Response::builder()
            .status(403)
            .body(None)
            .expect("403 response should be constructible"),
    ));
    let key_message = connect_failure_message(&unauthorized);
    let access_message = connect_failure_message(&forbidden);
    assert!(
        key_message.contains("方舟 API Key"),
        "401 应指向方舟 Key 的 ASR 2.0 权限"
    );
    assert!(
        !key_message.contains("sensitive-provider-body"),
        "不得向界面回显上游响应体"
    );
    assert!(
        access_message.contains("ASR 2.0"),
        "403 应说明 ASR 2.0 资源权限问题"
    );
    assert!(
        !access_message.contains("API Key"),
        "403 不应误报为密钥格式问题"
    );
}

/// DB-5：任务接收后只在协调器出现真实终态时生成豆包播报。
///
/// 数据构造（含关键数值的推导过程）：单个 Codex 任务 task_id=17；
/// queued/running 为 2 个非终态，succeeded/failed 为 2 个终态。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对同一任务依次代入排队、运行、完成与失败状态。
///   2. 读取拟发送给豆包的语音文本。
///
/// 预期结果：接收和运行时不能说已完成；完成或失败必须含真实 task_id 和结果。
#[test]
fn test_doubao_task_report_requires_terminal_state() {
    let mut task = agent::AgentCoordinatedTask {
        task_id: 17,
        task_ref: "runtime:17".into(),
        root_task_ref: "runtime:17".into(),
        parent_task_ref: None,
        source_definition_id: None,
        source_node_id: None,
        definition_id: 1,
        target_node_id: 9,
        agent: "Codex".into(),
        task: "检查项目".into(),
        state: "queued",
        tracking: agent::AgentTrackingMode::Managed,
        result: None,
    };
    assert!(
        task_completion_message(&task).is_none(),
        "排队回执不得冒充完成播报"
    );
    task.state = "running";
    assert!(
        task_completion_message(&task).is_none(),
        "运行状态不得冒充完成播报"
    );
    task.state = "succeeded";
    assert_eq!(
        task_completion_message(&task).as_deref(),
        Some("Codex 的任务 #17 已完成。"),
        "真实完成才允许播报完成"
    );
    task.state = "failed";
    assert_eq!(
        task_completion_message(&task).as_deref(),
        Some("Codex 的任务 #17 失败。"),
        "真实失败必须播报失败而非成功"
    );
}

/// DB-1：方舟 Chat 2.0 使用现有 Norma 人设和工具声明。
///
/// 数据构造（含关键数值的推导过程）：一个项目上下文、一个用户消息；
/// 工具列表来自现有 Norma 工具定义，不能是空列表。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从现有提示词组装入口生成当前项目人设。
///   2. 构造方舟 Chat 请求，并检查模型、上下文与工具。
///
/// 预期结果：人设及项目进入 system 消息；工具按方舟函数格式声明；
/// 请求中不包含已废弃的 Seeduplex 3.0 session.create 事件。
#[test]
fn test_doubao_plan_chat_reuses_norma_prompt_and_tools() {
    let context = agent::CurrentCanvasContext {
        project_name: "spec-workspace".into(),
        workspace_root: "/tmp/spec-workspace".into(),
        total_cards: 0,
        cards: Vec::new(),
        conversation_tails: Vec::new(),
    };
    let prompt = agent::norma_system_prompt(&context, &[]);
    let body = plan::chat::request_body(&prompt, &[json!({"role":"user","content":"你好"})], true);
    assert_eq!(body["model"], "doubao-seed-2.0-mini", "应调用方舟 2.0 模型");
    assert!(
        body["messages"][0]["content"]
            .as_str()
            .is_some_and(
                |text| text.contains("Norma Master Agent") && text.contains("spec-workspace")
            ),
        "应复用完整人设与项目上下文"
    );
    assert!(
        body["tools"].as_array().is_some_and(|items| items
            .iter()
            .any(|item| item["function"]["name"] == "assign_agent_task")),
        "应复用现有派工工具"
    );
    assert!(
        body.get("session").is_none(),
        "方舟 Chat 不应发送 Seeduplex 会话配置"
    );
}
