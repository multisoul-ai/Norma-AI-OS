#!/usr/bin/env python3
"""按可信清单生成发布媒体目录，保持源文件原字节。"""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import sys
import tempfile
from urllib.parse import urlparse
import uuid


PROFILES = ("full", "wallpapers-on-demand", "light")
POLICIES = ("core", "dynamic-wallpaper", "tutorial-video")


class PreparationError(ValueError):
    pass


def digest(path):
    sha = hashlib.sha256()
    with path.open("rb") as handle:
        if handle.read(128).startswith(b"version https://git-lfs.github.com/spec/v1"):
            raise PreparationError(f"Git LFS pointer is not media: {path}")
        handle.seek(0)
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            sha.update(chunk)
    return sha.hexdigest()


def safe_source(assets, rel):
    for ancestor in (assets, *assets.parents):
        if ancestor.is_symlink():
            raise PreparationError(f"symlink is not a release resource directory: {ancestor}")
    if not isinstance(rel, str) or "\\" in rel:
        raise PreparationError("resource path must be a relative POSIX path")
    path = PurePosixPath(rel)
    if (path.is_absolute() or not path.parts or ".." in path.parts
            or path.as_posix() != rel or path.parts[0] not in ("wallpapers", "feature-guide")):
        raise PreparationError(f"unsafe resource path: {rel}")
    current = assets
    for part in path.parts:
        current = current / part
        if current.is_symlink():
            raise PreparationError(f"symlink is not a release resource: {rel}")
    if not current.is_file():
        raise PreparationError(f"missing resource: {rel}")
    return current


def load_manifest(root):
    path = root / "packaging/media/resources.json"
    manifest = json.loads(path.read_text())
    if manifest.get("schema_version") != 1:
        raise PreparationError("unsupported media manifest schema")
    version = manifest.get("resource_version", "")
    if not isinstance(version, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", version):
        raise PreparationError("invalid resource version")
    base_url = manifest.get("download_base_url", "")
    parsed = urlparse(base_url)
    if (parsed.scheme != "https" or not parsed.hostname or parsed.username
            or parsed.password or parsed.query or parsed.fragment):
        raise PreparationError("production media base URL must use HTTPS without query or fragment")
    entries = manifest.get("resources")
    if not isinstance(entries, list) or not entries:
        raise PreparationError("media resource list cannot be empty")
    assets = root / "crates/app/assets"
    seen = set()
    for entry in entries:
        rel = entry["path"]
        source = safe_source(assets, rel)
        if rel in seen:
            raise PreparationError(f"duplicate resource: {rel}")
        seen.add(rel)
        if entry.get("policy") not in POLICIES:
            raise PreparationError(f"unknown resource policy: {rel}")
        if type(entry.get("bytes")) is not int or entry["bytes"] < 0:
            raise PreparationError(f"invalid resource length: {rel}")
        if not isinstance(entry.get("sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]):
            raise PreparationError(f"invalid resource SHA-256: {rel}")
        if source.stat().st_size != entry["bytes"] or digest(source) != entry["sha256"]:
            raise PreparationError(f"source integrity mismatch: {rel}")
    actual = {
        file.relative_to(assets).as_posix()
        for directory in ("wallpapers", "feature-guide")
        for file in (assets / directory).rglob("*") if file.is_file()
    }
    if actual != seen:
        raise PreparationError(f"media whitelist differs from source: {sorted(actual ^ seen)}")
    previews = manifest.get("previews", [])
    if not isinstance(previews, list):
        raise PreparationError("media previews must be a list")
    preview_ids = set()
    for preview in previews:
        if (preview.get("resource") not in seen or preview["resource"] in preview_ids
                or not re.fullmatch(r"wallpaper-previews/[A-Za-z0-9_-]+\.webp", preview.get("path", ""))
                or PurePosixPath(preview["path"]).stem != PurePosixPath(preview["resource"]).stem):
            raise PreparationError("unsafe or duplicate media preview")
        preview_ids.add(preview["resource"])
        source = preview_source(root, preview)
        if (type(preview.get("bytes")) is not int or preview["bytes"] < 0
                or not isinstance(preview.get("sha256"), str)
                or not re.fullmatch(r"[0-9a-f]{64}", preview["sha256"])
                or source.stat().st_size != preview["bytes"] or digest(source) != preview["sha256"]):
            raise PreparationError(f"preview integrity mismatch: {preview['path']}")
    return manifest


def preview_source(root, preview):
    source = root / "packaging/media/previews" / PurePosixPath(preview["path"]).name
    for component in (source, *source.parents):
        if component.is_symlink():
            raise PreparationError(f"symlink is not a media preview: {component}")
    if not source.is_file():
        raise PreparationError(f"missing media preview: {source}")
    return source


def is_bundled(policy, profile):
    return profile == "full" or policy == "core" or (
        profile == "wallpapers-on-demand" and policy == "tutorial-video")


def prepare(root, output, profile):
    root = root.resolve()
    build = root / "build"
    if build.is_symlink():
        raise PreparationError("build directory cannot be a symlink")
    if output.is_symlink():
        raise PreparationError("output directory cannot be a symlink")
    output = output.resolve()
    if output.exists() and not output.is_dir():
        raise PreparationError("existing output must be a directory")
    if output == build or build not in output.parents:
        raise PreparationError("output must be a child directory under the repository build directory")
    manifest = load_manifest(root)
    output.parent.mkdir(parents=True, exist_ok=True)
    backup = output.with_name(f".{output.name}-backup-{uuid.uuid4().hex}")
    with tempfile.TemporaryDirectory(prefix=".media-stage-", dir=output.parent) as temporary:
        stage = Path(temporary) / "ready"
        stage.mkdir()
        catalog = {"schema_version": 1, "resource_version": manifest["resource_version"],
                   "resources": []}
        bundled_bytes = 0
        bundled_count = 0
        previews_by_resource = {preview["resource"]: preview for preview in manifest.get("previews", [])}
        for entry in manifest["resources"]:
            rel = entry["path"]
            bundled = is_bundled(entry["policy"], profile)
            suffix = PurePosixPath(rel).suffix
            catalog["resources"].append({
                **entry, "bundled": bundled,
                "url": f"{manifest['download_base_url'].rstrip('/')}/{entry['sha256']}{suffix}",
            })
            if rel in previews_by_resource:
                catalog["resources"][-1]["preview"] = previews_by_resource[rel]
            if bundled:
                source = safe_source(root / "crates/app/assets", rel)
                target = stage / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
                if target.stat().st_size != entry["bytes"] or digest(target) != entry["sha256"]:
                    raise PreparationError(f"source changed during staging: {rel}")
                bundled_bytes += entry["bytes"]
                bundled_count += 1
        source_bytes = sum(entry["bytes"] for entry in manifest["resources"])
        preview_bytes = 0
        for preview in manifest.get("previews", []):
            source = preview_source(root, preview)
            target = stage / preview["path"]
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            if target.stat().st_size != preview["bytes"] or digest(target) != preview["sha256"]:
                raise PreparationError(f"preview changed during staging: {preview['path']}")
            preview_bytes += preview["bytes"]
        (stage / "wallpaper-previews").mkdir(exist_ok=True)
        catalog_json = json.dumps(catalog, ensure_ascii=False, indent=2) + "\n"
        report = {
            "schema_version": 1, "profile": profile,
            "source_bytes": source_bytes, "source_count": len(manifest["resources"]),
            "bundled_bytes": bundled_bytes, "bundled_count": bundled_count,
            "external_bytes": source_bytes - bundled_bytes,
            "external_count": len(manifest["resources"]) - bundled_count,
            "saved_percent": 100 * (source_bytes - bundled_bytes) / source_bytes if source_bytes else 0,
            "preview_bytes": preview_bytes,
            "catalog_bytes": len(catalog_json.encode("utf-8")),
            "media_payload_bytes": bundled_bytes + preview_bytes + len(catalog_json.encode("utf-8")),
        }
        (stage / "media-catalog.json").write_text(catalog_json)
        (stage / "media-report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
        if output.exists():
            output.rename(backup)
        try:
            stage.rename(output)
        except OSError:
            if backup.exists():
                backup.rename(output)
            raise
        if backup.exists():
            try:
                shutil.rmtree(backup)
            except OSError as error:
                # Promotion is the commit point. A cleanup warning must not
                # claim failure after the new profile has become visible.
                print(f"release media backup cleanup warning: {backup}: {error}", file=sys.stderr)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path, default=Path("build/release-media"))
    parser.add_argument("--profile", choices=PROFILES, default="full")
    args = parser.parse_args()
    output = args.output if args.output.is_absolute() else args.root / args.output
    try:
        print(json.dumps(prepare(args.root, output, args.profile), ensure_ascii=False))
    except (PreparationError, OSError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"release media preparation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
