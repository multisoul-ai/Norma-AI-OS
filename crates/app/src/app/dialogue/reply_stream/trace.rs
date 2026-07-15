use super::QueuedPlaybackChunk;
use std::time::Duration;
use voice_sherpa::SpeechError;

const TTS_TRACE_MARKER: &str = "[TTS-TRACE]";

pub(super) fn reply_stream_started(reply_generation: u64) {
    tracing::info!(target: "norma_voice", reply_generation, "{TTS_TRACE_MARKER} reply stream started");
}

pub(super) fn dialogue_delta(reply_generation: u64, text: &str, accumulated_chars: usize) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        delta_chars = text.chars().count(),
        delta = %text,
        accumulated_chars,
        "{TTS_TRACE_MARKER} model dialogue delta received"
    );
}

pub(super) fn dialogue_finished(reply_generation: u64, reply: &str, final_segment_count: usize) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        reply_chars = reply.chars().count(),
        reply = %reply,
        final_segment_count,
        "{TTS_TRACE_MARKER} model dialogue stream finished"
    );
}

pub(super) fn reply_stream_aborted(
    reply_generation: u64,
    mark_interrupted: bool,
    queued_segments: usize,
    playback_chunks_pending: usize,
) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        mark_interrupted,
        queued_segments,
        playback_chunks_pending,
        "{TTS_TRACE_MARKER} reply stream aborted"
    );
}

pub(super) fn speech_segment_queued(reply_generation: u64, segment: &str) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        segment_chars = segment.chars().count(),
        segment = %segment,
        "{TTS_TRACE_MARKER} speech segment queued"
    );
}

pub(super) fn synthesis_requested(
    reply_generation: u64,
    tts_request_sequence: u64,
    text: &str,
    remaining_segment_count: usize,
) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        tts_request_sequence,
        text_chars = text.chars().count(),
        text = %text,
        remaining_segment_count,
        "{TTS_TRACE_MARKER} TTS synthesis requested"
    );
}

pub(super) fn synthesis_finished(
    reply_generation: u64,
    tts_request_sequence: u64,
    result: &Result<(), SpeechError>,
) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        tts_request_sequence,
        success = result.is_ok(),
        error = ?result.as_ref().err(),
        "{TTS_TRACE_MARKER} TTS synthesis worker finished"
    );
}

pub(super) fn pcm_received(reply_generation: u64, chunk: &QueuedPlaybackChunk) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        tts_request_sequence = chunk.tts_request_sequence,
        chunk_sequence = chunk.chunk_sequence,
        pcm_bytes = chunk.pcm.bytes.len(),
        sample_rate_hz = chunk.pcm.sample_rate_hz,
        channels = chunk.pcm.channels,
        pcm_fingerprint = format_args!("{:016x}", chunk.pcm_fingerprint),
        "{TTS_TRACE_MARKER} TTS PCM chunk received"
    );
}

pub(super) fn playback_accepted(
    reply_generation: u64,
    chunk: &QueuedPlaybackChunk,
    pcm_duration: Duration,
) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        tts_request_sequence = chunk.tts_request_sequence,
        chunk_sequence = chunk.chunk_sequence,
        pcm_bytes = chunk.pcm.bytes.len(),
        pcm_duration_ms = pcm_duration.as_millis(),
        pcm_fingerprint = format_args!("{:016x}", chunk.pcm_fingerprint),
        "{TTS_TRACE_MARKER} PCM chunk accepted by playback backend"
    );
}

pub(super) fn playback_started(
    reply_generation: u64,
    tts_request_sequence: u64,
    chunk_sequence: u64,
    pcm_fingerprint: u64,
) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        tts_request_sequence,
        chunk_sequence,
        pcm_fingerprint = format_args!("{:016x}", pcm_fingerprint),
        "{TTS_TRACE_MARKER} playback backend started PCM chunk"
    );
}

pub(super) fn playback_finished(
    reply_generation: u64,
    tts_request_sequence: u64,
    chunk_sequence: u64,
    pcm_fingerprint: u64,
) {
    tracing::info!(
        target: "norma_voice",
        reply_generation,
        tts_request_sequence,
        chunk_sequence,
        pcm_fingerprint = format_args!("{:016x}", pcm_fingerprint),
        "{TTS_TRACE_MARKER} playback backend finished PCM chunk"
    );
}
