"""Run pinned local ASR under an externally enforced network-denial sandbox."""

import argparse
from dataclasses import asdict
from datetime import datetime, timezone
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import random
import resource
import subprocess
import sys
import time

from .engines import QwenEngine, SenseVoiceEngine, ZipformerEngine
from .manifest import file_sha256, load_manifest
from .measurement import measure_utterance
from .offline import verify_network_denied
from .report import summarize_results
from .scoring import NORMALIZATION_VERSION, score_pair


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n")


def run(args):
    offline = verify_network_denied()
    import numpy as np
    import soundfile as sf
    rows = load_manifest(args.manifest)
    if args.limit:
        rows = rows[:args.limit]
    args.output.mkdir(parents=True, exist_ok=False)
    started = time.perf_counter()
    if args.engine == "zipformer":
        engine = ZipformerEngine(args.model_dir, threads=args.threads, hotwords=not args.no_hotwords)
    else:
        if args.no_hotwords:
            raise ValueError("--no-hotwords only applies to zipformer")
        engine_type = {"sensevoice": SenseVoiceEngine, "qwen": QwenEngine}[args.engine]
        engine = engine_type(args.model_dir, threads=args.threads)
    load_ms = (time.perf_counter() - started) * 1000
    root = Path(__file__).resolve().parent
    source_snapshot = args.output / "source"
    source_snapshot.mkdir()
    for path in sorted(root.glob("*.py")):
        (source_snapshot / path.name).write_bytes(path.read_bytes())
    metadata = {
        "started_at": datetime.now(timezone.utc).isoformat(), "state": "running",
        "offline": offline, "platform": platform.platform(), "machine": platform.machine(),
        "chip": subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip(),
        "memory_bytes": int(subprocess.check_output(["sysctl", "-n", "hw.memsize"], text=True)),
        "load_average_before_samples": os.getloadavg(),
        "python": sys.version, "model_construction_ms": load_ms, "engine": engine.config,
        "model_files": {key: {"path": str(path), "sha256": file_sha256(path),
                               "bytes": path.stat().st_size} for key, path in engine.files.items()},
        "manifest": {"path": str(args.manifest.resolve()), "sha256": file_sha256(args.manifest)},
        "source_sha256": {path.name: file_sha256(path) for path in sorted(root.glob("*.py"))},
        "offline_wrapper_sha256": file_sha256(root.parent / "run-offline.sh"),
        "dependencies": {name: importlib.metadata.version(name)
                         for name in ("numpy", "soundfile", "sherpa-onnx", "sherpa-onnx-core")},
        "normalization": NORMALIZATION_VERSION, "realtime": args.realtime,
        "chunk_samples": args.chunk_samples, "seed": args.seed, "repeats": args.repeats,
        "selected_ids": [r["id"] for r in rows], "cli": sys.argv,
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "scope": "Python binding of Sherpa 1.13.3; engine baseline, not the running Rust App",
    }
    write_json(args.output / "metadata.json", metadata)
    results = []
    generator = random.Random(args.seed)
    with (args.output / "results.jsonl").open("w") as writer:
        for repeat in range(args.repeats):
            order = list(rows)
            generator.shuffle(order)
            for row in order:
                try:
                    samples, rate = sf.read(row["audio_path"], dtype="float32")
                    if rate != 16000 or samples.ndim != 1:
                        raise ValueError("validated audio format changed")
                    record = measure_utterance(
                        engine.new_session(), np.asarray(samples), realtime=args.realtime,
                        chunk_samples=args.chunk_samples,
                        speech_onset_s=row.get("speech_onset_s"), speech_offset_s=row.get("speech_offset_s"))
                except Exception as error:
                    record = {"status": "failed", "hypothesis": "", "error": str(error),
                              "realtime": args.realtime}
                record.update({key: row[key] for key in ("id", "speaker", "language", "reference", "sha256")})
                record["repeat"] = repeat
                record["score"] = asdict(score_pair(row["reference"], record["hypothesis"]))
                writer.write(json.dumps(record, ensure_ascii=False, allow_nan=False) + "\n")
                writer.flush()
                results.append(record)
                print(f"{len(results)}/{len(rows) * args.repeats} {record['status']}", flush=True)
    usage = resource.getrusage(resource.RUSAGE_SELF)
    summary = summarize_results(results)
    summary.update({"model_construction_ms": load_ms,
                    "process_peak_rss_bytes": usage.ru_maxrss if sys.platform == "darwin" else usage.ru_maxrss * 1024,
                    "distinct_audio_samples": len(rows), "offline": offline})
    write_json(args.output / "summary.json", summary)
    metadata.update({"state": "complete", "finished_at": datetime.now(timezone.utc).isoformat(),
                     "results_sha256": file_sha256(args.output / "results.jsonl")})
    write_json(args.output / "metadata.json", metadata)
    print(json.dumps(summary, ensure_ascii=False, allow_nan=False))


def positive_int(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return number


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--engine", choices=("zipformer", "sensevoice", "qwen"), default="zipformer")
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--threads", type=positive_int, default=2)
    parser.add_argument("--chunk-samples", type=positive_int, default=320)
    parser.add_argument("--repeats", type=positive_int, default=1)
    parser.add_argument("--limit", type=positive_int)
    parser.add_argument("--seed", type=int, default=20261024)
    parser.add_argument("--no-hotwords", action="store_true")
    parser.add_argument("--realtime", action="store_true")
    run(parser.parse_args())
