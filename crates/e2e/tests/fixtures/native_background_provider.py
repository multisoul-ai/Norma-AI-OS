"""Deterministic native TUI boundary: real PTY, no model/network or external commands."""
import os
from pathlib import Path
import tty

tty.setraw(0)
os.write(1, "\x1b[?2004hmodel: fixture\r\n› Ask Codex to do anything\r\n".encode())
Path("started").write_text("ready")
buffer = b""
while True:
    chunk = os.read(0, 8192)
    if not chunk:
        break
    os.write(1, chunk)
    buffer += chunk
    if b"\r" in buffer:
        prompt, buffer = buffer.split(b"\r", 1)
        prompt = prompt.replace(b"\x1b[200~", b"").replace(b"\x1b[201~", b"")
        Path("submitted.tmp").write_text(prompt.decode())
        Path("submitted.tmp").replace("submitted")
