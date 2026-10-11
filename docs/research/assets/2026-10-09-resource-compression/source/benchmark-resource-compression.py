#!/usr/bin/env python3
"""串行执行 CLI 无损压缩实验，保存原始指标并逐成员验证还原。"""

import argparse
import csv
import hashlib
import json
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import signal
import statistics
import subprocess
import sys
import tarfile
import tempfile
import time
from datetime import datetime, timezone


ROOTS = ("crates/app/assets/wallpapers", "crates/app/assets/feature-guide")
CASES = {
    "store": ("cp", "tar"),
    "gzip-9": ("gzip", "gz"),
    "bzip2-9": ("bzip2", "bz2"),
    "xz-9e": ("xz", "xz"),
    "zstd-3": ("zstd", "zst"),
    "zstd-19": ("zstd", "zst"),
    "zstd-22": ("zstd", "zst"),
    "brotli-6": ("brotli", "br"),
    "brotli-11": ("brotli", "br"),
    "7z-lzma2-9": ("7zz", "7z"),
    "7z-ppmd-9": ("7zz", "7z"),
    "zpaq-5": ("zpaq", "zpaq"),
}


def sha256(path):
    with Path(path).open("rb") as stream:
        return hash_stream(stream)


def hash_stream(stream):
    digest = hashlib.sha256()
    for block in iter(lambda: stream.read(1024 * 1024), b""):
        digest.update(block)
    return digest.hexdigest()


def savings_percent(source_bytes, compressed_bytes):
    if source_bytes <= 0:
        raise ValueError("数据集必须包含非空文件")
    return 100 * (1 - compressed_bytes / source_bytes)


def inventory(root):
    manifest = []
    for relative in ROOTS:
        directory = root / relative
        if not directory.is_dir() or directory.is_symlink():
            raise ValueError("资源目录缺失或为符号链接：" + str(directory))
        for path in directory.rglob("*"):
            if path.is_symlink():
                raise ValueError("资源不得包含符号链接：" + str(path))
            if path.is_file():
                manifest.append({"path": path.relative_to(root).as_posix(),
                                 "bytes": path.stat().st_size, "sha256": sha256(path)})
    return sorted(manifest, key=lambda item: item["path"])


def datasets(manifest):
    return {
        "video": [item for item in manifest if Path(item["path"]).suffix.lower() == ".mp4"],
        "images": [item for item in manifest if Path(item["path"]).suffix.lower()
                   in {".jpg", ".jpeg", ".png", ".webp", ".gif"}],
        "resources": manifest,
    }


def make_tar(root, manifest, target):
    with tarfile.open(target, "w", format=tarfile.USTAR_FORMAT) as archive:
        for item in manifest:
            info = tarfile.TarInfo(item["path"])
            info.size = item["bytes"]
            info.mode = 0o644
            info.mtime = 0
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            with (root / item["path"]).open("rb") as stream:
                archive.addfile(info, stream)


def verify_tar(path, manifest):
    expected = {item["path"]: item for item in manifest}
    if len(expected) != len(manifest):
        raise ValueError("源清单包含重复路径")
    seen = set()
    with tarfile.open(path, "r:") as archive:
        for member in archive:
            name = member.name
            parts = PurePosixPath(name)
            if (not member.isfile() or parts.is_absolute() or ".." in parts.parts
                    or name in seen or name not in expected):
                raise ValueError("非法、重复或额外成员：" + name)
            item = expected[name]
            if member.size != item["bytes"]:
                raise ValueError("成员长度不一致：" + name)
            with archive.extractfile(member) as stream:
                if hash_stream(stream) != item["sha256"]:
                    raise ValueError("成员 SHA-256 不一致：" + name)
            seen.add(name)
    if seen != set(expected):
        raise ValueError("归档缺少成员：" + str(sorted(set(expected) - seen)))
    return len(seen)


def run_command(argv, cwd, log, timeout, stdout_path=None):
    log.parent.mkdir(parents=True, exist_ok=True)
    measured_argv = ["/usr/bin/time", "-l"] + argv if platform.system() == "Darwin" else argv
    started = time.perf_counter()
    with log.open("wb") as stderr:
        stdout = stdout_path.open("wb") if stdout_path else stderr
        try:
            process = subprocess.Popen(measured_argv, cwd=cwd, stdout=stdout, stderr=stderr,
                                       start_new_session=True)
            try:
                exit_code = process.wait(timeout=timeout)
                status = "ok" if exit_code == 0 else "error"
            except subprocess.TimeoutExpired:
                try:
                    os_kill_group(process.pid)
                except ProcessLookupError:
                    pass
                exit_code = process.wait()
                status = "timeout"
        finally:
            if stdout_path:
                stdout.close()
    seconds = time.perf_counter() - started
    output = log.read_text(errors="replace")
    rss = re.search(r"(\d+)\s+maximum resident set size", output)
    cpu = re.search(r"([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys", output)
    return {"status": status, "exit_code": exit_code, "seconds": seconds,
            "max_rss_bytes": int(rss.group(1)) if rss else None,
            "user_seconds": float(cpu.group(2)) if cpu else None,
            "system_seconds": float(cpu.group(3)) if cpu else None,
            "argv": argv, "cwd": str(cwd), "log": str(log)}


def os_kill_group(pid):
    import os
    os.killpg(pid, signal.SIGKILL)


def commands(case, executable, source, compressed, restored):
    if case == "store":
        return [executable, str(source), str(compressed)], None, [executable, str(compressed), str(restored)], None
    if case == "gzip-9":
        return [executable, "-9", "-n", "-c", str(source)], compressed, [executable, "-dc", str(compressed)], restored
    if case == "bzip2-9":
        return [executable, "-9", "-c", str(source)], compressed, [executable, "-dc", str(compressed)], restored
    if case == "xz-9e":
        return [executable, "-9e", "-T1", "-c", str(source)], compressed, [executable, "-dc", str(compressed)], restored
    if case.startswith("zstd-"):
        level = case.split("-")[1]
        encoder = [executable, "--single-thread", "-q", "-" + level, "-c", str(source)]
        if level == "22":
            encoder.insert(1, "--ultra")
        return encoder, compressed, [executable, "-q", "-dc", str(compressed)], restored
    if case.startswith("brotli-"):
        level = case.split("-")[1]
        return [executable, "-q", level, "-c", str(source)], compressed, [executable, "-d", "-c", str(compressed)], restored
    if case.startswith("7z-"):
        method = "LZMA2" if case == "7z-lzma2-9" else "PPMd"
        encoder = [executable, "a", "-t7z", "-m0=" + method, "-mx=9", "-mmt=1",
                   "-y", "-bd", "-bb0", str(compressed), source.name]
        return encoder, None, [executable, "x", "-so", "-y", str(compressed)], restored
    if case == "zpaq-5":
        return [executable, "add", str(compressed), source.name, "-method", "5", "-threads", "1"], None, [executable, "extract", str(compressed), "-to", str(restored.parent), "-threads", "1"], None
    raise ValueError("未知实验组合：" + case)


def attempt(case, group, source, manifest, directory, logs, timeout, iteration):
    executable_name, suffix = CASES[case]
    executable = shutil.which(executable_name)
    result = {"algorithm": case, "dataset": group, "iteration": iteration,
              "status": "unavailable", "verified_files": 0}
    if executable is None:
        result["reason"] = "找不到 CLI：" + executable_name
        return result
    compressed = directory / ("compressed." + suffix)
    restore_directory = directory / "restored"
    restore_directory.mkdir()
    restored = restore_directory / "payload.tar"
    encoder, encoder_stdout, decoder, decoder_stdout = commands(case, executable, source, compressed, restored)
    result["compress"] = run_command(encoder, source.parent, logs / "compress.log", timeout, encoder_stdout)
    if result["compress"]["status"] != "ok":
        result["status"] = result["compress"]["status"]
        result["reason"] = "压缩未完成"
        return result
    result["compressed_bytes"] = compressed.stat().st_size
    result["compressed_sha256"] = sha256(compressed)
    verify_started = time.perf_counter()
    result["decompress"] = run_command(decoder, source.parent, logs / "decompress.log", timeout, decoder_stdout)
    if result["decompress"]["status"] != "ok":
        result["status"] = result["decompress"]["status"]
        result["reason"] = "解压未完成"
        return result
    try:
        if sha256(restored) != sha256(source):
            raise ValueError("解压后的 tar SHA-256 与统一输入不一致")
        result["verified_files"] = verify_tar(restored, manifest)
        result["status"] = "ok"
        result["decompress_and_verify_seconds"] = time.perf_counter() - verify_started
    except (ValueError, OSError, tarfile.TarError) as error:
        result["status"] = "invalid"
        result["reason"] = str(error)
    return result


def summarize(samples, source_bytes):
    good = [row for row in samples if row["status"] == "ok"]
    if not good:
        return None
    sizes = [row["compressed_bytes"] for row in good]
    result = {"successful_runs": len(good), "source_bytes": source_bytes,
              "compressed_bytes": statistics.median(sizes),
              "compressed_min_bytes": min(sizes), "compressed_max_bytes": max(sizes),
              "saving_percent": savings_percent(source_bytes, statistics.median(sizes)),
              "meets_50_percent": max(sizes) * 2 <= source_bytes}
    for phase in ["compress", "decompress"]:
        times = [row[phase]["seconds"] for row in good]
        memory = [row[phase]["max_rss_bytes"] for row in good
                  if row[phase]["max_rss_bytes"] is not None]
        result[phase + "_seconds"] = statistics.median(times)
        result[phase + "_min_seconds"] = min(times)
        result[phase + "_max_seconds"] = max(times)
        result[phase + "_max_rss_bytes"] = max(memory) if memory else None
        result[phase + "_mb_per_second"] = source_bytes / 1e6 / statistics.median(times)
    return result


def capture(argv, cwd=None):
    try:
        result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=10)
        return (result.stdout + result.stderr).strip()[:2000]
    except (OSError, subprocess.TimeoutExpired) as error:
        return str(error)


def environment(root):
    version_args = {"cp": [], "7zz": [], "zpaq": [], "bzip2": ["--help"]}
    tools = {}
    for executable in sorted({values[0] for values in CASES.values()}):
        path = shutil.which(executable)
        tools[executable] = {"path": path, "version": capture([path] + version_args.get(executable, ["--version"])) if path else None}
    return {"time_utc": datetime.now(timezone.utc).isoformat(),
            "platform": platform.platform(), "python": sys.version,
            "cpu": capture(["sysctl", "-n", "machdep.cpu.brand_string"]),
            "memory_bytes": capture(["sysctl", "-n", "hw.memsize"]),
            "cpu_count": capture(["sysctl", "-n", "hw.ncpu"]),
            "git_revision": capture(["git", "rev-parse", "HEAD"], root),
            "git_status": capture(["git", "status", "--short"], root), "tools": tools}


def save(data, output):
    target = output / "results.json"
    temporary = target.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(target)


def write_summary(data, output):
    rows = []
    for group, info in data["datasets"].items():
        for case in data["configuration"]["algorithms"]:
            samples = [row for row in data["results"] if row["dataset"] == group and row["algorithm"] == case]
            summary = summarize(samples, info["source_bytes"])
            row = {"dataset": group, "algorithm": case,
                   "attempted_runs": len(samples),
                   "all_attempts_verified": bool(samples) and all(sample["status"] == "ok" for sample in samples)}
            if summary:
                row.update(summary)
                row["tar_saving_percent"] = savings_percent(info["tar_bytes"], summary["compressed_bytes"])
            else:
                row["status"] = ",".join(sorted({sample["status"] for sample in samples})) or "not_run"
            rows.append(row)
    (output / "summary.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2) + "\n")
    fields = list(dict.fromkeys(key for row in rows for key in row))
    with (output / "summary.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=300)
    parser.add_argument("--algorithms", default=",".join(CASES))
    parser.add_argument("--datasets", default="video,images,resources")
    args = parser.parse_args()
    selected = args.algorithms.split(",")
    selected_groups = args.datasets.split(",")
    if args.repeats < 1 or args.timeout <= 0 or any(case not in CASES for case in selected):
        parser.error("轮数、超时或算法不合法")
    if any(group not in {"video", "images", "resources"} for group in selected_groups):
        parser.error("数据集不合法")
    root, output = args.root.resolve(), args.output.resolve()
    if (output / "results.json").exists():
        parser.error("输出目录已有实验结果，请选择新目录")
    for relative in ROOTS:
        if output == root / relative or root / relative in output.parents:
            parser.error("输出目录不得位于输入资源目录内")
    output.mkdir(parents=True, exist_ok=True)
    manifest = inventory(root)
    data = {"environment": environment(root), "manifest": manifest,
            "configuration": {"algorithms": selected, "repeats": args.repeats,
                              "timeout_seconds": args.timeout, "threads": 1},
            "datasets": {}, "results": []}
    groups = datasets(manifest)
    with tempfile.TemporaryDirectory(prefix="payloads-", dir=output) as temporary:
        work = Path(temporary)
        inputs = {}
        for group in selected_groups:
            items = groups[group]
            if not items:
                continue
            directory = work / group
            directory.mkdir()
            source = directory / "payload.tar"
            make_tar(root, items, source)
            verify_tar(source, items)
            inputs[group] = source
            data["datasets"][group] = {"file_count": len(items),
                                       "source_bytes": sum(item["bytes"] for item in items),
                                       "tar_bytes": source.stat().st_size, "tar_sha256": sha256(source)}
        save(data, output)
        failed = set()
        for iteration in range(1, args.repeats + 1):
            rotated = selected[iteration - 1:] + selected[:iteration - 1]
            for group, source in inputs.items():
                for case in rotated:
                    if (group, case) in failed:
                        continue
                    print(f"[{group}] {case} {iteration}/{args.repeats} 开始", flush=True)
                    logs = output / "logs" / group / case / str(iteration)
                    with tempfile.TemporaryDirectory(prefix=case + "-", dir=work) as run:
                        result = attempt(case, group, source, groups[group], Path(run), logs, args.timeout, iteration)
                    data["results"].append(result)
                    save(data, output)
                    if result["status"] != "ok":
                        failed.add((group, case))
                    size = result.get("compressed_bytes", "n/a")
                    print(f"[{group}] {case} {iteration}/{args.repeats} {result['status']} bytes={size}", flush=True)
    data["source_unchanged"] = inventory(root) == manifest
    data["finished_utc"] = datetime.now(timezone.utc).isoformat()
    save(data, output)
    write_summary(data, output)
    if not data["source_unchanged"]:
        print("原文件在实验期间发生变化，结果不可推荐", file=sys.stderr)
        return 1
    if any(row["status"] in {"invalid", "error"} for row in data["results"]):
        return 1
    print("实验完成：" + str(output / "summary.csv"), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
