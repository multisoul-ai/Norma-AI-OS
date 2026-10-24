"""Measure inference separately from real-time audio delivery and speech labels."""

import time

from .replay import replay_chunks
from .scoring import mixed_tokens


def measure_utterance(recognizer, samples, *, realtime: bool, chunk_samples=320,
                      speech_onset_s=None, speech_offset_s=None,
                      now=time.perf_counter_ns, sleep=time.sleep):
    if not len(samples):
        raise ValueError("cannot measure empty audio")
    origin = now()
    cpu_origin = time.process_time_ns()
    events, last_text = [], ""
    decode_ns, max_lag_ns = 0, 0
    for frame in replay_chunks(samples, 16000, chunk_samples, realtime=realtime,
                               now=now, sleep=sleep, origin_ns=origin):
        max_lag_ns = max(max_lag_ns, frame.delivered_ns - frame.deadline_ns)
        started = now()
        text = recognizer.accept(frame.samples).strip()
        decode_ns += now() - started
        if text != last_text:
            events.append({"time_ms": (now() - origin) / 1e6, "text": text,
                           "audio_samples": frame.end_sample})
            last_text = text
    started = now()
    final = recognizer.finish().strip()
    decode_ns += now() - started
    elapsed_ms = (now() - origin) / 1e6
    cpu_ms = (time.process_time_ns() - cpu_origin) / 1e6
    first_ms = next((e["time_ms"] for e in events if e["text"]), None) if realtime else None
    stable_ms = None
    final_tokens = mixed_tokens(final)
    if final_tokens and realtime:
        for index, event in enumerate(events):
            if all(mixed_tokens(later["text"])[:1] == final_tokens[:1]
                   for later in events[index:]):
                stable_ms = event["time_ms"]
                break
    duration_ms = len(samples) / 16
    return {
        "status": "ok" if final else "empty", "hypothesis": final,
        "realtime": realtime, "duration_s": duration_ms / 1000,
        "elapsed_ms": elapsed_ms, "decode_ms": decode_ns / 1e6, "cpu_ms": cpu_ms,
        "compute_rtf": decode_ns / 1e6 / duration_ms,
        "first_partial_from_file_start_ms": first_ms,
        "first_stable_prefix_from_file_start_ms": stable_ms,
        "first_partial_from_speech_start_ms": (
            first_ms - speech_onset_s * 1000
            if first_ms is not None and speech_onset_s is not None else None),
        "after_file_end_ms": elapsed_ms - duration_ms if realtime else None,
        "after_speech_end_ms": (elapsed_ms - speech_offset_s * 1000
                                if realtime and speech_offset_s is not None else None),
        "max_delivery_lag_ms": max_lag_ns / 1e6 if realtime else None,
        "partials": events,
    }
