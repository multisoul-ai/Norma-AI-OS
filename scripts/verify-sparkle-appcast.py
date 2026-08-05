#!/usr/bin/env python3
"""Verify a canonical Sparkle enclosure against local release artifacts."""

from __future__ import annotations

import argparse
import hashlib
import plistlib
import re
import sys
from pathlib import Path
from urllib.parse import quote
from xml.etree import ElementTree


# Sparkle 2.9.5's official generate_appcast still emits the original namespace.
SPARKLE_NS = "http://www.andymatuschak.org/xml-namespaces/sparkle"
DMG_NAME = "Norma-OS_aarch64.dmg"
RELEASE_BASE = "https://github.com/multisoul-ai/norma-os-releases/releases/download"


def fail(message: str) -> None:
    raise ValueError(message)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--assets", required=True, type=Path)
    parser.add_argument("--plist", required=True, type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not re.fullmatch(r"v\d+\.\d+\.\d+", args.tag):
        fail(f"tag must use vMAJOR.MINOR.PATCH: {args.tag}")

    dmg = args.assets / DMG_NAME
    checksum = args.assets / f"{DMG_NAME}.sha256"
    appcast = args.assets / "appcast.xml"
    for required in (dmg, checksum, appcast, args.plist):
        if not required.is_file():
            fail(f"required artifact is missing: {required}")

    digest = hashlib.sha256(dmg.read_bytes()).hexdigest()
    checksum_parts = checksum.read_text(encoding="utf-8").strip().split()
    if checksum_parts != [digest, DMG_NAME]:
        fail("checksum file must contain the exact DMG digest and canonical filename")

    with args.plist.open("rb") as stream:
        plist = plistlib.load(stream)
    short_version = plist.get("CFBundleShortVersionString")
    build_version = plist.get("CFBundleVersion")
    if not isinstance(short_version, str) or not short_version:
        fail("CFBundleShortVersionString is missing")
    if not isinstance(build_version, str) or not build_version:
        fail("CFBundleVersion is missing")
    if args.tag != f"v{short_version}":
        fail(f"tag {args.tag} does not match bundle short version {short_version}")

    root = ElementTree.parse(appcast).getroot()
    items = root.findall("./channel/item")
    if len(items) != 1:
        fail(f"appcast must contain exactly one item, found {len(items)}")
    item = items[0]
    enclosures = item.findall("enclosure")
    if len(enclosures) != 1:
        fail(f"appcast must contain exactly one enclosure, found {len(enclosures)}")
    enclosure = enclosures[0]

    expected_url = f"{RELEASE_BASE}/{quote(args.tag)}/{DMG_NAME}"
    actual_url = enclosure.get("url")
    if actual_url != expected_url:
        fail(f"enclosure URL must be immutable tag URL {expected_url}, got {actual_url}")
    if "/releases/latest/" in actual_url:
        fail("enclosure URL must not use releases/latest")

    actual_length = enclosure.get("length")
    expected_length = str(dmg.stat().st_size)
    if actual_length != expected_length:
        fail(f"enclosure length must be {expected_length}, got {actual_length}")

    sparkle_version = item.findtext(f"{{{SPARKLE_NS}}}version")
    sparkle_short_version = item.findtext(f"{{{SPARKLE_NS}}}shortVersionString")
    signature = enclosure.get(f"{{{SPARKLE_NS}}}edSignature")
    if sparkle_version != build_version:
        fail(f"sparkle:version must be {build_version}, got {sparkle_version}")
    if sparkle_short_version != short_version:
        fail(
            "sparkle:shortVersionString must match CFBundleShortVersionString "
            f"{short_version}, got {sparkle_short_version}"
        )
    if not signature:
        fail("sparkle:edSignature must be present and non-empty")

    print(f"verified Sparkle appcast for {args.tag}: {digest}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, ElementTree.ParseError, plistlib.InvalidFileException) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1) from error
