#!/usr/bin/env python3
"""Register an existing, pinned local A1 runtime for development use.

No downloads, model imports, compilation, inference, default changes or file removal.
The interpreter/models remain at their supplied locations; a release runtime bundle
must be prepared separately before distributing this feature in a DMG.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = ROOT / "crates/asr/runtime/a1/artifacts.json"
LOCK = ROOT / "crates/asr/runtime/a1/requirements.lock"


def sha(path):
    with path.open("rb") as stream:
        digest = hashlib.sha256()
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
        return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--zipformer", type=Path, required=True)
    parser.add_argument("--qwen", type=Path, required=True)
    parser.add_argument("--model-root", type=Path, required=True)
    args = parser.parse_args()
    # Preserve venv interpreter path: resolving its symlink loses site-packages.
    interpreter = args.python.absolute()
    roots = {role: getattr(args, role).resolve(strict=True) for role in ("zipformer", "qwen")}
    expected = dict(line.strip().split("==") for line in LOCK.read_text().splitlines() if line.strip())
    result = subprocess.run([str(interpreter), "-I", "-B", "-c",
        "import importlib.metadata,json,sys; names=json.loads(sys.argv[1]); "
        "print(json.dumps({'python':sys.version,'packages':{n:importlib.metadata.version(n) for n in names}}))",
        json.dumps(list(expected))], check=True, text=True, capture_output=True, timeout=30)
    runtime = json.loads(result.stdout)
    if runtime["packages"] != expected:
        raise RuntimeError("runtime package versions do not match requirements.lock")
    audited = []
    for artifact in json.loads(ARTIFACTS.read_text()):
        path = roots[artifact["role"]] / artifact["name"]
        if not path.is_file() or path.stat().st_size != artifact["bytes"] or sha(path) != artifact["sha256"]:
            raise RuntimeError(f"artifact integrity mismatch: {path}")
        audited.append(dict(artifact, path=str(path)))
    manifest = dict(protocol=1, python=str(interpreter), python_sha256=sha(interpreter),
                    zipformer=str(roots["zipformer"]), qwen=str(roots["qwen"]))
    destination = args.model_root.absolute() / "a1"
    destination.mkdir(parents=True, exist_ok=True)
    target = destination / "runtime.json"
    if target.exists() and json.loads(target.read_text()) != manifest:
        raise RuntimeError(f"different installation already registered: {target}")
    audit = dict(runtime=runtime, files=audited, manifest=manifest,
                 source="A1 audited 2026-10-26 artifacts; repository worker v1",
                 worker_sha256=sha(ROOT / "crates/asr/runtime/a1/worker.py"),
                 requirements_sha256=sha(LOCK), models_loaded=False,
                 limitation="Development registration; external interpreter/model paths, no redistributable runtime bundle")
    for path, content in ((target, manifest), (destination / "preparation-audit.json", audit)):
        with tempfile.NamedTemporaryFile(mode="w", dir=destination, delete=False) as stream:
            json.dump(content, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
            temporary = stream.name
        os.replace(temporary, path)
    print(json.dumps(dict(manifest=str(target), artifacts_verified=len(audited), models_loaded=False), ensure_ascii=False))


if __name__ == "__main__":
    main()
