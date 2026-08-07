use super::*;
use audio::{PcmAudioChunk, PcmFormat};
use std::collections::VecDeque;
use std::sync::Arc;

struct ScriptedTtsClient {
    requests: Vec<String>,
    chunks: VecDeque<PcmAudioChunk>,
    sink: Option<Arc<dyn tts::TtsChunkSink>>,
}

impl tts::TtsClient for ScriptedTtsClient {
    fn start_stream(&mut self, sink: Arc<dyn tts::TtsChunkSink>) -> Result<(), tts::TtsError> {
        self.sink = Some(sink);
        Ok(())
    }

    fn push_text(&mut self, text: &str) -> Result<(), tts::TtsError> {
        self.requests.push(text.to_string());
        let chunk = self.chunks.pop_front().ok_or_else(|| {
            tts::TtsError::new(tts::TtsErrorKind::Protocol, "missing scripted PCM chunk")
        })?;
        let accepted = self
            .sink
            .as_ref()
            .expect("scripted stream must be started before text is pushed")
            .send(chunk);
        if accepted {
            Ok(())
        } else {
            Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "scripted sink rejected PCM",
            ))
        }
    }

    fn finish_stream(&mut self) -> Result<(), tts::TtsError> {
        let sink = self
            .sink
            .take()
            .expect("scripted stream must be active before finish");
        sink.finish(Ok(tts::TtsStreamSummary::default()));
        Ok(())
    }

    fn cancel_stream(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.finish(Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "scripted stream was cancelled",
            )));
        }
    }
}

fn pcm16(samples: &[i16]) -> PcmAudioChunk {
    PcmAudioChunk::new(
        PcmFormat::Pcm16,
        16_000,
        1,
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect(),
    )
    .expect("16kHz mono PCM16 测试样本必须合法")
}

/// Agent 终态播报停顿：名称与 Done 必须分别合成，并在单条 PCM 中插入 250ms 静音。
///
/// 数据构造（含关键数值的推导过程）：
///   name PCM    = 2 samples × 2 bytes = 4 bytes
///   pause PCM   = 16,000Hz × 250ms / 1,000 × 2 bytes = 8,000 bytes
///   outcome PCM = 2 samples × 2 bytes = 4 bytes
///   total       = 4 + 8,000 + 4 = 8,008 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. TTS 先接收 `Raya` 并返回两个非零样本。
///   2. TTS 再接收 `Done` 并返回另外两个非零样本。
///   3. Agent 专用合成器在两段之间插入 250ms 零值 PCM，并返回一条合并音频。
///
/// 预期结果：
///   - 正断言：TTS 请求严格为 `Raya`、`Done` 两段，且合并长度为 8,008 bytes。
///   - 正断言：名称和结果样本分别保留在合并音频两端。
///   - 负断言：中间 8,000 bytes 不得包含非零样本，也不得把整句一次性交给 TTS。
#[test]
fn test_agent_terminal_announcement_inserts_250ms_pause_between_name_and_done() {
    let name_pcm = pcm16(&[101, 102]);
    let outcome_pcm = pcm16(&[201, 202]);
    let expected_name_bytes = name_pcm.bytes.clone();
    let expected_outcome_bytes = outcome_pcm.bytes.clone();
    let mut tts = ScriptedTtsClient {
        requests: Vec::new(),
        chunks: VecDeque::from([name_pcm, outcome_pcm]),
        sink: None,
    };

    let merged = synthesize_agent_terminal_announcement(&mut tts, "Raya", "Done")
        .expect("合法的同格式名称与结果 PCM 必须成功合并");

    assert_eq!(
        tts.requests,
        vec!["Raya".to_string(), "Done".to_string()],
        "Agent 终态播报必须分别合成名称与结果，不能一次合成 `Raya, Done`"
    );
    assert_eq!(
        merged.bytes.len(),
        8_008,
        "16kHz mono PCM16 的两段 4-byte 音频之间插入 250ms 后总长必须为 8,008 bytes"
    );
    assert_eq!(
        &merged.bytes[..4],
        expected_name_bytes.as_slice(),
        "合并音频开头必须完整保留 Raya 的名称 PCM"
    );
    assert!(
        merged.bytes[4..8_004].iter().all(|byte| *byte == 0),
        "名称与 Done 之间的 8,000 bytes 必须全部为静音零值"
    );
    assert_eq!(
        &merged.bytes[8_004..],
        expected_outcome_bytes.as_slice(),
        "合并音频结尾必须完整保留 Done 的结果 PCM"
    );
    assert!(
        !tts.requests.iter().any(|request| request.contains(',')),
        "Agent 专用路径不得继续把带逗号的完整短句交给 TTS"
    );
}

/// ACA-SYN-2：结果词已在工作区缓存时，只合成缺失的 Agent 名称，并复用缓存 PCM。
///
/// 数据构造（含关键数值的推导过程）：
///   cached outcome PCM = 2 samples × 2 bytes = 4 bytes（`Done`）
///   synthesized name   = 2 samples × 2 bytes = 4 bytes（`Raya`）
///   pause PCM          = 16,000Hz × 250ms / 1,000 × 2 bytes = 8,000 bytes
///   total output       = 4 + 8,000 + 4 = 8,008 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 把 `Done` PCM 放入缓存片段，`Raya` 保持缺失。
///   2. 调用带缓存的终态合成器，Fake TTS 仅准备 `Raya` 的一段 PCM。
///   3. 合成器只向 TTS 请求名称，并将缓存 `Done` 与静音停顿拼接进结果。
///
/// 预期结果：
///   - 正断言：TTS 只收到 `Raya`，输出仍包含正确的名称、停顿和 `Done` PCM。
///   - 负断言：TTS 不得再次收到 `Done`，避免重复后台合成。
#[test]
fn test_cached_outcome_reuses_pcm_and_synthesizes_only_missing_agent_name() {
    let name_pcm = pcm16(&[101, 102]);
    let cached_outcome_pcm = pcm16(&[201, 202]);
    let mut tts = ScriptedTtsClient {
        requests: Vec::new(),
        chunks: VecDeque::from([name_pcm.clone()]),
        sink: None,
    };
    let cached = CachedAnnouncementSegmentLookup {
        name: None,
        outcome: Some(Arc::new(cached_outcome_pcm.clone())),
    };

    let synthesized = synthesize_agent_terminal_announcement_with_cached_segments(
        &mut tts, "Raya", "Done", cached,
    )
    .expect("缺少名称但已有 Done 缓存时必须成功拼接终态播报");

    assert_eq!(
        tts.requests,
        vec!["Raya".to_string()],
        "仅名称缺失时 TTS 必须只合成 Raya，不能重复请求 Done"
    );
    assert!(
        !tts.requests.iter().any(|request| request == "Done"),
        "已缓存的 Done 不得再次进入 TTS 请求"
    );
    assert_eq!(
        synthesized.pcm.bytes.len(),
        8_008,
        "复用 4-byte Done PCM 并插入 250ms 停顿后的终态音频必须为 8,008 bytes"
    );
    assert_eq!(
        &synthesized.pcm.bytes[..4],
        name_pcm.bytes.as_slice(),
        "合并音频开头必须保留新合成的 Raya 名称 PCM"
    );
    assert_eq!(
        &synthesized.pcm.bytes[8_004..],
        cached_outcome_pcm.bytes.as_slice(),
        "合并音频结尾必须保留缓存的 Done PCM"
    );
}

/// ACA-SYN-3：名称与结果词均命中缓存时，播报直接拼接 PCM 而不需要 TTS client。
///
/// 数据构造（含关键数值的推导过程）：
///   cached name PCM    = 2 samples × 2 bytes = 4 bytes
///   cached outcome PCM = 2 samples × 2 bytes = 4 bytes
///   pause PCM          = 16,000Hz × 250ms / 1,000 × 2 bytes = 8,000 bytes
///   total output       = 4 + 8,000 + 4 = 8,008 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造完整的名称与结果词缓存片段，不创建 TTS client。
///   2. 调用缓存命中专用拼接器。
///   3. 读取最终 PCM 的首段、静音段和尾段。
///
/// 预期结果：
///   - 正断言：输出保留两个缓存片段并插入 250ms 静音。
///   - 负断言：该路径不接收 TTS client，不能触发任何后台语音合成。
#[test]
fn test_complete_cached_segments_assemble_without_tts_synthesis() {
    let name_pcm = Arc::new(pcm16(&[101, 102]));
    let outcome_pcm = Arc::new(pcm16(&[201, 202]));
    let cached = CachedAnnouncementSegmentLookup {
        name: Some(Arc::clone(&name_pcm)),
        outcome: Some(Arc::clone(&outcome_pcm)),
    };

    let synthesized = assemble_cached_agent_terminal_announcement(cached)
        .expect("完整缓存片段必须能在不创建 TTS client 的情况下拼接");

    assert_eq!(
        synthesized.pcm.bytes.len(),
        8_008,
        "完整缓存的名称与结果词拼接后必须包含 250ms 静音"
    );
    assert_eq!(
        &synthesized.pcm.bytes[..4],
        name_pcm.bytes.as_slice(),
        "缓存命中音频的开头必须保留名称 PCM"
    );
    assert!(
        synthesized.pcm.bytes[4..8_004]
            .iter()
            .all(|byte| *byte == 0),
        "缓存命中音频的中间必须保留 8,000-byte 静音停顿"
    );
    assert_eq!(
        &synthesized.pcm.bytes[8_004..],
        outcome_pcm.bytes.as_slice(),
        "缓存命中音频的结尾必须保留结果词 PCM"
    );
}
