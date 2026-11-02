#!/usr/bin/env python3
"""Collect bounded Windows feasibility observations, without credentials or live model calls."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import queue
import re
import shutil
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
TARGET = "x86_64-pc-windows-msvc"
COLLECTED = ("environment", "compile", "graphics", "agents", "speech")
EXIT_CHECKS = {
    "control": {"identity_tests": ("identity_exit", "identity.log"),
                "control_tests": ("control_exit", "control.log"),
                "runtime_control_tests": ("runtime_exit", "runtime.log"),
                "opencode_transport_tests": ("opencode_transport_exit", "opencode_transport.log"),
                "opencode_runtime_tests": ("opencode_runtime_exit", "opencode_runtime.log")},
    "platform": {"memory_platform_tests": ("memory_exit", "memory_platform.log"),
                 "review_platform_tests": ("review_exit", "review_platform.log"),
                 "a1_platform_tests": ("a1_exit", "a1_platform.log")},
    "app-adapters": {"app_adapter_tests": ("transport_exit", "transport.log")},
}
REQUIRED = (*COLLECTED, *EXIT_CHECKS)
STATUSES = {"PASS", "FAIL", "ENV_UNSUPPORTED", "NOT_RUN", "NOT_IN_SCOPE"}


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def source_sha():
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def terminate(process):
    if process.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    elif process.poll() is None:
        process.kill()
    process.wait(timeout=20)


def command(folder, name, args, timeout=120):
    started = time.monotonic()
    log = folder / f"{name}.log"
    result = {"id": name, "command": [str(a) for a in args], "log": log.name}
    try:
        with log.open("w", encoding="utf-8") as output:
            process = subprocess.Popen(args, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
            try:
                code = process.wait(timeout=timeout)
                result.update(status="PASS" if code == 0 else "FAIL", exit_code=code)
            except subprocess.TimeoutExpired:
                terminate(process)
                result.update(status="ENV_UNSUPPORTED", detail=f"Timeout after {timeout}s")
    except OSError as error:
        log.write_text(str(error), encoding="utf-8")
        result.update(status="NOT_RUN", detail=str(error))
    result["duration_seconds"] = round(time.monotonic() - started, 2)
    print(json.dumps(result, ensure_ascii=False), flush=True)
    return result


def native(folder, mode, executable=None):
    args = ["pwsh", "-NoLogo", "-NoProfile", "-File", str(ROOT / "scripts/windows/native.ps1"),
            "-Mode", mode, "-Output", str(folder)]
    if executable:
        args += ["-Executable", str(executable)]
    result = command(folder, "native", args, 90)
    path = folder / "native.json"
    if not path.exists():
        return [result], {}
    data = json.loads(path.read_text(encoding="utf-8-sig"))
    return [result, *data.pop("checks", [])], data


def environment(folder):
    checks = [command(folder, name, args) for name, args in [
        ("rustc", ["rustc", "-vV"]), ("cargo", ["cargo", "--version"]),
        ("node", ["node", "--version"]), ("git", ["git", "--version"]),
    ]]
    observations, metadata = native(folder, "environment")
    return checks + observations, metadata


def compile_probe(folder):
    checks = [command(folder, "canvas_tests", ["cargo", "test", "--locked", "-p", "canvas",
               "--lib", "--target", TARGET], 600)]
    for package in ("norma", "app"):
        args = ["cargo", "check", "--locked", "-p", package]
        if package == "norma":
            args.append("--lib")
        checks.append(command(folder, f"{package}_compile", [*args,
                      "--target", TARGET, "--message-format=short"], 1500 if package == "app" else 900))
        if package == "norma":
            checks.append(command(folder, "norma_cli_compile", ["cargo", "check", "--locked",
                          "-p", "norma", "--target", TARGET, "--message-format=short"], 900))
    checks.append(command(folder, "norma_platform_tests", [
        "cargo", "test", "--locked", "--no-fail-fast", "-p", "norma", "--lib",
        "--test", "client_platform", "--test", "codex_question", "--test", "cursor_questions",
        "--test", "client_tests", "--test", "protocol_tests", "--target", TARGET], 900))
    return checks, {"target": TARGET, "release_build": "NOT_RUN"}


def speech(folder):
    checks = [command(folder, "voice_input_compile", [
        "cargo", "check", "--locked", "-p", "voice_input", "--lib", "--target", TARGET,
        "--message-format=short"], 900)]
    checks.append(command(folder, "speech_tests", [
        "cargo", "test", "--locked", "--no-fail-fast", "-p", "voice_input", "-p", "asr", "-p", "voice_sherpa",
        "--target", TARGET], 1500))
    return checks, {"target": TARGET, "features": "default",
                    "scope": "Native backend compilation and default tests, not physical audio acceptance",
                    "microphone_and_real_model_acceptance": "NOT_RUN"}


def speech_check_status(rows):
    checks = {row["id"]: row for row in rows}
    if len(checks) != len(rows):
        return "FAIL"
    if set(checks) != {"voice_input_compile", "speech_tests"}:
        return "NOT_RUN"
    if any(row["status"] == "FAIL" or (row["status"] == "PASS" and row.get("exit_code") != 0)
           for row in rows):
        return "FAIL"
    if any(row["status"] == "ENV_UNSUPPORTED" for row in rows):
        return "ENV_UNSUPPORTED"
    return "PASS" if all(row["status"] == "PASS" for row in rows) else "NOT_RUN"


def native_check_status(task, rows):
    checks = {row["id"]: row for row in rows}
    if len(checks) != len(rows):
        return "FAIL"
    if set(checks) != set(EXIT_CHECKS[task]):
        return "NOT_RUN"
    for row in rows:
        if row["status"] == "FAIL" or (row["status"] == "PASS" and row.get("exit_code") != 0):
            return "FAIL"
        counts = row.get("counts", {})
        if row["status"] == "PASS":
            if not all(type(counts.get(k)) is int and counts[k] >= 0
                       for k in ("passed", "failed", "ignored")):
                return "FAIL"
            if counts["failed"] or counts["ignored"]:
                return "FAIL"
            if not counts["passed"]:
                return "NOT_RUN"
    if any(row["status"] == "ENV_UNSUPPORTED" for row in rows):
        return "ENV_UNSUPPORTED"
    return "PASS" if all(row["status"] == "PASS" for row in rows) else "NOT_RUN"


def record_exit_report(output, task):
    """Normalize captured native tests; never run the commands again or replace their exit codes."""
    folder = output / task
    path = folder / "report.json"
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
        checks = []
        for name, (field, filename) in EXIT_CHECKS[task].items():
            code = data.get(field)
            log = folder / filename
            rows = re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;",
                              log.read_text(encoding="utf-8-sig") if log.exists() else "")
            counts = {key: sum(int(row[i]) for row in rows)
                      for i, key in enumerate(("passed", "failed", "ignored"))}
            status = "NOT_RUN"
            if type(code) is int and code != 0:
                status = "FAIL"
            elif code == 0 and type(code) is int and rows:
                status = "FAIL" if counts["failed"] or counts["ignored"] else (
                    "PASS" if counts["passed"] else "NOT_RUN")
            checks.append({"id": name, "status": status, "exit_code": code,
                           "log": filename, "counts": counts})
        data.update(task=task, checks=checks)
        save(path, data)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Native report normalization FAIL: {error}")
        return 1
    return 0


def speech_gate(output):
    try:
        data = json.loads((output / "speech/report.json").read_text(encoding="utf-8-sig"))
        expected_sha = os.environ.get("GITHUB_SHA") or source_sha()
        if data["task"] != "speech" or data["source_sha"] != expected_sha:
            raise ValueError("speech report source identity does not match")
        for field, variable in (("run_id", "GITHUB_RUN_ID"), ("run_attempt", "GITHUB_RUN_ATTEMPT")):
            if os.environ.get(variable) and str(data.get(field)) != os.environ[variable]:
                raise ValueError("speech report belongs to another run or attempt")
        if speech_check_status(data["checks"]) != "PASS":
            raise ValueError("native speech compilation or tests did not pass")
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Speech gate FAIL: {error}")
        return 1
    print("Speech gate PASS: current captured checks succeeded; no commands were repeated")
    return 0


def graphics(folder):
    manifest = ROOT / "test/windows-probe/Cargo.toml"
    build = command(folder, "gpui_build", ["cargo", "build", "--locked", "--manifest-path",
                    str(manifest), "--target", TARGET], 1500)
    target = Path(os.environ.get("CARGO_TARGET_DIR", str(manifest.parent / "target")))
    exe = target / TARGET / "debug/norma-gpui-probe.exe"
    if build["status"] != "PASS" or not exe.exists():
        return [build, {"id": "gpui_window", "status": "NOT_RUN",
                        "detail": "GPUI example did not produce an executable; inspect build log"}], {}
    observations, metadata = native(folder, "graphics", exe)
    metadata["sample_sha256"] = hashlib.sha256(exe.read_bytes()).hexdigest()
    metadata["sample_source"] = "vendor/gpui-0.2.2/examples/hello_world.rs"
    metadata["scope"] = "Vendored GPUI example, not Norma or a release candidate"
    return [build, *observations], metadata


def bin_command(prefix, package, name):
    base = prefix / "node_modules" / package
    data = json.loads((base / "package.json").read_text(encoding="utf-8"))
    executable = base / (data["bin"][name] if isinstance(data["bin"], dict) else data["bin"])
    return (["node", str(executable)] if executable.suffix == ".js" else [str(executable)])


def codex_handshake(folder, cli):
    log = folder / "codex-initialize.log"
    process = None
    result = {"id": "codex_stdio_initialize", "status": "FAIL", "log": log.name}
    with log.open("w", encoding="utf-8") as errors:
        try:
            process = subprocess.Popen([*cli, "app-server", "--listen", "stdio://"],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors,
                                       text=True, encoding="utf-8")
            responses = queue.Queue()

            def read_lines():
                for line in process.stdout:
                    responses.put(line)

            threading.Thread(target=read_lines, daemon=True).start()
            request = {"id": 1, "method": "initialize", "params": {
                "clientInfo": {"name": "norma_windows_probe", "version": "0.0.1"}}}
            process.stdin.write(json.dumps(request) + "\n")
            process.stdin.flush()
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                try:
                    line = responses.get(timeout=1)
                except queue.Empty:
                    if process.poll() is not None:
                        break
                    continue
                errors.write(line)
                errors.flush()
                response = json.loads(line)
                if response.get("id") == 1:
                    result["status"] = "PASS" if "result" in response else "FAIL"
                    break
        except (OSError, ValueError) as error:
            result["detail"] = str(error)
        finally:
            if process is not None:
                terminate(process)
    return result


def agents(folder):
    prefix = Path(os.environ.get("RUNNER_TEMP", str(folder))) / "norma-probe-tools"
    npm = Path(shutil.which("node")).parent / "node_modules/npm/bin/npm-cli.js"
    packages = {"@openai/codex": ("0.166.0", "codex"),
                "@anthropic-ai/claude-code": ("2.1.316", "claude")}
    checks = [command(folder, "npm_install", ["node", str(npm), "install", "--prefix", str(prefix),
               "--no-audit", "--no-fund", *[f"{p}@{v[0]}" for p, v in packages.items()]], 300)]
    if checks[0]["status"] == "PASS":
        for package, (_, name) in packages.items():
            cli = bin_command(prefix, package, name)
            checks.append(command(folder, f"{name}_version", [*cli, "--version"]))
            checks.append(command(folder, f"{name}_help", [*cli, "--help"]))
            if name == "codex":
                checks.append(command(folder, "codex_server_help", [*cli, "app-server", "--help"]))
                checks.append(codex_handshake(folder, cli))
    checks += [{"id": name, "status": "NOT_RUN", "detail": detail} for name, detail in [
        ("live_agent_tasks", "No credentials or paid model calls are used; no real task was submitted"),
        ("native_codex_tui_relay", "stdio initialization does not validate Norma's Unix TUI relay"),
    ]]
    return checks, {"packages": packages, "credentials_supplied": False, "model_calls": 0}


def summary(output):
    checks, missing, shas = [], [], set()
    provenance_consistent = True
    for name in REQUIRED:
        path = output / name / "report.json"
        if not path.exists():
            missing.append(name)
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8-sig"))
            if data["task"] != name or not data["source_sha"] or not data["checks"]:
                raise ValueError("empty report or mismatched task identity")
            for field, variable in (("run_id", "GITHUB_RUN_ID"), ("run_attempt", "GITHUB_RUN_ATTEMPT")):
                if os.environ.get(variable) and str(data.get(field)) != os.environ[variable]:
                    provenance_consistent = False
                    raise ValueError(f"{field} does not belong to the current workflow attempt")
            shas.add(data["source_sha"])
            for row in data["checks"]:
                if row["status"] not in STATUSES:
                    raise ValueError("unknown status")
                checks.append({**row, "task": name})
        except (ValueError, KeyError, TypeError) as error:
            missing.append(name)
            checks.append({"id": name, "status": "FAIL", "detail": str(error), "task": name})
    by_task = {name: {row["id"]: row for row in checks if row["task"] == name} for name in REQUIRED}
    source_consistent = len(shas) == 1
    expected_sha = os.environ.get("GITHUB_SHA")
    if expected_sha and shas != {expected_sha}:
        source_consistent = False
    graphics_status = by_task["graphics"].get("gpui_window", {}).get("status", "NOT_RUN")
    if graphics_status == "PASS":
        graphics_status = by_task["graphics"].get("gpui_os_keyboard_close", {}).get("status", "NOT_RUN")
    speech_rows = [row for row in checks if row["task"] == "speech"]
    compile_rows = [by_task["compile"].get(name, {})
                    for name in ("norma_compile", "norma_cli_compile", "app_compile")]
    compile_states = ["FAIL" if row.get("status") == "FAIL" or (
        row.get("status") == "PASS" and row.get("exit_code") not in (None, 0)) else (
        "PASS" if row.get("status") == "PASS" and row.get("exit_code") == 0 else "NOT_RUN")
                      for row in compile_rows]
    goal_gates = {
        "product_compile": "FAIL" if "FAIL" in compile_states else (
            "PASS" if all(state == "PASS" for state in compile_states) else "NOT_RUN"),
        "gpui_graphics": graphics_status,
        "speech_default_checks": speech_check_status(speech_rows),
        "live_agents": "NOT_RUN", "candidate_package": "NOT_RUN",
    }
    for task, gate in (("control", "control_transport_checks"),
                       ("platform", "platform_boundary_checks"),
                       ("app-adapters", "app_adapter_checks")):
        goal_gates[gate] = native_check_status(task, [row for row in checks if row["task"] == task])
    goal_gates["evidence_integrity"] = "PASS" if (
        not missing and source_consistent and provenance_consistent) else "FAIL"
    data = {"kind": "feasibility_only", "source_shas": sorted(shas),
            "source_consistent": source_consistent, "provenance_consistent": provenance_consistent,
            "missing_reports": missing,
            "checks": checks, "goal_gates": goal_gates, "goal_complete": False}
    save(output / "feasibility.json", data)
    text = ["# Windows feasibility, not product acceptance", "", "| Gate | Status |", "|---|---|"]
    text += [f"| {name} | {status} |" for name, status in goal_gates.items()]
    text += ["", f"Missing reports: {missing}; consistent source: {source_consistent}; "
             f"current attempt: {provenance_consistent}",
             "", "Real model tasks and a product candidate were not executed."]
    (output / "summary.md").write_text("\n".join(text) + "\n", encoding="utf-8")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write("\n".join(text) + "\n")
    print(json.dumps(data, ensure_ascii=False, indent=2))
    return 1  # A feasibility run without real tasks/candidate is never the Goal's green acceptance gate.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("task", choices=[*COLLECTED, "summary", "speech-gate",
                                         *[f"record-{task}" for task in EXIT_CHECKS]])
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/windows-feasibility")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if args.task.startswith("record-"):
        return record_exit_report(output, args.task.removeprefix("record-"))
    if args.task == "speech-gate":
        return speech_gate(output)
    if args.task == "summary":
        return summary(output)
    if os.environ.get("RUNNER_TEMP"):
        os.environ["CARGO_TARGET_DIR"] = str(Path(os.environ["RUNNER_TEMP"]) / "norma-windows-probe-target")
    folder = output / args.task
    folder.mkdir(exist_ok=True)
    checks, metadata = globals()[args.task if args.task != "compile" else "compile_probe"](folder)
    save(folder / "report.json", {
        "task": args.task, "source_sha": source_sha(), "checks": checks, "metadata": metadata,
        "timestamp": datetime.now(timezone.utc).isoformat(), "os": platform.platform(),
        "run_id": os.environ.get("GITHUB_RUN_ID"), "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "image_version": os.environ.get("ImageVersion"),
        "disk_free_bytes": shutil.disk_usage(ROOT).free,
    })
    return 0  # Collection completed; failures remain in the report and fail the final gate.


if __name__ == "__main__":
    raise SystemExit(main())
