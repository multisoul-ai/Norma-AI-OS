use crate::audio::{PcmAudioChunk, PcmFormat};
use crate::session::{RealtimeSessionConfig, RealtimeSessionId};
use serde_json::json;

use super::{decode_event, encode_command, RealtimeCommand, RealtimeEvent, RealtimeProtocolError};

/// P-1: start_session command 编码为单行 JSON object，且 model/tools 位于顶层。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = "rt_1"（固定测试会话）
///   model      = "gpt-realtime"（后续 sidecar 直接读取的顶层字段）
///   tools      = 2 项：enqueue_card_task + cancel_card_task
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 RealtimeCommand::StartSession(config)
///   2. 调用 encode_command 得到 JSONL line
///   3. 用 serde_json 解析 line，逐字段检查 type/session_id/model/tools
///
/// 预期结果：
///   - 正断言：输出是无换行的单个 JSON object
///   - 正断言：type/session_id/model/tools 均存在且值正确
///   - 负断言：不应出现嵌套 config 字段，避免和本任务 brief 的顶层协议不一致
#[test]
fn test_start_session_encodes_top_level_model_and_tools() {
    let command = RealtimeCommand::StartSession(RealtimeSessionConfig {
        session_id: RealtimeSessionId::new("rt_1"),
        model: "gpt-realtime".to_string(),
        tools: vec![
            "enqueue_card_task".to_string(),
            "cancel_card_task".to_string(),
        ],
    });

    let encoded = encode_command(&command).expect("start_session command should encode to JSON");
    assert!(
        !encoded.contains('\n'),
        "encode_command must not append a newline; JSONL framing is owned by the writer"
    );

    let value: serde_json::Value =
        serde_json::from_str(&encoded).expect("encoded start_session should be valid JSON");
    assert_eq!(
        value["type"], "start_session",
        "start_session command should carry type=start_session"
    );
    assert_eq!(
        value["session_id"], "rt_1",
        "start_session command should preserve the session id"
    );
    assert_eq!(
        value["model"], "gpt-realtime",
        "start_session command should put model at the top level"
    );
    assert_eq!(
        value["tools"],
        json!(["enqueue_card_task", "cancel_card_task"]),
        "start_session command should encode the exact tool list in order"
    );
    assert!(
        value.get("config").is_none(),
        "start_session command should not nest model/tools under config for this task contract"
    );
}

/// P-2: input_audio_chunk command 编码 PCM bytes 为 base64，且不自动追加换行。
///
/// 数据构造（含关键数值的推导过程）：
///   bytes = [0, 1, 2, 3]
///   base64([0,1,2,3]) = "AAECAw=="（标准 base64，每 3 bytes 编为 4 chars，尾部补 ==）
///   sample_rate = 24_000Hz，channels = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造有效 PCM16 chunk（4 bytes，4 % 2 = 0）
///   2. 构造 RealtimeCommand::InputAudioChunk
///   3. 调用 encode_command 后解析 JSON 并检查音频字段
///
/// 预期结果：
///   - 正断言：data_b64 等于 "AAECAw=="，format/sample_rate/channels/session_id 正确
///   - 负断言：输出不包含换行，且不会泄漏原始 bytes 数组字段
#[test]
fn test_input_audio_chunk_encodes_base64_without_newline() {
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0, 1, 2, 3])
        .expect("test chunk uses even PCM16 byte length and valid metadata");
    let command = RealtimeCommand::InputAudioChunk {
        session_id: RealtimeSessionId::new("rt_1"),
        chunk,
    };

    let encoded = encode_command(&command).expect("input_audio_chunk command should encode");
    assert!(
        !encoded.contains('\n'),
        "encode_command must return one JSON object without a trailing newline"
    );

    let value: serde_json::Value =
        serde_json::from_str(&encoded).expect("encoded input_audio_chunk should be valid JSON");
    assert_eq!(
        value["type"], "input_audio_chunk",
        "input_audio_chunk command should carry type=input_audio_chunk"
    );
    assert_eq!(
        value["session_id"], "rt_1",
        "input_audio_chunk should include the target session id"
    );
    assert_eq!(
        value["format"], "pcm16",
        "input_audio_chunk should preserve the PCM format"
    );
    assert_eq!(
        value["sample_rate"], 24_000,
        "input_audio_chunk should preserve the sample rate in Hz"
    );
    assert_eq!(
        value["channels"], 1,
        "input_audio_chunk should preserve the channel count"
    );
    assert_eq!(
        value["data_b64"], "AAECAw==",
        "input_audio_chunk should encode raw bytes using standard base64"
    );
    assert!(
        value.get("bytes").is_none(),
        "wire JSON should not expose raw bytes because JSONL carries audio as data_b64"
    );
}

/// P-3: session_started event 解码为 RealtimeEvent::SessionStarted。
///
/// 数据构造（含关键数值的推导过程）：
///   line = {"type":"session_started","session_id":"rt_1"}
///   field count = 2（type + session_id，是 session_started 的最小有效载荷）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 decode_event 解析 JSON line
///   2. 比较返回的 domain event
///
/// 预期结果：
///   - 正断言：返回 SessionStarted 且 session_id 为 rt_1
///   - 负断言：不应被归类为 IgnoredUnknown
#[test]
fn test_session_started_decodes_to_domain_event() {
    let event = decode_event(r#"{"type":"session_started","session_id":"rt_1"}"#)
        .expect("session_started JSON should decode");

    assert_eq!(
        event,
        RealtimeEvent::SessionStarted {
            session_id: RealtimeSessionId::new("rt_1"),
        },
        "session_started should decode to the matching domain event"
    );
    assert_ne!(
        event,
        RealtimeEvent::IgnoredUnknown {
            event_type: "session_started".to_string(),
        },
        "known session_started events must not be ignored as unknown"
    );
}

/// P-4: assistant_audio_chunk event 解码为包含 PcmAudioChunk 的 domain event。
///
/// 数据构造（含关键数值的推导过程）：
///   data_b64 = "AAECAw==" → bytes [0, 1, 2, 3]
///   PCM16 byte length = 4，4 % 2 = 0，因此音频块有效
///   sample_rate = 24_000Hz，channels = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. decode_event 解析 assistant_audio_chunk JSON
///   2. base64 解码 data_b64
///   3. 用解码后的 bytes 构造 PcmAudioChunk
///
/// 预期结果：
///   - 正断言：返回 AssistantAudioChunk，session_id 和 chunk 字段完整
///   - 负断言：不应被误解码为 SessionStarted
#[test]
fn test_assistant_audio_chunk_decodes_pcm_audio_chunk() {
    let event = decode_event(
        r#"{"type":"assistant_audio_chunk","session_id":"rt_1","format":"pcm16","sample_rate":24000,"channels":1,"data_b64":"AAECAw=="}"#,
    )
    .expect("assistant_audio_chunk JSON should decode");

    assert_eq!(
        event,
        RealtimeEvent::AssistantAudioChunk {
            session_id: RealtimeSessionId::new("rt_1"),
            chunk: PcmAudioChunk {
                format: PcmFormat::Pcm16,
                sample_rate_hz: 24_000,
                channels: 1,
                bytes: vec![0, 1, 2, 3],
            },
        },
        "assistant_audio_chunk should decode into a validated PcmAudioChunk"
    );
    assert_ne!(
        event,
        RealtimeEvent::SessionStarted {
            session_id: RealtimeSessionId::new("rt_1"),
        },
        "assistant audio payload must not be misclassified as a session_started event"
    );
}

/// P-5: unknown event type 被保留为 IgnoredUnknown，而不是 panic 或报错。
///
/// 数据构造（含关键数值的推导过程）：
///   type = "future_event"（当前协议未知，但未来可能加入）
///   session_id = "rt_1"（未知事件仍可带普通字段）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. decode_event 先解析 JSON object
///   2. 读取 type 字段，发现不是已知类型
///   3. 返回 IgnoredUnknown { event_type }
///
/// 预期结果：
///   - 正断言：返回 IgnoredUnknown 且 event_type 原样保留
///   - 负断言：未知事件不应伪装成 SessionStarted
#[test]
fn test_unknown_event_type_decodes_to_ignored_unknown() {
    let event = decode_event(r#"{"type":"future_event","session_id":"rt_1","payload":1}"#)
        .expect("unknown event type should decode to IgnoredUnknown instead of failing");

    assert_eq!(
        event,
        RealtimeEvent::IgnoredUnknown {
            event_type: "future_event".to_string(),
        },
        "unknown event type should be preserved for logging and ignored by the caller"
    );
    assert_ne!(
        event,
        RealtimeEvent::SessionStarted {
            session_id: RealtimeSessionId::new("rt_1"),
        },
        "unknown event type must not be coerced into a known session event"
    );
}

/// P-7: assistant_audio_state event 解码为 assistant speaking 状态。
///
/// 数据构造（含关键数值的推导过程）：
///   line = {"type":"assistant_audio_state","session_id":"rt_1","speaking":true}
///   field count = 3（type + session_id + speaking，是 speaking state 的最小有效载荷）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 decode_event 解析 assistant_audio_state JSON
///   2. 协议层读取 speaking=true
///   3. 返回 RealtimeEvent::AssistantAudioState
///
/// 预期结果：
///   - 正断言：返回 AssistantAudioState 且 speaking 为 true
///   - 负断言：不应被归类为 IgnoredUnknown，否则 app 无法切换 Replying... 状态
#[test]
fn test_assistant_audio_state_decodes_speaking_state() {
    let event =
        decode_event(r#"{"type":"assistant_audio_state","session_id":"rt_1","speaking":true}"#)
            .expect("assistant_audio_state JSON should decode");

    assert_eq!(
        event,
        RealtimeEvent::AssistantAudioState {
            session_id: RealtimeSessionId::new("rt_1"),
            speaking: true,
        },
        "assistant_audio_state should decode into the assistant speaking domain event"
    );
    assert_ne!(
        event,
        RealtimeEvent::IgnoredUnknown {
            event_type: "assistant_audio_state".to_string(),
        },
        "assistant_audio_state must not be ignored because app uses it for Replying..."
    );
}

/// P-6: malformed JSON line 返回 MalformedJson 错误。
///
/// 数据构造（含关键数值的推导过程）：
///   line = "{not-json"（缺少 quoted key、冒号和值，不是合法 JSON object）
///   byte_len = 9 bytes，长度非空但语法无效
///
/// 执行过程（逐步说明系统如何处理）：
///   1. decode_event 调用 serde_json 解析 line
///   2. JSON parser 返回语法错误
///   3. 协议层映射为 RealtimeProtocolError::MalformedJson
///
/// 预期结果：
///   - 正断言：错误类型是 MalformedJson
///   - 负断言：malformed JSON 不应被当成 IgnoredUnknown
#[test]
fn test_malformed_json_returns_protocol_error() {
    let err = decode_event("{not-json").expect_err("malformed JSON should return an error");

    assert_eq!(
        err,
        RealtimeProtocolError::MalformedJson,
        "malformed JSON should map to the stable MalformedJson protocol error"
    );
    assert_ne!(
        err,
        RealtimeProtocolError::InvalidAudioData,
        "malformed JSON should fail before any audio decoding is attempted"
    );
}
