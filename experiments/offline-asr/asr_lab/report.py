"""Aggregate every attempt; missing timings and empty transcripts stay visible."""

from collections import Counter
import math

from .scoring import aggregate_pairs, mixed_tokens


TIMINGS = ("first_partial_from_file_start_ms", "first_stable_prefix_from_file_start_ms",
           "first_partial_from_speech_start_ms", "after_file_end_ms", "after_speech_end_ms",
           "max_delivery_lag_ms", "decode_ms", "compute_rtf", "cpu_ms")


def distribution(values, total):
    values = sorted(value for value in values if value is not None and math.isfinite(value))
    def quantile(fraction):
        return values[max(0, math.ceil(len(values) * fraction) - 1)] if values else None
    return {"measured_samples": len(values), "missing_samples": total - len(values),
            "p50": quantile(0.5), "p95": quantile(0.95), "max": max(values) if values else None}


def summarize_results(rows):
    return {
        "samples": len(rows), "status_counts": dict(Counter(r["status"] for r in rows)),
        "failed_samples": sum(r["status"] == "failed" for r in rows),
        "empty_positive_samples": sum(bool(mixed_tokens(r["reference"]))
                                      and not mixed_tokens(r["hypothesis"]) for r in rows),
        "quality": aggregate_pairs((r["reference"], r["hypothesis"]) for r in rows),
        "by_language": {
            language: aggregate_pairs((r["reference"], r["hypothesis"])
                                      for r in rows if r["language"] == language)
            for language in sorted({r["language"] for r in rows})},
        "latency": {key: distribution([r.get(key) for r in rows], len(rows)) for key in TIMINGS},
        "scope": "engine/file-replay only; no microphone, VAD endpoint or UI latency evidence",
    }
