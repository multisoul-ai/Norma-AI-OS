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
