#!/usr/bin/env python3
"""macOS Quick Look 静态帧 → lossless WebP；原视频/GIF 不被改写。"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--update-manifest", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    cwebp = shutil.which("cwebp")
    if not cwebp or not shutil.which("qlmanage"):
        parser.error("requires macOS qlmanage and cwebp")
    spec = importlib.util.spec_from_file_location("release_media", root / "scripts/prepare-release-media.py")
    preparation = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(preparation)
    manifest_path = root / "packaging/media/resources.json"
    manifest = json.loads(manifest_path.read_text())
    output = root / "packaging/media/previews"
    output.mkdir(parents=True, exist_ok=True)
    previews = []
    with tempfile.TemporaryDirectory(prefix="norma-media-preview-") as temporary:
        for entry in manifest["resources"]:
            if entry["policy"] != "dynamic-wallpaper":
                continue
            source = preparation.safe_source(root / "crates/app/assets", entry["path"])
            if source.stat().st_size != entry["bytes"] or preparation.digest(source) != entry["sha256"]:
                raise ValueError(f"original integrity mismatch: {source}")
            subprocess.run(["qlmanage", "-t", "-s", "640", "-o", temporary, str(source)],
                           check=True, capture_output=True, timeout=30)
            png = Path(temporary) / (source.name + ".png")
            target = output / (source.stem + ".webp")
            subprocess.run([cwebp, "-lossless", "-exact", "-m", "6", str(png), "-o", str(target)],
                           check=True, capture_output=True, timeout=30)
            data = target.read_bytes()
            previews.append({"resource": entry["path"], "path": "wallpaper-previews/" + target.name,
                             "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    if args.update_manifest:
        manifest["previews"] = previews
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"previews": previews, "total_preview_bytes": sum(p["bytes"] for p in previews)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
