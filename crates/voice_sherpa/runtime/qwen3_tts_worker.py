"""Qwen3-TTS worker on the managed A1 engine.

stdin: JSON lines {"id", "text", "voice"} or {"cancel": id}.
stdout: u32 little-endian length + PCM16 mono 24 kHz; 0 ends a request, 0xFFFFFFFF fails it.
The first 0 frame signals the model is loaded. Rust owns the process lifetime.
"""
import argparse
import json
import os
import struct
import sys
import threading
import time

END = 0
ERROR = 0xFFFFFFFF


def guard_parent(parent):
    # Native inference releases the GIL, so this still fires mid-generation.
    while True:
        if os.getppid() != parent:
            import signal
            os.killpg(os.getpgrp(), signal.SIGKILL)
        time.sleep(0.2)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", required=True)
    parser.add_argument("--parent", type=int, required=True)
    args = parser.parse_args()
    threading.Thread(target=guard_parent, args=(args.parent,), daemon=True).start()
    # mlx-audio prints progress to stdout; keep the real stdout private for frames only.
    out = os.fdopen(os.dup(1), "wb")
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    import numpy as np
    from mlx_audio.tts.utils import load_model

    model = load_model(args.model)
    requests, cancelled, ready = [], set(), threading.Condition()

    def read():
        for line in sys.stdin.buffer:
            message = json.loads(line)
            with ready:
                if "cancel" in message:
                    cancelled.add(message["cancel"])
                else:
                    requests.append(message)
                ready.notify()
        os._exit(0)

    threading.Thread(target=read, daemon=True).start()
    out.write(struct.pack("<I", END))
    out.flush()
    while True:
        with ready:
            while not requests:
                ready.wait()
            request = requests.pop(0)
        status = END
        try:
            for result in model.generate(text=request["text"], voice=request["voice"],
                                         stream=True, streaming_interval=0.32):
                if request["id"] in cancelled:
                    break
                audio = np.clip(np.asarray(result.audio, dtype=np.float32), -1, 1)
                pcm = (audio * 32767).astype("<i2").tobytes()
                out.write(struct.pack("<I", len(pcm)) + pcm)
                out.flush()
        except Exception:
            status = ERROR
        out.write(struct.pack("<I", status))
        out.flush()
        with ready:
            cancelled.discard(request["id"])


if __name__ == "__main__":
    main()
