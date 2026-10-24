"""PCM delivery paced by end-of-chunk sample deadlines, with no tail dropping."""

from dataclasses import dataclass
import time
from typing import Sequence


@dataclass(frozen=True)
class ReplayChunk:
    samples: Sequence
    start_sample: int
    end_sample: int
    deadline_ns: int
    delivered_ns: int


def replay_chunks(samples, sample_rate: int, chunk_samples: int, *,
                  realtime: bool, now=time.perf_counter_ns, sleep=time.sleep):
    if sample_rate <= 0 or chunk_samples <= 0:
        raise ValueError("sample rate and chunk size must be positive")
    origin = now()
    for start in range(0, len(samples), chunk_samples):
        end = min(start + chunk_samples, len(samples))
        deadline = origin + round(end * 1_000_000_000 / sample_rate)
        if realtime:
            while (remaining := deadline - now()) > 0:
                sleep(remaining / 1_000_000_000)
        yield ReplayChunk(samples[start:end], start, end,
                          deadline - origin, now() - origin)
