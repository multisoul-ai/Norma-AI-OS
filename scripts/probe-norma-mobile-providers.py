#!/usr/bin/env python3
"""G1: exercise real provider protocols in a disposable workspace.

This is a live integration check, not an emulator or a production bridge.
It uses the CLI's existing login without reading or copying credentials.
"""

import argparse
import json
import os
from pathlib import Path
import queue
import shlex
import signal
import subprocess
import sys
import tempfile
import threading
import time


class JsonProcess:
    def __init__(self, command, cwd):
        self.stderr = tempfile.TemporaryFile(mode="w+")
        self.process = subprocess.Popen(
            command, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.stderr, text=True, bufsize=1, start_new_session=True,
        )
        self.incoming = queue.Queue()
        self.events = []
        self.next_id = 0
        self.reader = threading.Thread(target=self.read_stdout, daemon=True)
        self.reader.start()

    def read_stdout(self):
        try:
            for line in self.process.stdout:
                try:
                    self.incoming.put(json.loads(line))
                except json.JSONDecodeError:
                    self.incoming.put({"probe_error": "CLI emitted non-JSON stdout"})
        finally:
            self.incoming.put({"probe_error": "CLI stdout closed"})

    def send(self, message):
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def receive(self, deadline):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("provider protocol response timed out")
        try:
            message = self.incoming.get(timeout=remaining)
        except queue.Empty as error:
            raise TimeoutError("provider protocol response timed out") from error
        if "probe_error" in message:
            raise RuntimeError(message["probe_error"])
        self.events.append(message)
        label = message.get("method", message.get("type", "response"))
        if label not in ("item/agentMessage/delta", "stream_event", "response"):
            print(f"probe event: {label}", flush=True)
        return message

    def request(self, method, params):
        self.next_id += 1
        request_id = self.next_id
        self.send({"id": request_id, "method": method, "params": params})
        # A handshake / local query gets 30s for CLI startup, not a model turn.
        deadline = time.monotonic() + 30
        while True:
            message = self.receive(deadline)
            if message.get("id") == request_id and "method" not in message:
                if "error" in message:
                    raise RuntimeError(f"{method}: {message['error']}")
                return message["result"]

    def close(self):
        if self.process.poll() is None:
            os.killpg(self.process.pid, signal.SIGTERM)
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
                self.process.wait()
        self.process.stdin.close()
        self.process.stdout.close()
        self.stderr.close()


def matches_probe_command(actual, expected):
    if actual == expected:
        return True
    if isinstance(actual, str):
        try:
            actual = shlex.split(actual)
        except ValueError:
            return False
    return (
        isinstance(actual, list) and len(actual) == 3
        and actual[0] in ("/bin/zsh", "/bin/bash", "/bin/sh", "zsh", "bash", "sh")
        and actual[1] in ("-lc", "-c") and actual[2] == expected
    )


def check_codex(workspace, case, isolate_hooks=False):
    """A real Codex turn exposes streaming output and exact thread history.

    Data: one disposable directory, one new thread, one user message;
    the expected reply is the independent literal NORMA_IOS_PROBE_OK.
    Flow: initialize -> start a read-only thread -> send -> await completion
    -> read that exact thread, without resuming any existing user session.
    Expected: final reply and delta exist; turn error and unrelated tool calls
    do not exist. The 180s turn limit allows startup + model generation;
    expiry fails the check instead of being reported as successful completion.
    Cancellation interrupts the pending turn, waits for request resolution,
    then sends a stale allow; the cancelled command must still not execute.
    """
    version = subprocess.check_output(["codex", "--version"], text=True).strip()
    # Explicit opt-in for a disposable protocol experiment. Sandbox and
    # request-level approvals below remain enabled; user config is untouched.
    command_line = ["codex"]
    if isolate_hooks:
        command_line.extend(["--disable", "hooks"])
    peer = JsonProcess(command_line + ["app-server", "--stdio"], workspace)
    thread_id = None
    try:
        peer.request("initialize", {
            "clientInfo": {"name": "norma_ios_probe", "version": "0.1.0"},
        })
        peer.send({"method": "initialized", "params": {}})
        started = peer.request("thread/start", {
            "cwd": str(workspace), "sandbox": "read-only",
            "approvalPolicy": "untrusted" if case != "chat" else "on-request",
            "approvalsReviewer": "user",
        })
        thread_id = started["thread"]["id"]
        output_file = workspace / "permission-check.txt"
        command = f"printf NORMA_APPROVAL_OK > {shlex.quote(str(output_file))}"
        prompt = (
            "This is a protocol integration check. Do not use tools or read "
            "files. Reply with exactly NORMA_IOS_PROBE_OK and nothing else."
        ) if case == "chat" else (
            "This is an approval protocol integration check in a disposable directory. "
            f"Use exec_command once to execute exactly: {command}\n"
            "Keep the command unchanged. Request approval if necessary. Do not use "
            "other tools or request persistent permissions. If denied, do not retry "
            "or use another approach. After the result, reply briefly and stop."
        )
        turn = peer.request("turn/start", {
            "threadId": thread_id,
            "input": [{"type": "text", "text": prompt}],
        })
        turn_id = turn["turn"]["id"]
        deadline = time.monotonic() + 180
        deltas = []
        completed = None
        approvals = []
        resolved = []
        while completed is None or any(request not in resolved for request in approvals):
            event = peer.receive(deadline)
            method = event.get("method")
            params = event.get("params", {})
            if method == "error":
                raise RuntimeError(f"Codex event error: {params.get('error')}")
            if params.get("threadId") != thread_id:
                continue
            if method == "item/agentMessage/delta":
                deltas.append(params["delta"])
            elif method == "turn/completed" and params["turn"]["id"] == turn_id:
                completed = params["turn"]
            elif method == "serverRequest/resolved":
                resolved.append(params["requestId"])
                if case == "approval-cancel" and params["requestId"] in approvals:
                    peer.send({"id": params["requestId"], "result": {"decision": "accept"}})
            elif "id" in event and method:
                assert case != "chat", f"No-tool probe unexpectedly requested {method}"
                assert method == "item/commandExecution/requestApproval", f"Unsupported approval method: {method}"
                assert params["turnId"] == turn_id, "Approval must target this exact turn"
                actual_command = params.get("command")
                assert matches_probe_command(actual_command, command), f"Refusing an unrecognized probe command: {actual_command}"
                assert not approvals, "The probe must not retry a previously decided command"
                assert not output_file.exists(), "The file must not exist before approval"
                approvals.append(event["id"])
                if case == "approval-cancel":
                    peer.next_id += 1
                    peer.send({"id": peer.next_id, "method": "turn/interrupt", "params": {
                        "threadId": thread_id, "turnId": turn_id,
                    }})
                    continue
                peer.send({"id": event["id"], "result": {
                    "decision": "accept" if case == "approval-accept" else "decline",
                }})
                if case == "approval-duplicate":
                    peer.send({"id": event["id"], "result": {"decision": "accept"}})
        assert completed.get("error") is None, "Codex turn failed, not completed successfully"
        expected_status = "interrupted" if case == "approval-cancel" else "completed"
        assert completed["status"] == expected_status, f"Codex turn must finish with {expected_status} status"
        history = peer.request("thread/read", {"threadId": thread_id, "includeTurns": True})
        assert history["thread"]["id"] == thread_id, "History must belong to the probe thread"
        items = [item for item_turn in history["thread"]["turns"] for item in item_turn["items"]]
        replies = [item["text"] for item in items if item["type"] == "agentMessage"]
        if case == "chat":
            assert "NORMA_IOS_PROBE_OK" in replies, "Full history must contain the exact assistant reply"
            assert deltas, "At least one real assistant message delta must arrive"
            assert not any(item["type"] == "commandExecution" for item in items), "No-tool probe must not run shell commands"
        else:
            assert len(approvals) == 1, "A real command approval request must be observed"
            assert approvals[0] in resolved, "Provider must acknowledge resolution of the exact approval"
            if case == "approval-accept":
                assert output_file.read_text() == "NORMA_APPROVAL_OK", "Approved command must create the expected file"
            else:
                assert not output_file.exists(), "Declined command must not create the file"
        return {
            "provider": "codex", "version": version, "case": case, "status": "passed",
            "external_hooks": "isolated_for_probe" if isolate_hooks else "user_configuration",
            "transport": "app-server stdio", "thread_id": thread_id,
            "turn_id": turn_id, "delta_count": len(deltas),
            "history_item_count": len(items), "approval_request_ids": approvals,
            "resolved_request_ids": resolved,
            "cancelled_request_late_allow": "passed" if case == "approval-cancel" else "not_tested",
            "conflicting_duplicate_response": "passed" if case == "approval-duplicate" else "not_tested",
            "existing_pty_attachment": "not_tested",
        }
    except Exception as error:
        error.probe_evidence = {
            "thread_id": thread_id,
            "event_types": [event.get("method", event.get("type", "response")) for event in peer.events[-40:]],
        }
        raise
    finally:
        if thread_id and peer.process.poll() is None:
            try:
                peer.request("thread/archive", {"threadId": thread_id})
            except Exception:
                pass  # Cleanup cannot turn a failed protocol check into a pass.
        peer.close()


def check_claude(workspace, case):
    """A real Claude SDK stream exposes text and a specific permission decision.

    Data: one temporary workspace and one Write of NORMA_APPROVAL_OK.
    Flow: initialize control protocol -> send a user message -> observe text
    or can_use_tool -> return a one-shot decision -> await the final result.
    Expected: chat echoes the exact sentinel; allow writes the exact content;
    deny leaves the file absent. No unrelated tool call may be approved.
    Duplicate case sends deny then a conflicting allow for the same request;
    the already denied tool must not execute. This is one transport, not a
    claim that two independent desktop/mobile clients have been verified.
    Cancellation interrupts a pending permission, observes control_cancel_request,
    and sends a late allow; the cancelled Write must not create the file.
    Initialization gets 60s and a model turn 180s; both deadlines fail closed.
    """
    version = subprocess.check_output(["claude", "--version"], text=True).strip()
    output_file = workspace / "permission-check.txt"
    hook_log = workspace / "permission-hook.json"
    command = [
        "claude", "--print", "--input-format", "stream-json",
        "--output-format", "stream-json", "--verbose",
        "--include-partial-messages", "--replay-user-messages",
        "--permission-prompt-tool", "stdio", "--permission-mode", "manual",
        "--no-session-persistence", "--tools", "Write",
    ]
    if case.startswith("hook-"):
        hook = workspace / "permission_hook.py"
        hook.write_text(
            "import json, sys\nfrom pathlib import Path\n"
            "event = json.load(sys.stdin)\n"
            f"Path({str(hook_log)!r}).write_text(json.dumps(event))\n"
            "data = event.get('tool_input', {})\n"
            f"valid = event.get('tool_name') == 'Write' and data.get('file_path') == {str(output_file)!r} and data.get('content') == 'NORMA_APPROVAL_OK'\n"
            f"behavior = 'allow' if valid and {case == 'hook-accept'!r} else 'deny'\n"
            "decision = {'behavior': behavior}\n"
            "if behavior == 'deny': decision['message'] = 'Test write declined. Do not retry.'\n"
            "print(json.dumps({'hookSpecificOutput': {'hookEventName': 'PermissionRequest', 'decision': decision}}))\n"
        )
        settings = workspace / "hook-settings.json"
        settings.write_text(json.dumps({"hooks": {"PermissionRequest": [{
            "matcher": "Write", "hooks": [{"type": "command", "timeout": 5,
                "command": f"{shlex.quote(sys.executable)} {shlex.quote(str(hook))}"}],
        }]}}))
        command.extend(["--settings", str(settings)])
    peer = JsonProcess(command, workspace)
    try:
        peer.send({"type": "control_request", "request_id": "probe-initialize",
                   "request": {"subtype": "initialize", "hooks": None}})
        deadline = time.monotonic() + 60
        while True:
            event = peer.receive(deadline)
            if event.get("type") == "control_response":
                response = event["response"]
                if response["request_id"] == "probe-initialize":
                    assert response["subtype"] == "success", "Claude initialization must succeed"
                    break
        prompt = (
            "This is a protocol integration check. Do not use tools. "
            "Reply with exactly NORMA_IOS_PROBE_OK and nothing else."
        ) if case == "chat" else (
            "This is an approval protocol integration check. Use Write exactly once "
            f"to create {output_file} with the exact content NORMA_APPROVAL_OK "
            "without a newline. If denied, do not retry or use another approach. "
            "After the result, reply briefly and stop."
        )
        peer.send({"type": "user", "message": {"role": "user", "content": prompt},
                   "parent_tool_use_id": None, "session_id": ""})
        deadline = time.monotonic() + 180
        approvals = []
        deltas = []
        texts = []
        session_id = None
        result = None
        cancelled = []
        while result is None or (case == "approval-cancel" and not cancelled):
            event = peer.receive(deadline)
            kind = event.get("type")
            if event.get("session_id"):
                if session_id is not None:
                    assert event["session_id"] == session_id, "Claude stream must not change sessions"
                session_id = event["session_id"]
            if kind == "stream_event":
                delta = event.get("event", {}).get("delta", {})
                if delta.get("type") == "text_delta":
                    deltas.append(delta["text"])
            elif kind == "assistant":
                for block in event["message"]["content"]:
                    if block.get("type") == "text":
                        texts.append(block["text"])
            elif kind == "control_request":
                request = event["request"]
                assert case != "chat", "No-tool chat must not request a permission"
                assert not case.startswith("hook-"), "Hook did not decide; CLI fell back to SDK permission callback"
                assert request["subtype"] == "can_use_tool", "Only tool permission requests are expected"
                assert request["tool_name"] == "Write", "Only the probe Write may be decided"
                tool_input = request["input"]
                assert tool_input["file_path"] == str(output_file), "Write must target the disposable probe file"
                assert tool_input["content"] == "NORMA_APPROVAL_OK", "Write content must match the independent sentinel"
                assert not approvals, "Claude must not retry a previously decided Write"
                assert not output_file.exists(), "File must not exist before permission is decided"
                approvals.append(event["request_id"])
                if case == "approval-cancel":
                    peer.send({"type": "control_request", "request_id": "probe-interrupt",
                               "request": {"subtype": "interrupt"}})
                    continue
                decision = {"behavior": "allow", "updatedInput": tool_input} if case == "approval-accept" else {
                    "behavior": "deny", "message": "The user declined this test write. Do not retry.",
                }
                peer.send({"type": "control_response", "response": {
                    "subtype": "success", "request_id": event["request_id"], "response": decision,
                }})
                if case == "approval-duplicate":
                    peer.send({"type": "control_response", "response": {
                        "subtype": "success", "request_id": event["request_id"],
                        "response": {"behavior": "allow", "updatedInput": tool_input},
                    }})
            elif kind == "control_cancel_request":
                assert event["request_id"] in approvals, "Cancellation must target our pending tool request"
                cancelled.append(event["request_id"])
                peer.send({"type": "control_response", "response": {
                    "subtype": "success", "request_id": event["request_id"],
                    "response": {"behavior": "allow", "updatedInput": {
                        "file_path": str(output_file), "content": "NORMA_APPROVAL_OK",
                    }},
                }})
            elif kind == "result":
                result = event
        if case == "approval-cancel":
            assert cancelled == approvals, "Provider must cancel the exact pending permission request"
            assert any(event.get("type") == "control_response"
                       and event.get("response", {}).get("request_id") == "probe-interrupt"
                       and event["response"]["subtype"] == "success" for event in peer.events), "Provider must acknowledge our interrupt"
        else:
            assert not result.get("is_error"), f"Claude turn failed: {result.get('result', result.get('errors'))}"
            assert result["subtype"] == "success", "Claude must report a successful turn result"
        if case == "chat":
            assert "NORMA_IOS_PROBE_OK" in texts, "Complete assistant message must match the sentinel"
            assert deltas, "Claude must provide real text deltas"
            assert not output_file.exists(), "No-tool chat must not write files"
        else:
            if case.startswith("hook-"):
                assert hook_log.exists(), "Claude must invoke the configured PermissionRequest hook"
                observed = json.loads(hook_log.read_text())
                assert observed["session_id"] == session_id, "Hook must identify the same live session"
                assert observed["tool_input"]["file_path"] == str(output_file), "Hook must describe the exact target"
            else:
                assert len(approvals) == 1, "A real Claude tool permission request must be observed"
            if case in ("approval-accept", "hook-accept"):
                assert output_file.read_text() == "NORMA_APPROVAL_OK", "Allowed Write must create the exact content"
            else:
                assert not output_file.exists(), "Denied Write must not create the file"
        return {
            "provider": "claude", "version": version, "case": case, "status": "passed",
            "transport": "SDK stream-json stdio", "session_id": session_id,
            "delta_count": len(deltas), "approval_request_ids": approvals,
            "permission_request_hook": "passed" if case.startswith("hook-") else "not_tested",
            "conflicting_duplicate_response": "passed" if case == "approval-duplicate" else "not_tested",
            "cancelled_request_ids": cancelled,
            "cancelled_request_late_allow": "passed" if case == "approval-cancel" else "not_tested",
            "provider_result_subtype": result["subtype"],
            "provider_result_is_error": result.get("is_error", False),
            "existing_pty_attachment": "not_tested", "history_pagination": "not_tested",
        }
    finally:
        peer.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--provider", choices=["codex", "claude"], required=True)
    parser.add_argument("--case", choices=["chat", "approval-accept", "approval-deny", "approval-duplicate", "approval-cancel", "hook-accept", "hook-deny"], default="chat")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--isolate-codex-hooks", action="store_true",
                        help="Opt in to excluding external hooks in this Codex test process only")
    args = parser.parse_args()
    if args.provider == "codex" and args.case.startswith("hook-"):
        parser.error("Codex hook probes are not implemented yet")
    with tempfile.TemporaryDirectory(prefix="norma-ios-provider-") as directory:
        try:
            workspace = Path(directory).resolve()
            result = check_codex(workspace, args.case, args.isolate_codex_hooks) if args.provider == "codex" else check_claude(workspace, args.case)
        except Exception as error:
            result = {"provider": args.provider, "case": args.case, "status": "failed", "error": str(error)}
            result.update(getattr(error, "probe_evidence", {}))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False))
    if result["status"] != "passed":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
