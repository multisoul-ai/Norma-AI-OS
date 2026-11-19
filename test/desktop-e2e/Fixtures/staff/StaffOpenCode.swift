import Foundation

enum StaffOpenCode {
    static let script = #"""
    #!/usr/bin/env python3
    import base64, json, os, re, sys, threading, time, tty
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
    from urllib.parse import parse_qs, urlparse
    from urllib.request import Request, urlopen

    def record(name, value):
        root = os.environ.get("NORMA_BRAIN_DIR")
        if not root: return
        with open(os.path.join(root, name), "a") as handle:
            handle.write(json.dumps(value) + "\n")

    def authorized(headers):
        password = os.environ.get("OPENCODE_SERVER_PASSWORD", "")
        expected = "Basic " + base64.b64encode(f"opencode:{password}".encode()).decode()
        return headers.get("Authorization") == expected

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_): return
        def respond(self, payload):
            body = json.dumps(payload).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        def do_GET(self):
            if not authorized(self.headers):
                self.send_response(401); self.end_headers(); return
            parsed = urlparse(self.path)
            if parsed.path == "/global/health":
                self.respond({"healthy": True, "version": "1.19.3"})
            elif parsed.path == "/question":
                self.respond([])
            else:
                self.send_response(404); self.end_headers()
        def do_POST(self):
            if not authorized(self.headers):
                self.send_response(401); self.end_headers(); return
            parsed = urlparse(self.path)
            directory = parse_qs(parsed.query).get("directory", [""])[0]
            if parsed.path == "/session":
                self.respond({"id": "ses_e2estaff", "directory": directory})
            else:
                self.send_response(404); self.end_headers()

    def serve():
        port = int(sys.argv[sys.argv.index("--port") + 1])
        ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()

    def route_from_config():
        config = json.load(open(os.environ["OPENCODE_TUI_CONFIG"]))
        plugin = config["plugin"][0]
        if plugin.startswith("file://"): plugin = plugin[7:]
        source = open(plugin).read()
        nonce = json.loads(re.search(r"const nonce = (.*?);", source).group(1))
        path = json.loads(re.search(r"const path = (.*?);", source).group(1))
        return nonce, path

    def publish(path, nonce, session):
        while True:
            payload = {"nonce": nonce, "revision": 1, "session": session, "updatedAt": int(time.time() * 1000)}
            temporary = path + ".pending"
            with open(temporary, "w") as handle: json.dump(payload, handle)
            os.replace(temporary, path)
            time.sleep(0.2)

    def post_hook(event, prompt):
        endpoint = os.environ.get("NORMA_HOOK_ENDPOINT")
        token = os.environ.get("NORMA_HOOK_TOKEN")
        if not endpoint or not token: return
        raw = {
            "hook_event_name": event,
            "session_id": "ses_e2estaff",
            "prompt": prompt,
        }
        if os.environ.get("NORMA_DISPATCH_ID"):
            raw["_norma_dispatch_id"] = os.environ["NORMA_DISPATCH_ID"]
        if os.environ.get("NORMA_CONTROL_RUN_ID"):
            raw["_norma_control_run_id"] = os.environ["NORMA_CONTROL_RUN_ID"]
        body = {
            "node_id": int(os.environ["NORMA_NODE_ID"]),
            "profile": "OpenCode",
            "provider_event": event,
            "session_id": "ses_e2estaff",
            "turn_id": "staff-turn",
            "tool_name": None,
            "message": None,
            "raw_json": raw,
        }
        if os.environ.get("NORMA_CONTROL_RUN_ID"):
            body["control_run_id"] = os.environ["NORMA_CONTROL_RUN_ID"]
        data = json.dumps(body).encode()
        request = Request(endpoint, data=data, method="POST")
        request.add_header("Authorization", "Bearer " + token)
        request.add_header("Content-Type", "application/json")
        request.add_header("Content-Length", str(len(data)))
        urlopen(request, timeout=3).read()
        record("hooks.jsonl", {"event": event, "node": body["node_id"], "dispatch": os.environ.get("NORMA_DISPATCH_ID")})

    def attach():
        tty.setraw(sys.stdin.fileno())
        session = "ses_e2estaff"
        if "--session" in sys.argv:
            session = sys.argv[sys.argv.index("--session") + 1]
        record("starts.jsonl", {"node": int(os.environ.get("NORMA_NODE_ID", "0")), "dispatch": os.environ.get("NORMA_DISPATCH_ID"), "session_start": False})
        sys.stdout.buffer.write(b"\x1b[?2004hOPENCODE READY\r")
        sys.stdout.buffer.flush()
        nonce, path = route_from_config()
        threading.Thread(target=publish, args=(path, nonce, session), daemon=True).start()
        buffer, pastes = b"", 0
        while True:
            data = os.read(0, 4096)
            if not data: break
            buffer += data
            if b"\x1b[201~" in buffer and buffer.endswith(b"\r"):
                task = buffer.replace(b"\x1b[200~", b"").replace(b"\x1b[201~", b"").rstrip(b"\r").decode("utf-8", "replace")
                buffer, pastes = b"", pastes + 1
                record("received.jsonl", {"node": int(os.environ["NORMA_NODE_ID"]), "dispatch": os.environ.get("NORMA_DISPATCH_ID"), "task": task, "paste": pastes})
                if pastes == 1: continue
                post_hook("UserPromptSubmit", task)
                with open("STAFF-RESULT.md", "w") as handle:
                    handle.write("# STAFF RESULT\n\nOpenCode accepted the prompt.\n")

    if "--version" in sys.argv:
        print("1.19.3")
    elif "serve" in sys.argv:
        serve()
    elif "attach" in sys.argv:
        attach()
    """#
}
