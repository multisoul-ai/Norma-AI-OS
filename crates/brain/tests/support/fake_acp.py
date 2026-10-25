#!/usr/bin/env python3
"""脚本化的假 ACP 适配器，仅用于 brain 的测试。

行为由 prompt 里的标记驱动：
  FAKE_RUN: <命令>   发起一次 session/request_permission，并把结果写进 $FAKE_LOG
  FAKE_SLEEP         处理这条 prompt 前先睡 0.6 秒
  FAKE_CRASH         立刻退出进程
每条 prompt 的原文也写进 $FAKE_LOG（行首 prompt:）。
"""
import json, os, re, sys, time

LOG = os.environ.get("FAKE_LOG", "/dev/null")
NEXT_ID = [1000]


def log(line):
    with open(LOG, "a", encoding="utf-8") as f:
        f.write(line.replace("\n", " ⏎ ") + "\n")


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def read():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)


def request_permission(session_id, command):
    NEXT_ID[0] += 1
    rid = NEXT_ID[0]
    send({
        "jsonrpc": "2.0", "id": rid, "method": "session/request_permission",
        "params": {
            "sessionId": session_id,
            "toolCall": {"toolCallId": "t1", "name": "Bash", "kind": "execute",
                         "rawInput": {"command": command}, "title": command},
            "options": [
                {"optionId": "allow-once", "name": "Yes", "kind": "allow_once"},
                {"optionId": "reject", "name": "No", "kind": "reject_once"},
            ],
        },
    })
    while True:
        msg = read()
        if msg.get("id") == rid:
            return msg["result"]["outcome"].get("optionId")


def main():
    session_id = "fake-session"
    while True:
        msg = read()
        method, mid = msg.get("method"), msg.get("id")
        if method == "initialize":
            send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1, "agentCapabilities": {}}})
        elif method == "session/new":
            log("session/new meta=" + json.dumps(msg["params"].get("_meta")))
            send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": session_id}})
        elif method == "session/set_mode":
            log("set_mode " + msg["params"]["modeId"])
            send({"jsonrpc": "2.0", "id": mid, "result": {}})
        elif method == "session/set_config_option":
            log("set_config " + msg["params"]["configId"] + "=" + msg["params"]["value"])
            send({"jsonrpc": "2.0", "id": mid, "result": {"configOptions": []}})
        elif method == "session/prompt":
            text = "".join(p.get("text", "") for p in msg["params"]["prompt"])
            log("prompt: " + text)
            if "FAKE_QUOTA" in text:
                # Captured from installed Claude ACP 0.94.0 consumer replay; no provider call.
                body = "You've hit your session limit · resets 6:30am (America/Los_Angeles)"
                send({"jsonrpc": "2.0", "method": "session/update", "params": {
                    "sessionId": session_id, "update": {"sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": body}}}})
                send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32603,
                    "message": "Internal error: " + body, "data": {"errorKind": "rate_limit"}}})
                continue
            if "FAKE_REMOTE_CANCEL" in text:
                send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "cancelled"}})
                continue
            if "FAKE_PARTIAL_SLEEP" in text:
                send({"jsonrpc": "2.0", "method": "session/update", "params": {
                    "sessionId": session_id, "update": {"sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": "取消前的部分输出"}}}})
                log("partial-output-sent")
                time.sleep(0.6)
            if "FAKE_CRASH" in text:
                sys.exit(3)
            if "FAKE_SLEEP" in text:
                time.sleep(0.6)
            for line in text.splitlines():
                if "FAKE_ENVELOPE: " in line:
                    command = {"normaShellCommand": line.split("FAKE_ENVELOPE: ", 1)[1]}
                    request_permission(session_id, command)
                if "FAKE_RUN: " in line:
                    command = line.split("FAKE_RUN: ", 1)[1]
                    choice = request_permission(session_id, command)
                    log(("allowed: " if choice == "allow-once" else "rejected: ") + command)
            # 审批提示里要求执行 decide 命令：模拟大脑照做
            found = re.search(r"([^\s`]+) brain decide (\S+) allow\|deny\|escalate", text)
            if found:
                command = f"{found.group(1)} brain decide {found.group(2)} allow --reason ok"
                choice = request_permission(session_id, command)
                log(("allowed: " if choice == "allow-once" else "rejected: ") + command)
            if "FAKE_TRACE" in text:
                def update(body):
                    send({"jsonrpc": "2.0", "method": "session/update",
                          "params": {"sessionId": session_id, "update": body}})
                update({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": "想想"}})
                update({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": "看"}})
                update({"sessionUpdate": "tool_call", "toolCallId": "t9", "title": "norma brain agents",
                        "kind": "execute", "status": "pending", "rawInput": {"command": "norma brain agents"}})
                update({"sessionUpdate": "tool_call_update", "toolCallId": "t9", "status": "completed",
                        "content": [{"type": "content", "content": {"type": "text", "text": "agents-ok"}}]})
                update({"sessionUpdate": "plan", "entries": [{"content": "汇报", "status": "pending"}]})
                update({"sessionUpdate": "brand_new_kind", "payload": 7})
                update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "先"}})
            send({"jsonrpc": "2.0", "method": "session/update", "params": {
                "sessionId": session_id,
                "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "收到"}}}})
            send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
        elif mid is not None:
            send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "unsupported"}})


main()
