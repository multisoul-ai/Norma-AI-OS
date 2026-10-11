#!/usr/bin/env python3
"""串行实验 3/s/z × thin/fat，保存主程序及构建证据，不修改正式 profile。"""
import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "build/package-size-reduction/compiler-matrix"
MATRIX = [(level, lto) for level in ("3", "s", "z") for lto in ("thin", "fat")]
FEATURES = ["production-data"]


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def source_fingerprint(root, paths):
    value = hashlib.sha256()
    for relative in sorted(set(paths)):
        path = root / relative
        value.update(relative.encode() + b"\0")
        value.update(digest(path).encode() if path.is_file() else b"missing")
        value.update(b"\0")
    return value.hexdigest()


def build_context():
    def command(*args):
        return subprocess.check_output(args, cwd=ROOT, text=True).strip()
    listed = subprocess.check_output(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT)
    paths = listed.decode().split("\0")
    # Include build scripts, embedded assets, vendor code and untracked new Rust.
    # Documentation and experiment outputs do not affect compilation.
    paths = [path for path in paths if path in ("Cargo.toml", "Cargo.lock") or
        path.startswith(("crates/", "vendor/", "assets/", "packaging/", ".cargo/", "scripts/"))]
    keys = {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "CARGO_BUILD_TARGET", "CC", "CXX",
        "SDKROOT", "DEVELOPER_DIR",
        "MACOSX_DEPLOYMENT_TARGET", "SHERPA_ONNX_LIB_DIR"}
    keys.update(key for key in os.environ if key.startswith("CARGO_PROFILE_RELEASE_") and
        key not in ("CARGO_PROFILE_RELEASE_OPT_LEVEL", "CARGO_PROFILE_RELEASE_LTO"))
    sdk = Path(os.environ["SDKROOT"]).resolve() if os.environ.get("SDKROOT") else None
    settings = sdk / "SDKSettings.json" if sdk else None
    return {"schema_version": 1, "revision": command("git", "rev-parse", "HEAD"),
        "source_sha256": source_fingerprint(ROOT, paths), "input_file_count": len(set(paths)),
        "cargo_lock_sha256": digest(ROOT / "Cargo.lock"), "rustc": command("rustc", "-Vv"),
        "cargo": command("cargo", "-V"), "architecture": platform.machine(),
        "macOS": platform.mac_ver()[0], "features": FEATURES,
        "sdkroot_resolved": str(sdk) if sdk else None,
        "sdk_settings_sha256": digest(settings) if settings and settings.is_file() else None,
        "apple_clang": command("cc", "--version"), "apple_linker": command("xcrun", "ld", "-version_details"),
        "environment": {key: os.environ.get(key) for key in sorted(keys)},
        "native_library_note": "External Sherpa libraries reuse the recorded pinned directory; not included in source SHA"}


def guard_context(output, context):
    path = output / "context.json"
    if path.exists():
        if json.loads(path.read_text()) != context:
            raise ValueError("different build inputs; use a new experiment directory")
    elif (output / "results.json").exists():
        raise ValueError("missing build context for existing results; use a new experiment directory")
    else:
        path.write_text(json.dumps(context, indent=2) + "\n")


def cargo_executable(log, target):
    artifacts = set()
    with log.open() as stream:
        for line in stream:
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            info = message.get("target", {})
            if (message.get("reason") != "compiler-artifact" or info.get("name") != "norma-os"
                or "bin" not in info.get("kind", []) or message.get("profile", {}).get("test")
                or not message.get("executable")):
                continue
            path = Path(message["executable"]).resolve()
            if not path.is_relative_to(target.resolve()) or not path.is_file():
                raise ValueError("compiler-artifact executable is missing or outside experiment target")
            artifacts.add(path)
    if len(artifacts) != 1:
        raise ValueError("expected one norma-os compiler-artifact; refusing cached-path fallback")
    return artifacts.pop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--describe", action="store_true", help="只列出矩阵，不构建")
    parser.add_argument("--variant", choices=[f"{level}-{lto}" for level, lto in MATRIX])
    parser.add_argument("--output", type=Path, default=OUTPUT, help="不混用旧输入的新实验目录")
    args = parser.parse_args()
    if args.describe:
        print(json.dumps(MATRIX))
        return 0
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / "build"):
        raise ValueError("experiment output must remain under this worktree's build directory")
    output.mkdir(parents=True, exist_ok=True)
    context = build_context()
    guard_context(output, context)
    rows = []
    existing = output / "results.json"
    if existing.exists():
        rows = json.loads(existing.read_text())
    for level, lto in MATRIX:
        name = f"{level}-{lto}"
        if args.variant and args.variant != name:
            continue
        if build_context() != context:
            raise ValueError("different build inputs during experiment; previous rows preserved")
        target = output / f"target-{name}"
        if target.exists():
            raise ValueError(f"previous experiment target requires inspection: {target}")
        target.mkdir()
        # APFS clone preserves the default-build cache without retaining 6 full targets.
        # Each experiment changes Cargo's environment profile, never Cargo.toml.
        subprocess.run(["cp", "-cR", str(ROOT / "target/release"), str(target)], check=True)
        environment = dict(os.environ, CARGO_TARGET_DIR=str(target),
            CARGO_PROFILE_RELEASE_OPT_LEVEL=level, CARGO_PROFILE_RELEASE_LTO=lto, CARGO_BUILD_JOBS="1")
        started = time.monotonic()
        print(f"building {name}", flush=True)
        with (output / f"{name}.log").open("w") as log:
            result = subprocess.run(["cargo", "build", "--release", "--locked", "--offline", "--message-format=json-render-diagnostics",
                "-p", "app", "--bin", "norma-os", "--no-default-features", "--features", ",".join(FEATURES)],
                cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT, check=False)
        row = {"variant": name, "opt_level": level, "lto": lto,
            "build_seconds": time.monotonic() - started, "exit_code": result.returncode,
            "source_sha256": context["source_sha256"], "features": FEATURES,
            "cache": "APFS clone of default release cache; build times are not cold-build comparisons",
            "app_bytes": None, "dmg_bytes": None, "performance_status": "pending real-App acceptance"}
        if build_context() != context:
            raise ValueError("different build inputs during build; artifact excluded and target preserved")
        if result.returncode == 0:
            binary = cargo_executable(output / f"{name}.log", target)
            destination = output / f"norma-os-{name}"
            shutil.copy2(binary, destination)
            row.update(binary_bytes=destination.stat().st_size, binary_sha256=digest(destination),
                cargo_executable=str(binary))
        rows = [entry for entry in rows if entry["variant"] != name] + [row]
        existing.write_text(json.dumps(rows, indent=2) + "\n")
        print(json.dumps(row), flush=True)
        # Only this invocation's exact, fixed experiment directory is removed.
        # Preserve failed caches for diagnosis; do not continue an invalid baseline.
        if result.returncode:
            return result.returncode
        shutil.rmtree(target)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
