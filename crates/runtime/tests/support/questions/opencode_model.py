"""Bounded localhost model fixture; the unmodified OpenCode CLI owns all question behavior."""
import http.server
import json
import sys
import time
from pathlib import Path

root = Path(sys.argv[1])
desktop_e2e = sys.argv[2:] == ["--desktop-e2e"]
desktop_prompt = "NATIVE_FIXTURE_COLOR"
desktop_question = {"questions": [{
    "header": "Color", "question": "Which fixture color should I use?",
    "options": [{"label": "Green", "description": "Use green"},
                {"label": "Blue", "description": "Use blue"}],
}]}
question = {"questions": [
    {"header": "Color", "question": "Color?", "options": [{"label": "Blue", "description": "First"}, {"label": "Green", "description": "Second"}]},
    {"header": "Platforms", "question": "Platforms?", "multiple": True, "options": [{"label": "macOS", "description": "Mac"}, {"label": "iOS", "description": "Phone"}, {"label": "Linux", "description": "Linux"}]},
    {"header": "Marker", "question": "Custom marker?", "options": [{"label": "One", "description": "First"}, {"label": "Two", "description": "Second"}]},
]}


def desktop_turn(tools, answers, messages):
    if "question" not in tools or not any(
        item.get("role") == "user" and desktop_prompt in json.dumps(item.get("content"))
        for item in messages
    ):
        return None
    if answers:
        (root / "consumed.tmp").write_text(json.dumps(answers))
        (root / "consumed.tmp").replace(root / "consumed.json")
        return "resume", "Color confirmed. ", {"content": "NATIVE_FIXTURE_DONE"}, "stop"
    tool = {"index": 0, "id": "native_fixture_question", "type": "function",
            "function": {"name": "question", "arguments": json.dumps(desktop_question)}}
    return "generate", "I will check the fixture color. ", {"tool_calls": [tool]}, "tool_calls"


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def chunk(self, delta, reason):
        message = {"id": "fixture", "object": "chat.completion.chunk", "created": 0, "model": "question", "choices": [{"index": 0, "delta": delta, "finish_reason": reason}]}
        self.wfile.write(("data: " + json.dumps(message) + "\n\n").encode())
        self.wfile.flush()

    def do_POST(self):
        size = int(self.headers.get("content-length", "0"))
        if size > 4 * 1024 * 1024:
            self.send_error(413)
            return
        body = json.loads(self.rfile.read(size))
        tools = [item.get("function", {}).get("name") for item in body.get("tools", [])]
        answers = [item for item in body.get("messages", []) if item.get("role") == "tool"]
        if answers and not desktop_e2e:
            (root / "consumed.json").write_text(json.dumps(answers))
        if "question" in tools and not answers:
            delta = {"tool_calls": [{"index": 0, "id": "native_fixture_question", "type": "function", "function": {"name": "question", "arguments": json.dumps(question)}}]}
            reason = "tool_calls"
        else:
            delta = {"content": "NATIVE_FIXTURE_RECEIVED Green macOS Linux marker" if answers else "Native question test"}
            reason = "stop"
        staged = desktop_turn(tools, answers, body.get("messages", [])) if desktop_e2e else None
        if desktop_e2e and not staged:
            delta, reason = {"content": "Fixture color"}, "stop"
        self.send_response(200)
        if body.get("stream"):
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            if staged:
                phase, prefix, delta, reason = staged
                self.chunk({"content": prefix}, None)
                (root / phase).touch()
                # Keep this real model stream open while the reviewer inspects the running state.
                deadline = time.monotonic() + 360
                while not (root / ("release-" + phase)).exists() and time.monotonic() < deadline:
                    time.sleep(0.05)
            self.chunk(delta, None)
            self.chunk({}, reason)
            self.wfile.write(b"data: [DONE]\n\n")
        else:
            message = {"id": "fixture", "object": "chat.completion", "created": 0, "model": "question", "choices": [{"index": 0, "message": {"role": "assistant", **delta}, "finish_reason": reason}], "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}}
            data = json.dumps(message).encode()
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
(root / "port").write_text(str(server.server_port))
server.serve_forever()
