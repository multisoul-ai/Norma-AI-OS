"""Serve the prototype and synthesize local macOS speech as analyzable PCM."""

import argparse
from functools import lru_cache, partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory
import threading

ROOT = Path(__file__).resolve().parent
SYNTHESIS = threading.BoundedSemaphore(2)


@lru_cache(maxsize=32)
def synthesize(text):
    with SYNTHESIS, TemporaryDirectory(prefix="norma-voice-") as temporary:
        directory = Path(temporary)
        source = directory / "reply.txt"
        aiff = directory / "reply.aiff"
        wav = directory / "reply.wav"
        source.write_text(text, encoding="utf-8")
        # File input preserves arbitrary punctuation as text, never shell syntax.
        # 20 s allows local speech initialization; each process is reaped on timeout.
        subprocess.run(
            ["/usr/bin/say", "-v", "Tingting", "-r", "190", "-f", str(source), "-o", str(aiff)],
            check=True, timeout=20, capture_output=True,
        )
        subprocess.run(
            ["/usr/bin/afconvert", "-f", "WAVE", "-d", "LEI16@24000", "-c", "1", str(aiff), str(wav)],
            check=True, timeout=20, capture_output=True,
        )
        return wav.read_bytes()


class Handler(SimpleHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/api/tts":
            self.send_error(404)
            return
        if self.headers.get_content_type() != "application/json":
            self.send_error(415, "JSON required")
            return
        origin = self.headers.get("Origin")
        if origin and origin != f"http://{self.headers.get('Host')}":
            self.send_error(403)
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            # 4096 bytes covers 500 Chinese characters plus JSON escaping.
            if not 0 < length <= 4096:
                raise ValueError("invalid body length")
            data = json.loads(self.rfile.read(length))
            text = data.get("text") if isinstance(data, dict) else None
            if not isinstance(text, str) or not 0 < len(text.strip()) <= 500:
                raise ValueError("invalid speech text")
            audio = synthesize(text.strip())
        except (ValueError, UnicodeError):
            self.send_error(400, "Invalid speech request")
            return
        except (OSError, subprocess.SubprocessError):
            self.send_error(503, "Local speech unavailable")
            return
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(audio)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        try:
            self.wfile.write(audio)
        except (BrokenPipeError, ConnectionResetError):
            pass  # The user interrupted a pending request.


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8765)
    arguments = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", arguments.port), partial(Handler, directory=str(ROOT)))
    print(f"Norma prototype: http://127.0.0.1:{arguments.port}/", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
