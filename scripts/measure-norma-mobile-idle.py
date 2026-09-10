#!/usr/bin/env python3
"""Explicit isolated E2E PID only: macOS service-thread CPU and nettop TCP byte deltas."""
import csv
import ctypes
import io
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import time


class ThreadInfo(ctypes.Structure):
    # Darwin proc_threadinfo: two nanosecond counters, eight int32 fields, name[64].
    _fields_ = [("user", ctypes.c_uint64), ("system", ctypes.c_uint64),
                ("fields", ctypes.c_int32 * 8), ("name", ctypes.c_char * 64)]


def threads(pid):
    lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    lib.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64,
                                ctypes.c_void_p, ctypes.c_int]
    ids = (ctypes.c_uint64 * 4096)()  # Reject truncation; isolated fixture has only a few threads.
    count = lib.proc_pidinfo(pid, 6, 0, ids, ctypes.sizeof(ids))
    if not 0 < count < ctypes.sizeof(ids) or count % 8:
        raise RuntimeError(f"incomplete PROC_PIDLISTTHREADS: {count}")
    result = {}
    for identifier in ids[:count // 8]:
        info = ThreadInfo()
        # Flavor5 accepts LISTTHREADS handles; flavor15 takes a different ID namespace.
        size = lib.proc_pidinfo(pid, 5, identifier, ctypes.byref(info), ctypes.sizeof(info))
        if size != ctypes.sizeof(info):
            raise RuntimeError(f"thread {identifier} disappeared or unreadable: {size}")
        result[identifier] = {"name": info.name.decode(), "user_ns": info.user, "system_ns": info.system}
    return result


def network_samples(raw, pid):
    values = []
    columns = None
    for row in csv.reader(io.StringIO(raw)):
        if "bytes_in" in row and "bytes_out" in row:
            columns = row
        elif row and row[0].endswith(f".{pid}"):
            if columns is None:
                raise ValueError("nettop missing header")
            values.append({key: int(row[columns.index(key)]) for key in ("bytes_in", "bytes_out")})
    if len(values) != 2 or any(values[1][key] < values[0][key] for key in values[0]):
        raise ValueError("nettop needs two monotonic samples for the exact PID")
    return values


def service_threads(values):
    return {key: item for key, item in values.items()
            if item["name"].startswith("norma-mobile-") or item["name"] == "norma-agent-runtime"}


def sample(pid, directory):
    directory.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    before = threads(pid)
    selected = service_threads(before)
    names = [item["name"] for item in selected.values() if item["name"].startswith("norma-mobile-")]
    if sorted(names) != ["norma-mobile-listen", "norma-mobile-peer"]:
        raise RuntimeError(f"expected exactly one listener and one live watch peer: {names}")
    #6s covers three production2s heartbeat periods;5s is bounded nettop startup/teardown allowance.
    command = ["/usr/bin/nettop", "-n", "-x", "-m", "tcp", "-P", "-L", "2", "-s", "6",
               "-p", str(pid), "-J", "bytes_in,bytes_out"]
    with (directory / "nettop.csv").open("w") as output, (directory / "nettop.stderr").open("w") as error:
        subprocess.run(command, stdout=output, stderr=error, check=True, timeout=11)
    after = threads(pid)
    elapsed = time.monotonic() - started
    current = service_threads(after)
    if selected.keys() != current.keys():
        raise RuntimeError("service threads changed during measurement")
    measured = []
    for key, first in selected.items():
        last = current[key]
        if first["name"] != last["name"] or any(last[field] < first[field] for field in ("user_ns", "system_ns")):
            raise RuntimeError("thread identity changed or CPU counter regressed")
        cpu = sum(last[field] - first[field] for field in ("user_ns", "system_ns")) / 1e9
        measured.append({"thread_handle": key, "name": first["name"], "cpu_seconds": cpu,
                         "one_core_percent": cpu / elapsed * 100})
    samples = network_samples((directory / "nettop.csv").read_text(), pid)
    delta = {key: samples[1][key] - samples[0][key] for key in samples[0]}
    if delta["bytes_out"] <= 0:
        raise RuntimeError("idle watch produced no system-counted bytes")
    result = {"pid": pid, "wall_seconds": elapsed, "threads": measured,
              "all_threads_before": before, "all_threads_after": after,
              "server_network_thread_cpu_seconds": sum(item["cpu_seconds"] for item in measured if item["name"].startswith("norma-mobile-")),
              "shared_runtime_cpu_seconds": sum(item["cpu_seconds"] for item in measured if item["name"] == "norma-agent-runtime"),
              "server_transport_thread_cpu_seconds": sum(item["cpu_seconds"] for item in measured),
              "server_transport_one_core_percent": sum(item["one_core_percent"] for item in measured),
              "nettop_samples": samples, "nettop_delta": delta, "nettop_command": command,
              "scope": "libproc named listener/peer plus shared Tokio runtime threads; isolated fixture has no model work. Excludes synthetic fixture workers, GPUI and Provider. "
                       "nettop TCP per-process byte counters across two requested6s samples; "
                       "established encrypted WSS traffic, not JSON or complete TCP/IP wire size. "
                       "CPU window includes nettop startup/teardown; network window is not identical."}
    (directory / "server-idle.json").write_text(json.dumps(result, indent=2) + "\n")


def self_test():
    """场景：精确PID计数、缺失/回退拒绝，以及libproc纳秒CPU单位。

    数据构造：两样本4096-0=4096字节；其他PID9999字节不得混入；忙算0.1s对照getrusage。
    执行过程：解析实际nettop格式 → 拒绝不完整/回退 → 同进程前后CPU与rusage比较。
    预期结果：只取得指定PID的4096字节；无样本不算0；CPU单位不是微秒或整个墙钟。
    """
    # 场景：v102把socket等待交给共享运行时，不能漏算这些线程，也不能混入fixture。
    # 数据：listener/peer/runtime各1个；fixture为反例，不承担正式传输等待。
    # 执行：通过与sample相同的选择器取线程；预期保留三个正式线程，排除fixture。
    selected = service_threads({
        1: {"name": "norma-mobile-listen"}, 2: {"name": "norma-mobile-peer"},
        3: {"name": "norma-agent-runtime"}, 4: {"name": "fixture-worker"},
    })
    assert 1 in selected, "listener must enter transport CPU accounting"
    assert 2 in selected, "TLS peer must enter transport CPU accounting"
    assert 3 in selected, "shared readiness reactor must not be omitted from transport CPU"
    assert 4 not in selected, "synthetic fixture work must not masquerade as transport CPU"
    raw = ",bytes_in,bytes_out,\nPython.12,0,0,\nPython.13,9999,9999,\n,bytes_in,bytes_out,\nPython.12,4096,4096,\n"
    values = network_samples(raw, 12)
    assert values[1]["bytes_out"] == 4096, "requested PID's second sample must retain exact bytes"
    assert values[1]["bytes_out"] != 9999, "another PID must never enter target counters"
    for invalid in ("", raw.replace("Python.12,4096,4096,", ""), raw.replace("Python.12,0,0,", "Python.12,8192,8192,")):
        try:
            network_samples(invalid, 12)
        except ValueError:
            continue
        raise AssertionError("missing or regressing counters must fail rather than produce zero usage")
    assert ctypes.sizeof(ThreadInfo) == 112, "Darwin proc_threadinfo ABI must match the SDK"
    pid = os.getpid()
    first = threads(pid)
    usage = resource.getrusage(resource.RUSAGE_SELF)
    base = usage.ru_utime + usage.ru_stime
    until = time.monotonic() + 0.1  #100ms CPU >> syscall measurement noise; tolerance20ms below.
    while time.monotonic() < until:
        pass
    last = threads(pid)
    usage = resource.getrusage(resource.RUSAGE_SELF)
    consumed = usage.ru_utime + usage.ru_stime - base
    observed = sum(last[key][field] - first[key][field] for key in first for field in ("user_ns", "system_ns")) / 1e9
    assert observed > 0.05, "busy loop must produce measurable CPU, not a zero/unreadable counter"
    assert abs(observed - consumed) < 0.02, "nanosecond libproc delta must match independent getrusage seconds"
    print(json.dumps({"self_test": "passed", "libproc_cpu_seconds": observed, "getrusage_cpu_seconds": consumed}))


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    elif len(sys.argv) == 3 and int(sys.argv[1]) > 1:
        sample(int(sys.argv[1]), Path(sys.argv[2]))
    else:
        raise SystemExit("usage: measure-norma-mobile-idle.py <isolated-e2e-pid> <evidence-directory> | --self-test")
