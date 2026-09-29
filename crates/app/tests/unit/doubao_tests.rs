use super::playback::start_playback_with;
use super::*;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

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
/// 预期结果：确实收到 Stop；只有第一段被提交播放，第二段不能续播。
#[test]
fn test_doubao_interruption_discards_queued_audio() {
    let (first_tx, first_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (stopped_tx, stopped_rx) = mpsc::channel();
    let written = Arc::new(AtomicUsize::new(0));
    let recorded = Arc::clone(&written);
    let playback = start_playback_with(move || {
        Box::new(BlockingSink {
            written: recorded,
            first_started: first_tx,
            release_first: release_rx,
            stopped: stopped_tx,
        })
    });
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960])
        .expect("20 ms output PCM should be valid");
    playback
        .enqueue(chunk.clone())
        .expect("第一段应进入播放队列");
    first_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("第一段应开始播放");
    playback.enqueue(chunk).expect("第二段应进入播放队列");
    playback.interrupt();
    release_tx.send(()).expect("应释放第一段的模拟写入");
    stopped_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("插话后应停止播放器");
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
    let playback = start_playback_with(move || {
        Box::new(BlockingSink {
            written: Arc::new(AtomicUsize::new(0)),
            first_started: first_tx,
            release_first: release_rx,
            stopped: stopped_tx,
        })
    });
    let chunk =
        PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).expect("20 ms PCM 应有效");
    playback.enqueue(chunk.clone()).expect("首段应进入 sink");
    first_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("首段应被播放线程取走");
    for _ in 0..16 {
        playback
            .enqueue(chunk.clone())
            .expect("容量内的 16 段均应入队");
    }
    assert!(
        playback.enqueue(chunk).is_err(),
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
    let playback = start_playback_with(|| {
        Box::<crate::app::voice::playback::NoopAssistantAudioSink>::default()
    });
    let chunk =
        PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 960]).expect("20 ms PCM 应有效");
    playback.enqueue(chunk).expect("有效音频应进入队列");
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

/// DB-6：插话后旧回复迟到的音频不能在新一轮转写结束后恢复播放。
///
/// 数据构造（含关键数值的推导过程）：两段回复分别用 old/new 身份；
/// 每段代表 20 毫秒、960 字节的 24 kHz 单声道 PCM，此处只验收播放准入。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. old 开始并收到音频 → 准许播放。
///   2. 用户插话 → old 被阻断，转写结束后迟到的 old 仍被阻断。
///   3. new 开始 → new 音频准许播放，old 始终被阻断。
///
/// 预期结果：旧音频不能续播；新回复可播，通话无需重建。
#[test]
fn test_doubao_interruption_blocks_late_old_response() {
    let mut gate = AudioTurnGate::default();
    assert!(gate.started(Some("old"), false), "旧回复初次开始应允许播放");
    assert!(
        gate.accept_delta(Some("old"), false),
        "旧回复插话前音频应可播放"
    );
    gate.interrupt();
    assert!(
        !gate.accept_delta(Some("old"), true),
        "说话时旧音频不得播放"
    );
    assert!(
        !gate.accept_delta(Some("old"), false),
        "转写结束后旧音频仍不得续播"
    );
    assert!(
        !gate.started(Some("old"), false),
        "旧回复迟到的开始事件不能解除阻断"
    );
    assert!(gate.started(Some("new"), false), "新回复开始应解除等待状态");
    assert!(gate.accept_delta(Some("new"), false), "新回复音频应能播放");
    assert!(
        !gate.accept_delta(Some("old"), false),
        "新回复开始后旧音频仍不能播放"
    );
}

/// DB-3：真实握手拒绝 Ark Key 时，界面给出语音服务凭据指引而不泄露服务响应。
///
/// 数据构造（含关键数值的推导过程）：401 表示鉴权拒绝，403 表示权限拒绝；
/// 两种状态均来自零音频上传的 WebSocket 握手，响应体故意含敏感测试文本。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为 401 和 403 构造 HTTP 握手错误。
///   2. 通过连接错误映射生成用户可见文案。
///
/// 预期结果：401 指向豆包语音 Key、403 指向全双工授权；响应体不出现在文案中。
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
        key_message.contains("豆包语音"),
        "401 应引导使用豆包语音服务凭据"
    );
    assert!(
        !key_message.contains("sensitive-provider-body"),
        "不得向界面回显上游响应体"
    );
    assert!(
        access_message.contains("全双工"),
        "403 应说明全双工权限问题"
    );
    assert!(
        !access_message.contains("API Key"),
        "403 不应误报为密钥格式问题"
    );
}

/// DB-4：豆包工具回执按现有 3.0 客户端的 items/role/content 协议编码。
///
/// 数据构造（含关键数值的推导过程）：一个 call_id 对应一个结果 item；
/// 成功回执含一个 node_id，故 content 数量 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将桌面工具结果转换为豆包 item。
///   2. 检查调用关联键、角色和输入文本。
///
/// 预期结果：豆包可解析原调用的真实结果；不能退回 OpenAI function_call_output 结构。
#[test]
fn test_doubao_tool_receipt_uses_provider_item_shape() {
    let item = tools::output("call-one", json!({"status":"succeeded","node_id":7}));
    assert_eq!(item["call_id"], "call-one", "应保留原豆包调用 ID");
    assert_eq!(item["role"], "tool", "应使用豆包 tool 角色");
    assert_eq!(
        item["content"][0]["type"], "input_text",
        "应使用豆包输入文本内容"
    );
    assert!(
        item["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("\"node_id\":7")),
        "应回传真实 node_id"
    );
    assert!(
        item.get("type").is_none(),
        "不得发送 OpenAI 专用 function_call_output 类型"
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

/// DB-1：豆包握手沿用 Norma 的真实提示词和工具定义，且采用低延迟 PCM 格式。
///
/// 数据构造（含关键数值的推导过程）：16 kHz × 2 字节/采样 × 1 声道为 32 KB/s 输入；
/// 24 kHz × 2 字节/采样 × 1 声道为 48 KB/s 输出；提示词含专用项目名。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用现有 Norma 提示词组装器生成项目上下文。
///   2. 构造豆包 session.create，并读取音频格式与工具列表。
///
/// 预期结果：人设、项目和工具在同一握手中；不发送空工具或第二个模型配置。
#[test]
fn test_doubao_session_reuses_norma_prompt_and_tools() {
    let context = agent::CurrentCanvasContext {
        project_name: "spec-workspace".into(),
        workspace_root: "/tmp/spec-workspace".into(),
        total_cards: 0,
        cards: Vec::new(),
        conversation_tails: Vec::new(),
    };
    let prompt = agent::norma_system_prompt(&context, &[]);
    let event = session_create(&prompt);
    let instructions = event["session"]["instructions"]
        .as_str()
        .unwrap_or_default();
    assert!(
        instructions.contains("Norma Master Agent"),
        "握手应复用 Norma 人设"
    );
    assert!(
        instructions.contains("spec-workspace"),
        "握手应包含当前项目"
    );
    assert_eq!(
        event["session"]["audio"]["input"]["format"]["rate"], 16_000,
        "输入采样率必须匹配麦克风 PCM"
    );
    assert_eq!(
        event["session"]["audio"]["output"]["format"]["rate"], 24_000,
        "输出采样率必须匹配播放器 PCM"
    );
    assert!(
        event["session"]["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == "assign_agent_task")),
        "握手应声明现有派工工具"
    );
    assert_ne!(event["session"]["tools"], json!([]), "工具列表不可为空");
}
