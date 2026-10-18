#!/usr/bin/env python3
"""本地假 OpenAI Chat Completions 服务，仅用于探测 Kimi Code CLI 的会话与 hooks（无任何真实凭据）。
触发词（出现在最后一条 user 消息中）: TOOLBASH -> 先发一次 Bash 工具调用; SLOW -> 回复前延迟 6 秒;
BADTOOL -> 调用不存在的工具; 其余 -> 返回 "MOCK-OK"。最后一条消息是 tool 结果 -> 返回 "TOOL-DONE"。
"""
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        self.send_response(200); self.send_header("content-type", "application/json"); self.end_headers()
        self.wfile.write(json.dumps({"data": [{"id": "mock-model"}]}).encode())
    def do_POST(self):
        n = int(self.headers.get("content-length", 0)); body = json.loads(self.rfile.read(n) or b"{}")
        msgs = body.get("messages", [])
        with open(sys.argv[2], "a") as f: f.write(json.dumps({"path": self.path, "last": msgs[-1] if msgs else None}) + "\n")
        last = msgs[-1] if msgs else {}
        text = ""; tool = None
        c = last.get("content")
        if isinstance(c, list): c = " ".join(p.get("text", "") for p in c if isinstance(p, dict))
        c = c or ""
        if last.get("role") == "tool": text = "TOOL-DONE"
        elif "TOOLBASH" in c: tool = ("Bash", {"command": "echo hello-from-tool"})
        elif "BADTOOL" in c: tool = ("NoSuchTool", {})
        else: text = "MOCK-OK"
        if "SLOW" in c: time.sleep(6)
        if "FAILHTTP" in c:
            self.send_response(400); self.send_header("content-type", "application/json"); self.end_headers()
            self.wfile.write(json.dumps({"error": {"message": "mock failure", "type": "invalid_request_error"}}).encode()); return
        self.send_response(200); self.send_header("content-type", "text/event-stream"); self.end_headers()
        def ev(d): self.wfile.write(("data: " + json.dumps(d) + "\n\n").encode()); self.wfile.flush()
        base = {"id": "c1", "object": "chat.completion.chunk", "created": 0, "model": "mock-model"}
        if tool:
            ev({**base, "choices": [{"index": 0, "delta": {"role": "assistant", "tool_calls": [{"index": 0, "id": "call_1", "type": "function", "function": {"name": tool[0], "arguments": json.dumps(tool[1])}}]}, "finish_reason": None}]})
            ev({**base, "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]})
        else:
            ev({**base, "choices": [{"index": 0, "delta": {"role": "assistant", "content": text}, "finish_reason": None}]})
            ev({**base, "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]})
        ev({**base, "choices": [], "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
        self.wfile.write(b"data: [DONE]\n\n"); self.wfile.flush()

ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
