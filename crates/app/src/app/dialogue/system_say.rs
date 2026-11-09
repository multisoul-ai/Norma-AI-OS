//! macOS `say` as a reply-level PCM client. Voices come from `say -v '?'`.

use audio::{PcmAudioChunk, PcmFormat};
use std::sync::Arc;

type SaySynth = fn(&str, &str) -> Result<Vec<u8>, String>;

pub(crate) struct SystemSayClient {
    voice: String,
    text: String,
    sink: Option<Arc<dyn tts::TtsChunkSink>>,
    cancelled: bool,
    synthesize: SaySynth,
}

impl SystemSayClient {
    pub(crate) fn new(voice: &str) -> Self {
        Self::with_synthesizer(voice, synthesize_with_say)
    }

    fn with_synthesizer(voice: &str, synthesize: SaySynth) -> Self {
        Self {
            voice: voice.to_string(),
            text: String::new(),
            sink: None,
            cancelled: false,
            synthesize,
        }
    }
}

pub(crate) fn list_system_voices() -> Vec<String> {
    let Ok(output) = std::process::Command::new("say").args(["-v", "?"]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_say_voice_listing(&String::from_utf8_lossy(&output.stdout))
}

pub(crate) fn parse_say_voice_listing(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        let Some(hash) = line.find(" #") else {
            continue;
        };
        let head = line[..hash].trim_end();
        let mut parts = head.rsplitn(2, char::is_whitespace);
        let locale = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("").trim();
        if name.is_empty() || !is_voice_locale(locale) {
            continue;
        }
        if !names.iter().any(|existing: &String| existing == name) {
            names.push(name.to_string());
        }
    }
    names
}

fn is_voice_locale(value: &str) -> bool {
    let mut parts = value.split('_');
    let (Some(language), Some(region)) = (parts.next(), parts.next()) else {
        return false;
    };
    parts.next().is_none()
        && (2..=3).contains(&language.len())
        && language.bytes().all(|byte| byte.is_ascii_lowercase())
        && region.len() == 2
        && region.bytes().all(|byte| byte.is_ascii_uppercase())
}

pub(crate) fn pcm16_mono_16k_from_wav(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("system voice audio is not a WAV".to_string());
    }
    let mut offset = 12usize;
    let mut rate = None;
    let mut channels = None;
    let mut bits = None;
    let mut data = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size =
            u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().expect("4 bytes")) as usize;
        let start = offset + 8;
        let end = start
            .checked_add(size)
            .ok_or_else(|| "system voice WAV chunk is too large".to_string())?;
        if end > bytes.len() {
            return Err("system voice WAV is truncated".to_string());
        }
        if id == b"fmt " && size >= 16 {
            channels = Some(u16::from_le_bytes([bytes[start + 2], bytes[start + 3]]));
            rate = Some(u32::from_le_bytes(
                bytes[start + 4..start + 8].try_into().expect("4 bytes"),
            ));
            bits = Some(u16::from_le_bytes([bytes[start + 14], bytes[start + 15]]));
        } else if id == b"data" {
            data = Some(bytes[start..end].to_vec());
        }
        offset = end + (size % 2);
    }
    if rate != Some(16_000) || channels != Some(1) || bits != Some(16) {
        return Err("system voice WAV must be 16 kHz mono PCM16".to_string());
    }
    let pcm = data.ok_or_else(|| "system voice WAV has no audio".to_string())?;
    if !pcm.len().is_multiple_of(2) {
        return Err("system voice PCM length is odd".to_string());
    }
    Ok(pcm)
}

fn synthesize_with_say(voice: &str, text: &str) -> Result<Vec<u8>, String> {
    let path = std::env::temp_dir().join(format!(
        "norma-say-{}-{}.wav",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let output = std::process::Command::new("say")
        .args([
            "-v",
            voice,
            "-o",
            &path.to_string_lossy(),
            "--file-format=WAVE",
            "--data-format=LEI16@16000",
            text,
        ])
        .output()
        .map_err(|error| error.to_string())?;
    let wav = std::fs::read(&path).map_err(|error| error.to_string());
    let _ = std::fs::remove_file(&path);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(error = %stderr.trim(), "system voice synthesis failed");
        return Err(ui::i18n::text("voice.system_voice_synthesis_failed").to_string());
    }
    wav
}

impl tts::TtsClient for SystemSayClient {
    fn start_stream(&mut self, sink: Arc<dyn tts::TtsChunkSink>) -> Result<(), tts::TtsError> {
        if self.sink.is_some() {
            return Err(tts::TtsError::new(
                tts::TtsErrorKind::Configuration,
                "system voice stream is already active",
            ));
        }
        self.sink = Some(sink);
        self.text.clear();
        self.cancelled = false;
        Ok(())
    }

    fn push_text(&mut self, text: &str) -> Result<(), tts::TtsError> {
        if self.cancelled {
            return Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "system voice stream was cancelled",
            ));
        }
        if self.sink.is_none() {
            return Err(tts::TtsError::new(
                tts::TtsErrorKind::Configuration,
                "system voice stream has not started",
            ));
        }
        self.text.push_str(text);
        Ok(())
    }

    fn finish_stream(&mut self) -> Result<(), tts::TtsError> {
        let Some(sink) = self.sink.take() else {
            return Err(tts::TtsError::new(
                tts::TtsErrorKind::Configuration,
                "system voice stream has not started",
            ));
        };
        if self.cancelled || sink.is_cancelled() {
            sink.finish(Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "system voice stream was cancelled",
            )));
            return Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "system voice stream was cancelled",
            ));
        }
        let text = self.text.trim();
        if text.is_empty() {
            sink.finish(Ok(tts::TtsStreamSummary::new(None, None, 0, 0, 0)));
            return Ok(());
        }
        let wav = match (self.synthesize)(&self.voice, text) {
            Ok(wav) => wav,
            Err(error) => {
                let failure = tts::TtsError::new(
                    tts::TtsErrorKind::Transport,
                    ui::i18n::text("voice.system_voice_synthesis_failed"),
                );
                tracing::warn!(error = %error, "system voice synthesis failed");
                sink.finish(Err(failure));
                return Err(tts::TtsError::new(
                    tts::TtsErrorKind::Transport,
                    ui::i18n::text("voice.system_voice_synthesis_failed"),
                ));
            }
        };
        let pcm = match pcm16_mono_16k_from_wav(&wav) {
            Ok(pcm) => pcm,
            Err(error) => {
                tracing::warn!(error = %error, "system voice audio rejected");
                let failure = tts::TtsError::new(
                    tts::TtsErrorKind::AudioDecode,
                    ui::i18n::text("voice.system_voice_audio_invalid"),
                );
                sink.finish(Err(failure));
                return Err(tts::TtsError::new(
                    tts::TtsErrorKind::AudioDecode,
                    ui::i18n::text("voice.system_voice_audio_invalid"),
                ));
            }
        };
        let byte_count = pcm.len() as u64;
        let chunk = match PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, pcm) {
            Ok(chunk) => chunk,
            Err(_) => {
                sink.finish(Err(tts::TtsError::new(
                    tts::TtsErrorKind::AudioDecode,
                    "system voice PCM was rejected",
                )));
                return Err(tts::TtsError::new(
                    tts::TtsErrorKind::AudioDecode,
                    "system voice PCM was rejected",
                ));
            }
        };
        if !sink.is_cancelled() {
            sink.send(chunk);
        }
        sink.finish(Ok(tts::TtsStreamSummary::new(None, None, 0, 1, byte_count)));
        Ok(())
    }

    fn cancel_stream(&mut self) {
        self.cancelled = true;
        if let Some(sink) = self.sink.take() {
            sink.finish(Err(tts::TtsError::new(
                tts::TtsErrorKind::Cancelled,
                "system voice stream was cancelled",
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tts::TtsClient;

    struct MemorySink {
        pcm: Mutex<Vec<u8>>,
        done: Mutex<Option<String>>,
    }

    impl tts::TtsChunkSink for MemorySink {
        fn send(&self, chunk: PcmAudioChunk) -> bool {
            self.pcm.lock().expect("pcm lock").extend(chunk.bytes);
            true
        }

        fn finish(&self, result: Result<tts::TtsStreamSummary, tts::TtsError>) {
            *self.done.lock().expect("done lock") = Some(match result {
                Ok(summary) => format!("ok:{}", summary.byte_count()),
                Err(error) => format!("err:{error}"),
            });
        }

        fn is_cancelled(&self) -> bool {
            false
        }
    }

    fn fixture_wav(pcm: &[u8]) -> Vec<u8> {
        let data_size = pcm.len() as u32;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + data_size).to_le_bytes());
        bytes.extend(b"WAVE");
        bytes.extend(b"fmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(16_000u32.to_le_bytes());
        bytes.extend(32_000u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(data_size.to_le_bytes());
        bytes.extend(pcm);
        bytes
    }

    fn fixture_synth(_: &str, text: &str) -> Result<Vec<u8>, String> {
        assert_eq!(text, "hello", "合成应收到完整回复文本");
        Ok(fixture_wav(&[0x01, 0x00, 0x02, 0x00]))
    }

    /// SAY-1：`say -v '?'` 的名单只保留声音名。
    ///
    /// 数据构造：两行「名字 + 语言 + 注释」，其中 Bad News 含空格；另有一行没有两个空格列。
    ///
    /// 执行过程：解析整段名单。
    ///
    /// 预期结果：
    ///   - 正断言：得到 Tingting 与 Bad News。
    ///   - 负断言：没有两个空格列的行不得进入名单。
    #[test]
    fn test_say_listing_keeps_spaced_voice_names() {
        let listing = "\
Tingting            zh_CN    # 你好
Bad News            en_US    # hello
Not a voice line
";
        assert_eq!(
            parse_say_voice_listing(listing),
            vec!["Tingting".to_string(), "Bad News".to_string()],
            "应解析出 Tingting 与 Bad News"
        );
        assert!(
            !parse_say_voice_listing(listing)
                .iter()
                .any(|name| name == "Not a voice line"),
            "缺少语言列的行不得进入系统声音名单"
        );
    }

    /// SAY-2：系统旁白把 16 kHz WAV 的 data 块交给回复播放，不经过本机模型目录。
    ///
    /// 数据构造：夹具 WAV 为 16 kHz、单声道、16 bit，PCM 为 01 00 02 00。文本是 hello。
    ///
    /// 执行过程：开始流、推入文本、完成合成。
    ///
    /// 预期结果：
    ///   - 正断言：播放端拿到 4 字节 PCM，完成摘要字节数为 4。
    ///   - 负断言：本测试不读模型目录；采样不是 16 kHz 时由 SAY-3 拒绝。
    #[test]
    fn test_system_client_sends_pcm_from_wav_fixture() {
        let sink = Arc::new(MemorySink {
            pcm: Mutex::new(Vec::new()),
            done: Mutex::new(None),
        });
        let mut client = SystemSayClient::with_synthesizer("Bad News", fixture_synth);
        client
            .start_stream(sink.clone())
            .expect("系统旁白流应能开始");
        client.push_text("hello").expect("文本应进入待合成缓冲");
        client.finish_stream().expect("夹具 WAV 应能完成");
        assert_eq!(
            sink.pcm.lock().expect("pcm").as_slice(),
            &[0x01, 0x00, 0x02, 0x00],
            "播放端必须拿到 16-bit PCM 采样"
        );
        assert_eq!(
            sink.done.lock().expect("done").as_deref(),
            Some("ok:4"),
            "完成摘要的字节数必须等于 PCM 长度"
        );
    }

    /// SAY-3：不是 16 kHz 单声道 PCM16 的 WAV 不得送出 PCM。
    ///
    /// 数据构造：与 SAY-2 相同的 data，但 fmt 采样率写成 8_000。
    ///
    /// 执行过程：调用 pcm16_mono_16k_from_wav。
    ///
    /// 预期结果：
    ///   - 正断言：返回错误。
    ///   - 负断言：不得返回 SAY-2 的 4 字节 PCM。
    #[test]
    fn test_system_wav_rejects_non_16k_audio() {
        let mut wav = fixture_wav(&[0x01, 0x00, 0x02, 0x00]);
        wav[24..28].copy_from_slice(&8_000u32.to_le_bytes());
        let decoded = pcm16_mono_16k_from_wav(&wav);
        assert!(decoded.is_err(), "8 kHz WAV 必须被拒绝");
        assert_ne!(
            decoded.ok().as_deref(),
            Some(&[0x01, 0x00, 0x02, 0x00][..]),
            "被拒绝的 WAV 不得把采样交给播放"
        );
    }
}
