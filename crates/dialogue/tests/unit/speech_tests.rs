use crate::{
    DialogueError, SpokenTextNormalizer, TtsChunkSink, TtsClient, UtteranceCommitter,
    MAX_SPEECH_SEGMENT_CHARS, SHORT_SPEECH_MERGE_WINDOW,
};
use audio::{PcmAudioChunk, PcmFormat};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct OneChunkTts;

impl TtsClient for OneChunkTts {
    fn synthesize(&mut self, _text: &str) -> Result<PcmAudioChunk, DialogueError> {
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 0])
            .map_err(|error| DialogueError::Message(format!("测试 PCM 应合法：{error:?}")))
    }
}

#[derive(Default)]
struct CollectingTtsChunkSink {
    chunks: Mutex<Vec<PcmAudioChunk>>,
}

impl TtsChunkSink for CollectingTtsChunkSink {
    fn send(&self, chunk: PcmAudioChunk) -> bool {
        self.chunks
            .lock()
            .expect("测试 sink lock 不应中毒")
            .push(chunk);
        true
    }

    fn is_cancelled(&self) -> bool {
        false
    }
}

/// SPEECH-1：播报文本应移除展示格式，并以固定提示替换不可自然朗读的内容。
///
/// 数据构造：
///   原文同时包含 Markdown 加粗、列表符、行内 code、代码块与 HTTPS 链接；
///   文本结论「已创建 terminal」与数字「3」属于用户应听到的事实。
///
/// 执行过程：
///   1. 调用 `SpokenTextNormalizer::normalize` 生成独立的播报文本；
///   2. 不修改调用方持有的原始展示文本。
///
/// 预期结果：
///   - 正断言：事实文本与数字保留；
///   - 正断言：代码块和链接各替换为一条可播报提示；
///   - 负断言：结果不含 Markdown 标记、原始 URL 或代码内容。
#[test]
fn test_spoken_text_normalizer_preserves_facts_and_replaces_display_only_content() {
    let display_text = "**结论：**\n- 已创建 `terminal`，共有 3 个节点。\n```bash\ncargo test\n```\n详情见 https://example.com/report。";

    let spoken_text = SpokenTextNormalizer::normalize(display_text);

    assert!(spoken_text.contains("结论"), "播报文本应保留原始结论");
    assert!(
        spoken_text.contains("已创建 terminal，共有 3 个节点"),
        "播报文本应保留 terminal 与节点数量事实，实际：{spoken_text:?}"
    );
    assert!(
        spoken_text.contains("详细内容已显示在屏幕上"),
        "代码块应替换为可播报提示，实际：{spoken_text:?}"
    );
    assert!(
        spoken_text.contains("链接已显示在屏幕上"),
        "链接应替换为可播报提示，实际：{spoken_text:?}"
    );
    assert!(
        !spoken_text.contains("**") && !spoken_text.contains("```"),
        "播报文本不应残留 Markdown 标记，实际：{spoken_text:?}"
    );
    assert!(
        !spoken_text.starts_with('#'),
        "播报文本不应以 Markdown 标题标记开头，实际：{spoken_text:?}"
    );
    assert!(
        !spoken_text.contains("https://") && !spoken_text.contains("cargo test"),
        "播报文本不应逐字符朗读链接或代码，实际：{spoken_text:?}"
    );
}

/// SPEECH-2：Markdown 标题和列表前缀只服务展示，不能成为播报内容。
///
/// 数据构造：
///   两行文本分别以 `## ` 与 `- ` 作为展示前缀，正文为「状态」和「正常」。
///
/// 执行过程：
///   1. 调用 `SpokenTextNormalizer::normalize`。
///
/// 预期结果：
///   - 正断言：正文按自然空格连接；
///   - 负断言：结果不含 `#` 或列表连字符前缀。
#[test]
fn test_spoken_text_normalizer_removes_heading_and_list_prefixes() {
    let spoken_text = SpokenTextNormalizer::normalize("## 状态\n- 正常");

    assert_eq!(
        spoken_text, "状态 正常",
        "播报文本应只保留标题和列表的正文，实际：{spoken_text:?}"
    );
    assert!(
        !spoken_text.contains('#') && !spoken_text.starts_with('-'),
        "播报文本不应残留标题或列表展示前缀，实际：{spoken_text:?}"
    );
}

/// SPEECH-3：短确认句后快速到达的后文必须合并，避免 Matcha 为每句重新起调。
///
/// 数据构造：
///   第一段「好的。」长度 = 3 个字符，小于最小发声段 8 个字符；
///   第二段「正在处理。」在 100ms 后到达，小于 250ms 合并窗口。
///
/// 执行过程：
///   1. 0ms 推入第一段；
///   2. 100ms 推入第二段；
///   3. 读取本次可提交的发声段。
///
/// 预期结果：
///   - 正断言：只产生一个含两句的完整发声段；
///   - 负断言：第一段不能在后文到达前被单独提交。
#[test]
fn test_utterance_committer_merges_short_sentence_when_following_text_arrives_within_window() {
    let mut committer = UtteranceCommitter::new();

    let first = committer.push_delta("好的。", Duration::ZERO);
    let merged = committer.push_delta("正在处理。", Duration::from_millis(100));

    assert!(
        first.is_empty(),
        "3 字短确认句应在合并窗口内等待后文，实际：{first:?}"
    );
    assert_eq!(
        merged,
        vec!["好的。正在处理。".to_string()],
        "100ms 内到达后文时，应合并为一个发声段"
    );
    assert!(
        SHORT_SPEECH_MERGE_WINDOW > Duration::from_millis(100),
        "测试前提：100ms 必须小于短句合并窗口"
    );
}

/// SPEECH-4：达到最小语境的完整中文句应立即提交，未完成后文必须继续缓冲。
///
/// 数据构造：
///   「这是一条足够长的完整句。」共 11 个字符，超过最小发声段 8 字；
///   「后文仍在生成」没有强标点，不能与前句一起提交。
///
/// 执行过程：
///   1. 单次推入完整句与未完成后文；
///   2. 读取即时提交结果；
///   3. 调用 `finish` 刷新剩余后文。
///
/// 预期结果：
///   - 正断言：即时结果只含完整句；
///   - 正断言：finish 后才得到未完成后文；
///   - 负断言：第一段不得吞并后文。
#[test]
fn test_utterance_committer_keeps_unfinished_chinese_tail_buffered() {
    let mut committer = UtteranceCommitter::new();

    let immediate = committer.push_delta("这是一条足够长的完整句。后文仍在生成", Duration::ZERO);
    let final_tail = committer.finish();

    assert_eq!(
        immediate,
        vec!["这是一条足够长的完整句。".to_string()],
        "完整句应即时提交，后文必须保持缓冲"
    );
    assert_eq!(
        final_tail,
        vec!["后文仍在生成".to_string()],
        "模型完成后才应刷新未完成后文"
    );
    assert!(
        !immediate[0].contains("后文"),
        "即时发声段不应吞并未完成后文，实际：{:?}",
        immediate[0]
    );
}

/// SPEECH-5：没有后文的短确认句在 250ms 合并窗口到期后必须独立提交。
///
/// 数据构造：
///   「好的。」长度 = 3 个字符，小于 8 字最小发声段；
///   到期前检查点 = 249ms；到期检查点 = 250ms。
///
/// 执行过程：
///   1. 0ms 推入短句；
///   2. 249ms 调用 `on_idle`；
///   3. 250ms 再次调用 `on_idle`。
///
/// 预期结果：
///   - 正断言：250ms 恰好到期时提交「好的。」；
///   - 负断言：249ms 时不得过早提交；
///   - 负断言：到期后不得重复提交。
#[test]
fn test_utterance_committer_flushes_short_sentence_when_merge_window_expires() {
    let mut committer = UtteranceCommitter::new();

    let initial = committer.push_delta("好的。", Duration::ZERO);
    let before_deadline = committer.on_idle(Duration::from_millis(249));
    let at_deadline = committer.on_idle(SHORT_SPEECH_MERGE_WINDOW);
    let after_flush = committer.on_idle(Duration::from_millis(300));

    assert!(initial.is_empty(), "短句首次到达时应等待合并窗口");
    assert!(
        before_deadline.is_empty(),
        "249ms 小于 250ms 合并窗口时不得过早提交"
    );
    assert_eq!(
        at_deadline,
        vec!["好的。".to_string()],
        "250ms 到期时应提交独立短确认句"
    );
    assert!(
        after_flush.is_empty(),
        "已提交的短句不应在后续 idle tick 重复提交"
    );
}

/// SPEECH-6：模型长时间不输出标点时，发声段必须在硬上限处提交以保证首响。
///
/// 数据构造：
///   输入由 `MAX_SPEECH_SEGMENT_CHARS` 个「甲」组成，不含任何强标点；
///   60 字上限避免等待模型生成无限长的段落。
///
/// 执行过程：
///   1. 单次推入正好达到硬上限的文本；
///   2. 读取即时提交结果。
///
/// 预期结果：
///   - 正断言：即时产生一个等长发声段；
///   - 负断言：不得因缺少句号而继续无限缓冲。
#[test]
fn test_utterance_committer_forces_commit_at_maximum_segment_length() {
    let mut committer = UtteranceCommitter::new();
    let unpunctuated = "甲".repeat(MAX_SPEECH_SEGMENT_CHARS);

    let committed = committer.push_delta(&unpunctuated, Duration::ZERO);

    assert_eq!(
        committed,
        vec![unpunctuated],
        "无标点文本达到 60 字硬上限时应立即提交"
    );
    assert_eq!(
        committed[0].chars().count(),
        MAX_SPEECH_SEGMENT_CHARS,
        "提交段长度应等于硬上限，避免延迟失控"
    );
}

/// SPEECH-7：流式模型的思考块不得进入 TTS，思考结论与最终答案不得重复播报。
///
/// 数据构造：
///   开始标签被拆成 `<thi` 与 `nk>`，结束标签被拆成 `</thi` 与 `nk>`；
///   思考块和最终答案都包含「应该重启服务」，用于复现用户听到的部分重复。
///
/// 执行过程：
///   1. 按真实流式顺序推入三个 delta；
///   2. 收集每次即时提交的 TTS 段；
///   3. 模型结束后调用 `finish` 刷出可见尾段。
///
/// 预期结果：
///   - 正断言：只产生一次最终答案「最终回答：应该重启服务。」；
///   - 负断言：结果不含 `<think>`、内部分析或第二次「应该重启服务」。
#[test]
fn test_utterance_committer_suppresses_split_thinking_block_and_repeated_conclusion() {
    let mut committer = UtteranceCommitter::new();
    let mut spoken = Vec::new();

    spoken.extend(committer.push_delta("<thi", Duration::ZERO));
    spoken.extend(committer.push_delta(
        "nk>我需要先分析。关键结论是应该重启服务。</thi",
        Duration::from_millis(10),
    ));
    spoken.extend(committer.push_delta("nk>最终回答：应该重启服务。", Duration::from_millis(20)));
    spoken.extend(committer.finish());

    assert_eq!(
        spoken,
        vec!["最终回答：应该重启服务。".to_string()],
        "TTS 应跳过跨 delta 的完整思考块，并只播一次最终答案；实际：{spoken:?}"
    );
    let joined = spoken.join(" ");
    assert!(
        !joined.contains("<think>") && !joined.contains("我需要先分析"),
        "TTS 不得泄漏思考标签或内部分析；实际：{joined:?}"
    );
    assert_eq!(
        joined.matches("应该重启服务").count(),
        1,
        "思考结论与最终答案重合时只能播报最终答案一次；实际：{joined:?}"
    );
}

/// SPEECH-7b：跨越 60 字分段边界的 Markdown 代码块不能吞掉代码块后的回复结论。
///
/// 数据构造（含关键数值的推导过程）：
///   prefix              =「下面是诊断过程。」（8 个字符，达到即时提交下限）；
///   code payload        = 65 个 `x`，大于 60 字硬分段上限；
///   forced code chunks  = 至少 1 个，因此 opening fence 与 closing fence 落在不同分段；
///   visible tail        =「最后结论必须完整朗读。」（用户应听到的后半段正文）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 单次推入前缀、跨分段代码块和代码块后的结论；
///   2. 收集即时提交的 speech segment；
///   3. 调用 `finish` 刷出最后不足 60 字的尾段，并合并实际播报文本。
///
/// 预期结果：
///   - 正断言：播报中保留代码块前的诊断前缀；
///   - 正断言：代码块只转换为一次屏幕提示；
///   - 正断言：代码块后的最后结论仍被完整朗读；
///   - 负断言：65 个 `x` 的代码内容不得进入 TTS。
#[test]
fn test_utterance_committer_preserves_visible_tail_after_cross_segment_code_block() {
    let mut committer = UtteranceCommitter::new();
    let code_payload = "x".repeat(65);
    let display_text =
        format!("下面是诊断过程。\n```text\n{code_payload}\n```\n最后结论必须完整朗读。");

    let mut spoken = committer.push_delta(&display_text, Duration::ZERO);
    spoken.extend(committer.finish());
    let joined = spoken.join(" ");

    assert!(
        joined.contains("下面是诊断过程。"),
        "长回复播报必须保留代码块前的可见正文，实际：{joined:?}"
    );
    assert_eq!(
        joined.matches("详细内容已显示在屏幕上").count(),
        1,
        "跨分段代码块只能产生一次屏幕提示，实际：{joined:?}"
    );
    assert!(
        joined.contains("最后结论必须完整朗读。"),
        "跨分段代码块不得吞掉 AI 回复后半段结论，实际：{joined:?}"
    );
    assert!(
        !joined.contains(&code_payload),
        "代码块内容不得作为普通正文进入 TTS，实际：{joined:?}"
    );
}

/// SPEECH-8：旧式整句 TTS 实现通过默认适配仍可向流式播放端交付 PCM。
///
/// 数据构造：
///   `OneChunkTts` 对任意文本返回 16kHz/mono/PCM16 的单帧静音块；
///   `CollectingTtsChunkSink` 记录收到的块且从不取消。
///
/// 执行过程：
///   1. 调用公开的 `TtsClient::synthesize_stream`；
///   2. 读取 sink 中的音频块。
///
/// 预期结果：
///   - 正断言：调用成功且 sink 收到一块原始 PCM；
///   - 负断言：默认适配不得丢弃旧实现的整句结果。
#[test]
fn test_tts_client_default_stream_adapter_forwards_full_utterance_chunk() {
    let mut tts = OneChunkTts;
    let sink = Arc::new(CollectingTtsChunkSink::default());

    let result = tts.synthesize_stream("本地兼容测试", sink.clone());
    let chunks = sink.chunks.lock().expect("测试 sink lock 不应中毒");

    assert_eq!(result, Ok(()), "默认流式适配应成功转发整句 PCM");
    assert_eq!(chunks.len(), 1, "默认流式适配应恰好转发一块 PCM");
    assert_eq!(
        chunks[0].sample_rate_hz, 16_000,
        "转发后的 PCM 应保留原始采样率"
    );
    assert_eq!(chunks[0].bytes, vec![0, 0], "转发后的 PCM 不得丢失样本");
}
