#!/usr/bin/env python3
"""发布前核验按需A1运行环境的固定来源、清单和实际归档字节。"""
import argparse
import hashlib
import gzip
import json
from pathlib import Path
import sys
import urllib.request
from urllib.parse import urlsplit

PREFIX = "https://github.com/multisoul-ai/norma-os-releases/releases/download/a1-runtime-"


class HTTPSRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, new_url):
        if urlsplit(new_url).scheme != "https":
            raise ValueError("A1 redirects must remain HTTPS")
        return super().redirect_request(request, response, code, message, headers, new_url)


def sha256(value):
    return len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def validate(descriptor, root):
    base = descriptor["base_url"]
    url = urlsplit(base)
    expected = hashlib.sha256((root / "crates/asr/runtime/a1/requirements.lock").read_bytes()).hexdigest()
    if (not base.startswith(PREFIX) or url.query or url.fragment or "/" in base[len(PREFIX):]
            or not base[len(PREFIX):] or descriptor["schema"] != 1
            or descriptor["target"] != "aarch64-apple-darwin" or descriptor["minimum_macos_major"] != 26
            or descriptor["requirements_sha256"] != expected
            or not 0 < descriptor["manifest_bytes"] <= 16 * 1024 * 1024
            or not sha256(descriptor["manifest_sha256"])):
        raise ValueError("invalid or incompatible A1 release descriptor")


def checked(source, length, expected, capture=False):
    digest = hashlib.sha256()
    completed = 0
    chunks = []
    for chunk in iter(lambda: source.read(1024 * 1024), b""):
        completed += len(chunk)
        if completed > length:
            raise ValueError("oversized A1 release asset")
        digest.update(chunk)
        if capture:
            chunks.append(chunk)
    if completed != length or digest.hexdigest() != expected:
        raise ValueError("A1 release asset size or SHA-256 mismatch")
    return b"".join(chunks)


def verify(descriptor, root, local_assets=None):
    validate(descriptor, root)
    opener = urllib.request.build_opener(HTTPSRedirects())

    def asset(name, length, expected, capture=False):
        if local_assets is not None:
            with (local_assets / name).open("rb") as source:
                return checked(source, length, expected, capture)
        request = urllib.request.Request(descriptor["base_url"] + "/" + name, headers={"Accept-Encoding": "identity"})
        with opener.open(request, timeout=30) as source:
            if urlsplit(source.geturl()).scheme != "https":
                raise ValueError("A1 asset must remain HTTPS")
            return checked(source, length, expected, capture)

    manifest = json.loads(asset("engine.json", descriptor["manifest_bytes"], descriptor["manifest_sha256"], True))
    for key in ("schema", "target", "minimum_macos_major", "requirements_sha256"):
        if manifest[key] != descriptor[key]:
            raise ValueError("A1 manifest does not match app compatibility requirements")
    if not 0 < manifest["archive_bytes"] <= 1024 * 1024 * 1024 or not sha256(manifest["archive_sha256"]):
        raise ValueError("invalid A1 runtime archive metadata")
    asset("engine.tar.bz2", manifest["archive_bytes"], manifest["archive_sha256"])
    return manifest["archive_bytes"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--local-test", action="store_true", help="开发包仅校验描述，不声明远端资源可用")
    parser.add_argument("--local-assets", type=Path, help="校验待发布资源；不代替线上可用性验收")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    try:
        path = root / "packaging/a1-engine/release.json"
        if path.stat().st_size > 16_384:
            raise ValueError("A1 descriptor exceeds 16 KiB")
        descriptor = json.loads(path.read_bytes())
        with gzip.open(path.parent / "engine.json.gz", "rb") as source:
            checked(source, descriptor["manifest_bytes"], descriptor["manifest_sha256"])
        if args.local_test and args.local_assets is None:
            validate(descriptor, root)
            print("A1 descriptor valid; remote availability NOT verified (local test)")
        else:
            count = verify(descriptor, root, args.local_assets)
            print(f"verified A1 manifest and {count} runtime archive bytes")
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"A1 runtime unavailable or invalid: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
