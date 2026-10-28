"""Bounded localhost model fixture; the unmodified OpenCode CLI owns all question behavior."""
import http.server
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
question = {"questions": [
    {"header": "Color", "question": "Color?", "options": [{"label": "Blue", "description": "First"}, {"label": "Green", "description": "Second"}]},
    {"header": "Platforms", "question": "Platforms?", "multiple": True, "options": [{"label": "macOS", "description": "Mac"}, {"label": "iOS", "description": "Phone"}, {"label": "Linux", "description": "Linux"}]},
    {"header": "Marker", "question": "Custom marker?", "options": [{"label": "One", "description": "First"}, {"label": "Two", "description": "Second"}]},
]}


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        size = int(self.headers.get("content-length", "0"))
        if size > 4 * 1024 * 1024:
            self.send_error(413)
            return
        body = json.loads(self.rfile.read(size))
        tools = [item.get("function", {}).get("name") for item in body.get("tools", [])]
        answers = [item for item in body.get("messages", []) if item.get("role") == "tool"]
        if answers:
            (root / "consumed.json").write_text(json.dumps(answers))
        if "question" in tools and not answers:
            delta = {"tool_calls": [{"index": 0, "id": "native_fixture_question", "type": "function", "function": {"name": "question", "arguments": json.dumps(question)}}]}
            reason = "tool_calls"
        else:
            delta = {"content": "NATIVE_FIXTURE_RECEIVED Green macOS Linux marker" if answers else "Native question test"}
            reason = "stop"
        self.send_response(200)
        if body.get("stream"):
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            for content, finish in [(delta, None), ({}, reason)]:
                message = {"id": "fixture", "object": "chat.completion.chunk", "created": 0, "model": "question", "choices": [{"index": 0, "delta": content, "finish_reason": finish}]}
                self.wfile.write(("data: " + json.dumps(message) + "\n\n").encode())
                self.wfile.flush()
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
