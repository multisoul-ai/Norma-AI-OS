#!/usr/bin/env python3
"""轻量发布前流式验证 GitHub 原文件，拒绝仅凭 HEAD 判断可用性。"""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import sys
import urllib.request
from urllib.parse import urlsplit


class TrustedRedirects(urllib.request.HTTPRedirectHandler):
    max_redirections = 10

    def redirect_request(self, request, response, code, message, headers, new_url):
        if urlsplit(new_url).scheme != "https":
            raise ValueError("media redirect must remain HTTPS at every hop")
        count = getattr(request, "_media_redirect_count", 0)
        if count >= 10:
            raise ValueError("excessive media redirects")
        redirected = super().redirect_request(request, response, code, message, headers, new_url)
        if redirected is not None:
            redirected._media_redirect_count = count + 1
        return redirected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("wallpapers-on-demand", "light"), required=True)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    spec = importlib.util.spec_from_file_location("release_media", root / "scripts/prepare-release-media.py")
    preparation = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(preparation)
    try:
        manifest = preparation.load_manifest(root)
        verified = 0
        opener = urllib.request.build_opener(TrustedRedirects())
        for entry in manifest["resources"]:
            if preparation.is_bundled(entry["policy"], args.profile):
                continue
            url = manifest["download_base_url"].rstrip("/") + "/" + entry["sha256"] + Path(entry["path"]).suffix
            request = urllib.request.Request(url, headers={"Accept-Encoding": "identity"})
            with opener.open(request, timeout=20) as response:
                if not response.geturl().startswith("https://"):
                    raise ValueError("media redirect must remain HTTPS")
                digest = hashlib.sha256()
                length = 0
                for chunk in iter(lambda: response.read(1024 * 1024), b""):
                    length += len(chunk)
                    if length > entry["bytes"]:
                        raise ValueError(f"oversized remote media: {entry['path']}")
                    digest.update(chunk)
                if length != entry["bytes"] or digest.hexdigest() != entry["sha256"]:
                    raise ValueError(f"remote integrity mismatch: {entry['path']}")
            verified += 1
        print(f"verified {verified} original GitHub media assets for {args.profile}")
    except Exception as error:
        print(f"release media unavailable or invalid: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
