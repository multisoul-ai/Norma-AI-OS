#!/usr/bin/env python3
"""Build-time only: produce a relocatable, signed A1 engine; never run pip in the App."""
import argparse
import hashlib
import json
import os
import pathlib
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
LOCK = ROOT / "crates/asr/runtime/a1/requirements.lock"
PYTHON_URL = "https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.12.16%2B20261026-aarch64-apple-darwin-install_only_stripped.tar.gz"
PYTHON_SHA256 = "353d5f1b4fd2715e7053f12f068435d5bbad16b91e057006b5d01e0925709a7e"
MACHO = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"}


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def contained(path, root):
    return os.path.commonpath([str(path), str(root)]) == str(root)


def extract_python(source, destination):
    links = []
    for member in source.getmembers():
        output = destination / member.name
        if not contained(output.resolve(), destination.resolve()):
            raise ValueError("unsafe standalone Python path")
        if member.issym():
            links.append((output, member.linkname))
        elif member.isdir():
            output.mkdir(parents=True, exist_ok=True)
        elif member.isfile():
            output.parent.mkdir(parents=True, exist_ok=True)
            with source.extractfile(member) as stream, output.open("wb") as target:
                shutil.copyfileobj(stream, target)
            output.chmod(member.mode & 0o777)
        else:
            raise ValueError("unsupported standalone Python archive entry")
    # Files are written before links exist, preventing symlink traversal during extraction.
    for output, target in links:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.symlink_to(target)
    for output, _ in links:
        if not contained(output.resolve(), destination.resolve()):
            raise ValueError("standalone Python link escapes its root")


def materialize(source, destination):
    source = source.resolve()
    for path in source.rglob("*"):
        if path.is_symlink() and not contained(path.resolve(), source):
            raise ValueError("engine contains a link outside its root: " + str(path.relative_to(source)))
    shutil.copytree(source, destination, symlinks=False, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))


def environment():
    result = os.environ.copy()
    for name in ("PYTHONHOME", "PYTHONPATH", "VIRTUAL_ENV", "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH"):
        result.pop(name, None)
    result.update(PYTHONDONTWRITEBYTECODE="1", HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", PATH="/usr/bin:/bin")
    return result


def check_runtime(python):
    # Imports exercise the real native wheels, but do not load a model or access the network.
    code = "import sys,json,importlib.metadata as m; import mlx.core,numpy,sherpa_onnx; from mlx_audio.stt.utils import load_model; print(json.dumps({n:m.version(n) for n in json.loads(sys.argv[1])}))"
    expected = dict(line.split("==") for line in LOCK.read_text().splitlines() if line.strip())
    raw = subprocess.check_output([str(python), "-I", "-B", "-c", code, json.dumps(list(expected))], env=environment(), timeout=120)
    if json.loads(raw) != expected:
        raise ValueError("engine dependency versions differ from requirements.lock")


def has_external_dependencies(output):
    return any(line.startswith("\t") and not line.strip().startswith(("@rpath/", "@loader_path/", "@executable_path/", "/System/Library/", "/usr/lib/"))
               for line in output.splitlines())


def sign_native_files(root, identity):
    native = []
    for path in sorted(root.rglob("*")):
        if path.is_file():
            with path.open("rb") as stream:
                if stream.read(4) in MACHO:
                    native.append(path)
    for path in native:
        # Some standalone libraries retain a build-prefix LC_ID_DYLIB. It is not a
        # dependency, but normalize it before checking actual load commands.
        ids = subprocess.check_output(["/usr/bin/otool", "-D", str(path)], text=True)
        if any(line and not line.startswith("@") and not line.endswith(":") for line in ids.splitlines()[1:]):
            subprocess.run(["/usr/bin/install_name_tool", "-id", "@rpath/" + path.name, str(path)], check=True, stderr=subprocess.PIPE)
        linked = subprocess.check_output(["/usr/bin/otool", "-L", str(path)], text=True)
        for dependency in set(line.strip().split(" (", 1)[0] for line in linked.splitlines() if line.startswith("\t")):
            if dependency.startswith(("@", "/System/Library/", "/usr/lib/")):
                continue
            matches = [candidate for candidate in native if candidate.name == pathlib.Path(dependency).name]
            if len(matches) != 1:
                raise ValueError("non-relocatable native dependency: " + str(path.relative_to(root)))
            replacement = "@loader_path/" + os.path.relpath(matches[0], path.parent)
            subprocess.run(["/usr/bin/install_name_tool", "-change", dependency, replacement, str(path)], check=True, stderr=subprocess.PIPE)
        linked = subprocess.check_output(["/usr/bin/otool", "-L", str(path)], text=True)
        if has_external_dependencies(linked):
            raise ValueError("non-relocatable native dependency: " + str(path.relative_to(root)))
        command = ["/usr/bin/codesign", "--force", "--sign", identity]
        if identity != "-":
            command += ["--options", "runtime", "--timestamp"]
        if path.name == "python3.12":
            command += ["--entitlements", str(ROOT / "packaging/macos/NormaOS.entitlements")]
        subprocess.run(command + [str(path)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(path)], check=True, stderr=subprocess.PIPE)
    return len(native)


def build_inputs():
    return {"python_sha256": PYTHON_SHA256, "builder_sha256": digest(pathlib.Path(__file__)),
            "entitlements_sha256": digest(ROOT / "packaging/macos/NormaOS.entitlements")}


def verified_output(output, identity):
    try:
        metadata = json.loads((output / "engine.json").read_text())
        build = json.loads((output / "build.json").read_text())
        return (all(build.get(key) == value for key, value in build_inputs().items())
                and build["signing_identity"] == identity
                and metadata["requirements_sha256"] == digest(LOCK)
                and metadata["archive_sha256"] == digest(output / "engine.tar.bz2"))
    except (OSError, ValueError, KeyError):
        return False


def build(identity):
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise ValueError("A1 engine packaging currently requires an Apple silicon Mac")
    output = ROOT / "build/a1-engine"
    if verified_output(output, identity):
        print("A1 engine package is already verified")
        return
    cache = ROOT / "build/a1-engine-cache"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / "python.tar.gz"
    if not archive.is_file() or digest(archive) != PYTHON_SHA256:
        partial = archive.with_suffix(".partial")
        with urllib.request.urlopen(PYTHON_URL, timeout=60) as response, partial.open("wb") as target:
            shutil.copyfileobj(response, target)
        if digest(partial) != PYTHON_SHA256:
            partial.unlink()
            raise ValueError("standalone Python SHA256 mismatch")
        partial.replace(archive)
    with tempfile.TemporaryDirectory(prefix="a1-engine-", dir=ROOT / "build") as temporary:
        staging = pathlib.Path(temporary)
        raw = staging / "raw"
        raw.mkdir()
        with tarfile.open(archive) as source:
            # Upstream digest is pinned above; data filter also rejects escaping links and devices.
            extract_python(source, raw)
        engine = staging / "engine"
        engine.mkdir()
        materialize(raw / "python", engine / "python")
        shutil.rmtree(raw)
        python = engine / "python/bin/python3.12"
        wheels = cache / "wheels"
        wheels.mkdir(exist_ok=True)
        build_env = environment()
        # Package acquisition is exclusively a build-time operation, never an App operation.
        subprocess.run([str(python), "-I", "-B", "-m", "pip", "--isolated", "download", "--only-binary=:all:", "--index-url", "https://pypi.org/simple", "--no-deps", "--dest", str(wheels), "-r", str(LOCK), "setuptools==84.2.1"], check=True, env=build_env)
        subprocess.run([str(python), "-I", "-B", "-m", "pip", "--isolated", "install", "--no-index", "--find-links", str(wheels), "--no-deps", "--only-binary=:all:", "-r", str(LOCK), "setuptools==84.2.1"], check=True, env=build_env)
        # Wheel links must also be internal. Materialize in place without a second full copy.
        for path in engine.rglob("*"):
            if path.is_symlink():
                target = path.resolve()
                if not contained(target, engine) or not target.is_file():
                    raise ValueError("unsafe engine wheel symlink")
                path.unlink()
                shutil.copy2(target, path)
        for path in list(engine.rglob("__pycache__")):
            shutil.rmtree(path)
        native_count = sign_native_files(engine, identity)
        check_runtime(python)
        package = staging / "package"
        package.mkdir()
        metadata = {"schema": 1, "minimum_macos_major": 26, "target": "aarch64-apple-darwin", "python": "python/bin/python3.12", "requirements_sha256": digest(LOCK), "files": []}
        for path in sorted(engine.rglob("*")):
            if path.is_file():
                metadata["files"].append({"path": path.relative_to(engine).as_posix(), "bytes": path.stat().st_size, "sha256": digest(path)})
        payload = package / "engine.tar.bz2"
        with tarfile.open(payload, "w:bz2", compresslevel=1, dereference=True) as target:
            target.add(engine, arcname="engine")
        metadata.update(archive_bytes=payload.stat().st_size, archive_sha256=digest(payload))
        (package / "engine.json").write_text(json.dumps(metadata, indent=2) + "\n")
        provenance = {**build_inputs(), "python_url": PYTHON_URL, "python_sha256": PYTHON_SHA256, "signing_identity": identity, "native_files_signed": native_count, "wheels": [{"name": p.name, "sha256": digest(p)} for p in sorted(wheels.glob("*.whl"))]}
        (package / "build.json").write_text(json.dumps(provenance, indent=2) + "\n")
        # Do not replace a previous good package until every dependency and signature passed.
        if output.exists():
            shutil.rmtree(output)
        package.rename(output)
        print("Built A1 engine:", len(metadata["files"]), "files;", metadata["archive_bytes"], "archive bytes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--signing-identity", default="-")
    arguments = parser.parse_args()
    build(arguments.signing_identity)
