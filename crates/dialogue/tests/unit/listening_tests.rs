use crate::clients::{AsrClient, AsrEvent, VadClient, VadEvent};
use crate::listening::{apply_asr_final, process_listening_pcm_chunk};
use crate::session::{DialoguePhase, DialogueSession};
use crate::DialogueError;
use audio::{PcmAudioChunk, PcmFormat};

struct FakeVad {
    scripted: Vec<Vec<VadEvent>>,
    index: usize,
}

impl FakeVad {
    fn new(scripted: Vec<Vec<VadEvent>>) -> Self {
        Self { scripted, index: 0 }
    }
}

impl VadClient for FakeVad {
    fn push_pcm(&mut self, _chunk: &PcmAudioChunk) -> Result<Vec<VadEvent>, DialogueError> {
        let events = self.scripted.get(self.index).cloned().unwrap_or_default();
        self.index = self.index.saturating_add(1);
        Ok(events)
    }

    fn reset(&mut self) {}
}

struct FakeAsr {
    finals: Vec<Option<String>>,
    partials: Vec<Vec<AsrEvent>>,
    push_index: usize,
    end_index: usize,
}

impl FakeAsr {
    fn new(finals: Vec<Option<String>>, partials: Vec<Vec<AsrEvent>>) -> Self {
        Self {
            finals,
            partials,
            push_index: 0,
            end_index: 0,
        }
    }
}

impl AsrClient for FakeAsr {
    fn push_pcm(&mut self, _chunk: &PcmAudioChunk) -> Result<Vec<AsrEvent>, DialogueError> {
        let events = self
            .partials
            .get(self.push_index)
            .cloned()
            .unwrap_or_default();
        self.push_index = self.push_index.saturating_add(1);
        Ok(events)
    }

    fn end_utterance(&mut self) -> Result<Option<String>, DialogueError> {
        let text = self.finals.get(self.end_index).cloned().flatten();
        self.end_index = self.end_index.saturating_add(1);
        Ok(text)
    }

    fn reset(&mut self) {}
}

fn test_pcm_chunk() -> PcmAudioChunk {
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 0, 0, 0])
        .expect("测试 PCM 块应通过校验")
}

/// VAD-ASR-1: SpeechEnd 后 Asr final 会把 session 推到 Thinking。
///
/// 数据构造：
///   FakeVad 先 SpeechStart 再 SpeechEnd；FakeAsr end_utterance 返回 "你好"。
///
/// 执行过程：
///   1. session 进入 Listening
///   2. 驱动 process_listening_pcm_chunk 两轮（起说、收段）
///   3. 对 final transcript 调用 apply_asr_final
///
/// 预期结果：
///   - 正断言：phase == Thinking
///   - 正断言：pending_transcript == Some("你好")
#[test]
fn test_speech_end_final_transcript_moves_session_to_thinking() {
    let mut session = DialogueSession::new();
    session.on_listening_started();
    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "前置：应在 Listening"
    );

    let mut vad = FakeVad::new(vec![vec![VadEvent::SpeechStart], vec![VadEvent::SpeechEnd]]);
    let mut asr = FakeAsr::new(vec![Some("你好".to_string())], vec![vec![], vec![]]);
    let mut speech_active = false;

    let first =
        process_listening_pcm_chunk(&mut vad, &mut asr, &test_pcm_chunk(), &mut speech_active)
            .expect("起说轮次应成功");
    assert!(
        first.final_transcript.is_none(),
        "SpeechStart 轮次不应产生 final transcript"
    );
    assert!(speech_active, "SpeechStart 后 speech_active 应为 true");

    let second =
        process_listening_pcm_chunk(&mut vad, &mut asr, &test_pcm_chunk(), &mut speech_active)
            .expect("收段轮次应成功");
    assert_eq!(
        second.final_transcript.as_deref(),
        Some("你好"),
        "SpeechEnd 后应拿到 ASR final"
    );
    assert!(!speech_active, "SpeechEnd 后 speech_active 应为 false");

    apply_asr_final(&mut session, second.final_transcript.as_deref().unwrap())
        .expect("apply_asr_final 应成功");

    assert_eq!(
        session.phase(),
        DialoguePhase::Thinking,
        "final transcript 后应进入 Thinking"
    );
    assert_eq!(
        session.pending_transcript().as_deref(),
        Some("你好"),
        "Thinking 前应保存 pending_transcript"
    );
}

/// VAD-ASR-2: 空 final transcript 不应推进 session。
///
/// 预期结果：
///   - 负断言：apply_asr_final("") 返回 Err
///   - 正断言：phase 仍为 Listening
#[test]
fn test_apply_asr_final_rejects_empty_transcript() {
    let mut session = DialogueSession::new();
    session.on_listening_started();

    assert_eq!(
        apply_asr_final(&mut session, ""),
        Err(DialogueError::Message("empty transcript".to_string())),
        "空 transcript 应返回 Err"
    );
    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "空 transcript 不应改变 phase"
    );
}

/// VAD-ASR-3: 非 Listening 阶段 apply_asr_final 应返回 Err。
#[test]
fn test_apply_asr_final_requires_listening_phase() {
    let mut session = DialogueSession::new();

    assert_eq!(
        apply_asr_final(&mut session, "你好"),
        Err(DialogueError::Message(
            "on_utterance_final requires Listening, got Idle".to_string()
        )),
        "Idle 下 apply_asr_final 应失败"
    );
}
