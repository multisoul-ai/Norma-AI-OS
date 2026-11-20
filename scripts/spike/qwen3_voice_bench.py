"""Spike: Qwen3-TTS / Qwen3-ASR latency, RTF, memory and round-trip CER on Apple Silicon.

Usage:
  python qwen3_voice_bench.py tts <hf-repo> <out-dir> [speaker]
  python qwen3_voice_bench.py asr <hf-repo> <wav-dir> [--hotwords]
One model per process so peak memory is attributable.
"""
import json
import resource
import sys
import time
import wave
from pathlib import Path

import mlx.core as mx
import numpy as np

TEXTS = [
    "好的，已经帮你新建了一个 Claude 卡片。",
    "Codex 那边的测试跑完了，有三个用例失败，要我帮你看看吗？",
    "我把 cargo test 的输出整理好了，主要问题在 voice_sherpa 的 finalizer 模块。",
    "请确认是否允许 Grok 修改 crates/app 目录下的文件。",
    "Today's build passed. 下一步我们可以把 Qwen3 TTS 接进设置页。",
]
HOTWORDS = ["Claude", "Codex", "Grok", "cargo test", "voice_sherpa", "finalizer", "Qwen3", "crates/app", "TTS"]


def rss_mb():
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024 / 1024  # macOS reports bytes


def write_wav(path, audio, rate):
    pcm = (np.clip(np.asarray(audio, dtype=np.float32), -1, 1) * 32767).astype("<i2")
    with wave.open(str(path), "wb") as f:
        f.setnchannels(1), f.setsampwidth(2), f.setframerate(rate)
        f.writeframes(pcm.tobytes())


def tts(repo, out_dir, speaker):
    from mlx_audio.tts.utils import load_model

    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    t0 = time.perf_counter()
    model = load_model(repo)
    load_s = time.perf_counter() - t0
    speakers = model.get_supported_speakers()
    speaker = speaker or (speakers[0] if speakers else None)
    rows = []
    for i, text in enumerate([TEXTS[0]] + TEXTS):  # first run is warm-up, dropped
        t0 = time.perf_counter()
        first, chunks, rate = None, [], 24000
        for r in model.generate(text=text, voice=speaker, stream=True, streaming_interval=0.32):
            mx.eval(r.audio)
            first = first or time.perf_counter() - t0
            chunks.append(np.asarray(r.audio))
            rate = r.sample_rate
        total = time.perf_counter() - t0
        audio = np.concatenate(chunks)
        dur = len(audio) / rate
        if i == 0:
            continue
        write_wav(out / f"{i - 1}.wav", audio, rate)
        rows.append({"i": i - 1, "ttfa_ms": round(first * 1000), "gen_s": round(total, 2),
                     "audio_s": round(dur, 2), "rtf": round(total / dur, 3)})
    (out / "texts.json").write_text(json.dumps(TEXTS, ensure_ascii=False))
    print(json.dumps({"model": repo, "speaker": speaker, "speakers": speakers, "load_s": round(load_s, 1),
                      "peak_mlx_gb": round(mx.get_peak_memory() / 1e9, 2), "rss_gb": round(rss_mb() / 1024, 2),
                      "rows": rows}, ensure_ascii=False))


def cer(ref, hyp):
    norm = lambda s: [c for c in s.lower() if c.isalnum()]
    a, b = norm(ref), norm(hyp)
    d = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        prev, d[0] = d[0], i
        for j, cb in enumerate(b, 1):
            prev, d[j] = d[j], min(d[j] + 1, d[j - 1] + 1, prev + (ca != cb))
    return d[-1] / max(1, len(a))


def asr(repo, wav_dir, hotwords):
    from mlx_audio.stt.utils import load_model

    wav = Path(wav_dir)
    texts = json.loads((wav / "texts.json").read_text())
    t0 = time.perf_counter()
    model = load_model(repo)
    load_s = time.perf_counter() - t0
    model.generate(str(wav / "0.wav"))  # warm-up
    rows = []
    for i, ref in enumerate(texts):
        path = wav / f"{i}.wav"
        with wave.open(str(path)) as f:
            dur = f.getnframes() / f.getframerate()
        t0 = time.perf_counter()
        hyp = model.generate(str(path), hotwords=HOTWORDS if hotwords else None).text
        took = time.perf_counter() - t0
        rows.append({"i": i, "cer": round(cer(ref, hyp), 3), "rtf": round(took / dur, 3), "hyp": hyp})
    print(json.dumps({"model": repo, "hotwords": hotwords, "load_s": round(load_s, 1),
                      "peak_mlx_gb": round(mx.get_peak_memory() / 1e9, 2),
                      "mean_cer": round(sum(r["cer"] for r in rows) / len(rows), 3), "rows": rows},
                     ensure_ascii=False))


if __name__ == "__main__":
    mode, repo, path, *rest = sys.argv[1:]
    if mode == "tts":
        tts(repo, path, rest[0] if rest else None)
    else:
        asr(repo, path, "--hotwords" in rest)
