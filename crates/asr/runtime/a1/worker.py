"""A1 v1 worker. Binary PCM16 stdin/stdout protocol, local paths only.

Inference parameters ported from the audited A1 experiment (2026-10-26).
Each invocation owns ONE backend. Rust owns both child lifetimes and deadlines.
No transcript logging, preview fallback, network download or child launcher.
"""
import argparse
import errno
import importlib.metadata
import json
import os
from pathlib import Path
import socket
import struct
import sys
import threading
import time

VERSION = 1
MAX_HEADER = 4096
MAX_RESPONSE = 65536
MAX_CHUNK = 32000  # 1 s × 16000 samples/s × PCM16
MAX_AUDIO = 1920000  # 60 s, original audio only
VERSIONS = {"mlx": "0.32.4", "mlx-audio": "0.6.1", "numpy": "2.2.6",
            "sherpa-onnx": "1.13.3", "transformers": "5.20.2"}


def read_exact(source, count):
    chunks = bytearray()
    while len(chunks) < count:
        part = source.read(count - len(chunks))
        if not part:
            raise EOFError("truncated frame")
        chunks.extend(part)
    return bytes(chunks)


def send(output, value):
    encoded = json.dumps(value, ensure_ascii=False).encode()
    if len(encoded) > MAX_RESPONSE:
        raise ValueError("response too large")
    output.write(struct.pack("<I", len(encoded)) + encoded)
    output.flush()


def serve(source, output, backend, role):
    session = seq = total = 0
    active = False
    while first := source.read(1):
        length = struct.unpack("<I", first + read_exact(source, 3))[0]
        if not 0 < length <= MAX_HEADER:
            raise ValueError("header too large")
        request = json.loads(read_exact(source, length))
        version, next_session, next_seq, op, count = (
            request[k] for k in ("v", "session", "seq", "op", "bytes"))
        if (version != VERSION or type(next_session) is not int or next_session <= 0
                or type(next_seq) is not int or type(count) is not int
                or count < 0 or count > MAX_CHUNK or count % 2):
            raise ValueError("invalid envelope")
        if op == "begin":
            if active or next_session <= session or next_seq != 1 or count:
                raise ValueError("invalid session start")
            session, seq, total, active = next_session, 1, 0, True
            backend.begin()
            text = ""
        else:
            if not active or next_session != session or next_seq != seq + 1:
                raise ValueError("stale or out-of-order request")
            seq = next_seq
            if op == "audio" and 0 < count and total + count <= MAX_AUDIO:
                text = backend.accept(read_exact(source, count))
                total += count
            elif op == "finish" and not count:
                text = backend.finish() if total else ""
                active = False
            else:
                raise ValueError("invalid operation or audio limit")
        send(output, dict(v=VERSION, session=session, seq=seq, op=op, text=text,
                          source=role, fallback=False))


class Zipformer:
    def __init__(self, root):
        import sherpa_onnx
        import tempfile
        self.hotwords = tempfile.NamedTemporaryFile(mode="w", suffix=".txt")
        self.hotwords.write("JAVA\nKOTLIN\n")
        self.hotwords.flush()
        self.recognizer = sherpa_onnx.OnlineRecognizer.from_transducer(
            encoder=str(root / "encoder-epoch-99-avg-1.int8.onnx"),
            decoder=str(root / "decoder-epoch-99-avg-1.onnx"),
            joiner=str(root / "joiner-epoch-99-avg-1.int8.onnx"),
            tokens=str(root / "tokens.txt"), bpe_vocab=str(root / "bpe.vocab"),
            num_threads=2, provider="cpu", modeling_unit="cjkchar+bpe",
            decoding_method="modified_beam_search", max_active_paths=8,
            hotwords_score=2.0, hotwords_file=self.hotwords.name, enable_endpoint_detection=False)

    def begin(self):
        self.stream = self.recognizer.create_stream()

    def accept(self, pcm):
        import numpy as np
        self.stream.accept_waveform(16000, np.frombuffer(pcm, dtype="<i2").astype(np.float32) / 32768.0)
        return self.decode()

    def decode(self):
        while self.recognizer.is_ready(self.stream):
            self.recognizer.decode_stream(self.stream)
        return self.recognizer.get_result(self.stream)

    def finish(self):
        import numpy as np
        self.stream.accept_waveform(16000, np.zeros(10560, dtype=np.float32))  # 660 ms
        self.stream.input_finished()
        return self.decode()


class Qwen:
    def __init__(self, root):
        import mlx.core as mx
        from mlx_audio.stt.utils import load_model
        config = json.loads((root / "config.json").read_text())
        if config["model_type"] != "qwen3_asr" or config["quantization"]["bits"] != 8:
            raise ValueError("A1 requires Qwen3-ASR MLX 8-bit")
        self.mx = mx
        self.model = load_model(root.resolve(), lazy=False, strict=True)
        self.model.eval()
        mx.eval(self.model.parameters())
        mx.synchronize()

    def begin(self):
        self.audio = bytearray()

    def accept(self, pcm):
        self.audio.extend(pcm)
        return ""

    def finish(self):
        import numpy as np
        samples = np.frombuffer(self.audio, dtype="<i2").astype(np.float32) / 32768.0
        try:
            result = self.model.generate(
                samples, max_tokens=512, batch_size=1, temperature=0.0,
                top_p=1.0, top_k=0, language=None, stream=False,
                chunk_duration=1200.0, verbose=False, system_prompt=None, hotwords=None)
            self.mx.synchronize()
            return result.text
        finally:
            self.audio.clear()


def guard_parent(parent):
    # Runs even when Python is waiting in a native inference call (GIL released).
    # The separate process group also contains any native helper descendants.
    while True:
        if os.getppid() != parent:
            import signal
            os.killpg(os.getpgrp(), signal.SIGKILL)
        time.sleep(0.2)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--role", choices=["zipformer", "qwen"], required=True)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--parent", type=int, required=True)
    args = parser.parse_args()
    threading.Thread(target=guard_parent, args=(args.parent,), daemon=True).start()
    # Isolate protocol output even if a library prints to fd 1 during model load.
    output = os.fdopen(os.dup(sys.stdout.fileno()), "wb", buffering=0)
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    try:
        with socket.socket() as probe:
            probe.connect(("127.0.0.1", 9))
    except OSError as error:
        if error.errno not in (errno.EPERM, errno.EACCES):
            raise RuntimeError("OS network denial is required") from error
    else:
        raise RuntimeError("OS network denial is required")
    for package, version in VERSIONS.items():
        if importlib.metadata.version(package) != version:
            raise RuntimeError(f"A1 runtime version mismatch: {package}")
    backend = Zipformer(args.model) if args.role == "zipformer" else Qwen(args.model)
    send(output, dict(v=VERSION, op="ready", source=args.role))
    serve(sys.stdin.buffer, output, backend, args.role)


if __name__ == "__main__":
    main()
