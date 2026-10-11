#!/usr/bin/env python3
"""用固定模拟载荷验证资源预压缩对 DMG 的实际收益；不签名、不发布。"""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import shutil
import tempfile
from datetime import datetime, timezone

spec = importlib.util.spec_from_file_location(
    "resource_compression_benchmark", Path(__file__).with_name("benchmark-resource-compression.py"))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)
APP = "Norma Compression Fixture.app"


def validate_output(root, output):
    for relative in bench.ROOTS:
        if output == root / relative or root / relative in output.parents:
            raise ValueError("DMG 实验输出不得位于原资源目录内")


def choose_algorithm(data):
    if not data.get("finished_utc") or not data.get("source_unchanged"):
        raise ValueError("基准实验必须已完成，且原资源未改变")
    candidates = []
    cases = sorted({row["algorithm"] for row in data["results"]})
    for case in cases:
        if case == "store":
            continue
        rows = [row for row in data["results"]
                if row["dataset"] == "resources" and row["algorithm"] == case]
        if len(rows) == data["configuration"]["repeats"] and all(row["status"] == "ok" for row in rows):
            candidates.append((max(row["compressed_bytes"] for row in rows), case))
    if not candidates:
        raise ValueError("没有完整还原通过的完整资源压缩方案")
    return min(candidates)[1]


def build_payload(root, manifest, program, target, compressed=None):
    contents = target / APP / "Contents"
    executable = contents / "MacOS/fixture"
    executable.parent.mkdir(parents=True)
    shutil.copyfile(program, executable)
    executable.chmod(0o755)
    info = {"CFBundleIdentifier": "com.norma.compression-fixture",
            "CFBundleName": "Norma Compression Fixture", "CFBundleExecutable": "fixture",
            "CFBundlePackageType": "APPL", "CFBundleVersion": "0"}
    (contents / "Info.plist").write_bytes(plistlib.dumps(info, sort_keys=True))
    resources = contents / "Resources"
    resources.mkdir()
    if compressed:
        shutil.copyfile(compressed, resources / compressed.name)
    else:
        for item in manifest:
            relative = Path(item["path"]).relative_to("crates/app/assets")
            destination = resources / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / item["path"], destination)


def tree_manifest(root):
    result = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError("模拟载荷不应包含符号链接：" + str(path))
        if path.is_file():
            result[path.relative_to(root).as_posix()] = {
                "bytes": path.stat().st_size, "sha256": bench.sha256(path)}
    return result


def require_ok(measurement, label):
    if measurement["status"] != "ok":
        raise RuntimeError(label + "未完成，日志：" + measurement["log"])


def test_image(kind, payload_name, payload, work, output, source_tar, manifest,
               case, compressed_name, timeout):
    label = payload_name + "-" + kind
    image = work / (label + ".dmg")
    command = ["hdiutil", "create", "-quiet", "-srcfolder", str(payload),
               "-fs", "HFS+", "-volname", "Norma Compression Fixture",
               "-format", kind, "-nospotlight"]
    if kind == "UDZO":
        command += ["-imagekey", "zlib-level=9"]
    command += [str(image)]
    row = {"payload": payload_name, "format": kind,
           "create": bench.run_command(command, work, output / (label + "-create.log"), timeout)}
    require_ok(row["create"], "创建 DMG")
    row["bytes"] = image.stat().st_size
    row["sha256"] = bench.sha256(image)
    row["verify"] = bench.run_command(["hdiutil", "verify", "-quiet", str(image)], work,
                                      output / (label + "-verify.log"), timeout)
    require_ok(row["verify"], "DMG 完整性验证")
    mount = work / (label + "-mount")
    mount.mkdir()
    try:
        row["attach"] = bench.run_command(["hdiutil", "attach", "-readonly", "-nobrowse",
                                          "-mountpoint", str(mount), "-quiet", str(image)],
                                         work, output / (label + "-attach.log"), timeout)
        require_ok(row["attach"], "只读挂载")
        row["payload_sha256_matches"] = tree_manifest(mount / APP) == tree_manifest(payload / APP)
        if not row["payload_sha256_matches"]:
            raise ValueError("挂载后的完整模拟载荷与源载荷不一致")
        if payload_name == "compressed":
            restored_dir = work / (label + "-restored")
            restored_dir.mkdir()
            restored = restored_dir / "payload.tar"
            mounted_archive = mount / APP / "Contents/Resources" / compressed_name
            executable = shutil.which(bench.CASES[case][0])
            _, _, decoder, decoder_stdout = bench.commands(case, executable, source_tar, mounted_archive, restored)
            row["decompress"] = bench.run_command(decoder, work,
                                                  output / (label + "-decompress.log"),
                                                  timeout, decoder_stdout)
            require_ok(row["decompress"], "从 DMG 中恢复资源")
            row["tar_sha256_matches"] = bench.sha256(restored) == bench.sha256(source_tar)
            if not row["tar_sha256_matches"]:
                raise ValueError("从 DMG 中解压的 tar 不一致")
            row["verified_resource_files"] = bench.verify_tar(restored, manifest)
            shutil.rmtree(restored_dir)
        else:
            row["verified_resource_files"] = len(manifest)
    finally:
        row["detach"] = bench.run_command(["hdiutil", "detach", "-quiet", str(mount)],
                                          work, output / (label + "-detach.log"), timeout)
        if row.get("attach", {}).get("status") == "ok" or os.path.ismount(mount):
            require_ok(row["detach"], "卸载实验 DMG")
    image.unlink()
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--program", type=Path, default=Path("target/release/norma-os"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=300)
    args = parser.parse_args()
    data = json.loads(args.results.read_text())
    case = choose_algorithm(data)
    root, program, output = args.root.resolve(), args.program.resolve(), args.output.resolve()
    validate_output(root, output)
    if (output / "results.json").exists():
        parser.error("输出目录已有对照结果，请使用新目录")
    if bench.inventory(root) != data["manifest"]:
        parser.error("当前资源与基准实验清单不一致")
    if not program.is_file():
        parser.error("模拟载荷程序文件不存在")
    output.mkdir(parents=True, exist_ok=True)
    evidence = {"kind": "synthetic_fixed_payload_not_release", "resource_algorithm": case,
                "program": {"source": str(program), "bytes": program.stat().st_size,
                            "sha256": bench.sha256(program),
                            "mtime_utc": datetime.fromtimestamp(program.stat().st_mtime, timezone.utc).isoformat()},
                "manifest": data["manifest"], "results": []}
    with tempfile.TemporaryDirectory(prefix="dmg-payloads-", dir=output) as temporary:
        work = Path(temporary)
        snapshot = work / "program.snapshot"
        shutil.copyfile(program, snapshot)
        if bench.sha256(snapshot) != evidence["program"]["sha256"]:
            raise ValueError("程序在快照期间发生变化")
        source = work / "payload.tar"
        bench.make_tar(root, data["manifest"], source)
        if bench.sha256(source) != data["datasets"]["resources"]["tar_sha256"]:
            raise ValueError("本次 DMG 的资源输入与基准 tar 不一致")
        compressed = work / ("resources.tar." + bench.CASES[case][1])
        restored = work / "restored/payload.tar"
        encoder, encoder_stdout, _, _ = bench.commands(
            case, shutil.which(bench.CASES[case][0]), source, compressed, restored)
        evidence["compress"] = bench.run_command(encoder, work, output / "resource-compress.log",
                                                 args.timeout, encoder_stdout)
        require_ok(evidence["compress"], "压缩资源")
        evidence["compressed_bytes"] = compressed.stat().st_size
        evidence["compressed_sha256"] = bench.sha256(compressed)
        payloads = {name: work / name for name in ["plain", "compressed"]}
        build_payload(root, data["manifest"], snapshot, payloads["plain"])
        build_payload(root, data["manifest"], snapshot, payloads["compressed"], compressed)
        bench.save(evidence, output)
        for kind in ["UDZO", "ULMO"]:
            for name, payload in payloads.items():
                print(kind, name, "开始", flush=True)
                row = test_image(kind, name, payload, work, output, source,
                                 data["manifest"], case, compressed.name, args.timeout)
                evidence["results"].append(row)
                bench.save(evidence, output)
                print(kind, name, "bytes", row["bytes"], "SHA-256 通过", flush=True)
    evidence["source_unchanged"] = bench.inventory(root) == data["manifest"]
    evidence["finished_utc"] = datetime.now(timezone.utc).isoformat()
    bench.save(evidence, output)
    if not evidence["source_unchanged"]:
        raise ValueError("原资源在 DMG 对照期间发生变化")


if __name__ == "__main__":
    main()
