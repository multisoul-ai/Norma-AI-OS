import json, os, threading, time, urllib.request

MAP = {  # Hermes 事件 -> Norma（Claude 风格）事件名
    "on_session_start": "SessionStart", "pre_llm_call": "UserPromptSubmit",
    "pre_tool_call": "PreToolUse", "post_tool_call": "PostToolUse",
    "on_human_input_request": "PermissionRequest", "on_human_input_resolved": "PostToolUse",
    "api_request_error": "ApiError", "on_session_end": "Stop", "on_session_finalize": "SessionEnd",
}
KEEP = ("tool_name", "session_id", "completed", "failed", "interrupted", "turn_exit_reason", "retryable",
        "status_code", "reason", "kind", "outcome", "status")

def _post(name, payload):
    ep, tok, node = (os.environ.get(k) for k in ("MULTISOUL_HOOK_ENDPOINT", "MULTISOUL_HOOK_TOKEN", "MULTISOUL_NODE_ID"))
    if not (ep and tok and node):
        return
    body = json.dumps(dict(payload, hook_event_name=name)).encode()
    req = urllib.request.Request(ep, body, {"Authorization": "Bearer " + tok, "Content-Type": "application/json"})
    try:
        urllib.request.urlopen(req, timeout=2).read()
    except Exception:
        pass

def register(ctx):
    threading.Thread(target=_post, args=("PluginLoaded", {"cwd": os.getcwd()}), daemon=True).start()
    def make(hermes_name, norma_name):
        def cb(**kw):
            payload = {k: kw[k] for k in KEEP if k in kw}
            payload["hermes_event"] = hermes_name
            threading.Thread(target=_post, args=(norma_name, payload), daemon=True).start()
        return cb
    for h, n in MAP.items():
        ctx.register_hook(h, make(h, n))
