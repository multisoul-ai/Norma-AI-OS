#!/usr/bin/env python3
"""Guard live video wallpaper against CPU-frame and blocking regressions."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    return Path(
        subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"],
            text=True,
        ).strip()
    )


def strip_rust_comments(source: str) -> str:
    output: list[str] = []
    i = 0
    in_block = 0
    in_string = False
    in_char = False
    escaped = False
    while i < len(source):
        ch = source[i]
        nxt = source[i + 1] if i + 1 < len(source) else ""

        if in_block:
            if ch == "/" and nxt == "*":
                in_block += 1
                output.extend("  ")
                i += 2
            elif ch == "*" and nxt == "/":
                in_block -= 1
                output.extend("  ")
                i += 2
            else:
                output.append("\n" if ch == "\n" else " ")
                i += 1
            continue

        if in_string:
            output.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                in_string = False
            i += 1
            continue

        if in_char:
            output.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == "'":
                in_char = False
            i += 1
            continue

        if ch == "/" and nxt == "/":
            while i < len(source) and source[i] != "\n":
                output.append(" ")
                i += 1
            continue
        if ch == "/" and nxt == "*":
            in_block = 1
            output.extend("  ")
            i += 2
            continue
        if ch == '"':
            in_string = True
        elif ch == "'":
            in_char = True
        output.append(ch)
        i += 1

    return "".join(output)


def add_failure(failures: list[str], path: Path, message: str, hint: str) -> None:
    failures.append(f"{path}: {message}\n  fix: {hint}")


def require_contains(
    failures: list[str],
    path: Path,
    source: str,
    needle: str,
    message: str,
    hint: str,
) -> None:
    if needle not in source:
        add_failure(failures, path, message, hint)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check live video wallpaper low-copy performance invariants."
    )
    parser.add_argument(
        "--staged",
        action="store_true",
        help="Accepted for pre-commit compatibility; this fixed-path rule still checks the relevant files.",
    )
    parser.add_argument(
        "--base",
        help="Accepted for CI compatibility; this fixed-path rule still checks the relevant files.",
    )
    parser.parse_args()

    root = repo_root()
    video_wallpaper = root / "crates/app/src/app/video_wallpaper.rs"
    video_decoder = root / "crates/app/src/app/video_decoder.rs"
    chrome = root / "crates/app/src/app/chrome.rs"
    license_gate = root / "crates/app/src/license_gate/gate_root.rs"

    failures: list[str] = []
    wall_raw = video_wallpaper.read_text()
    decoder_raw = video_decoder.read_text()
    chrome_raw = chrome.read_text()
    license_gate_raw = license_gate.read_text()
    wall_code = strip_rust_comments(wall_raw)
    decoder_code = strip_rust_comments(decoder_raw)
    chrome_code = strip_rust_comments(chrome_raw)
    license_gate_code = strip_rust_comments(license_gate_raw)

    for token in (
        "RenderImage",
        "render_image_from_rgba",
        "next_frame_rgba",
        "render_video_wallpaper_image",
    ):
        if token in wall_code:
            add_failure(
                failures,
                video_wallpaper,
                f"live video wallpaper must not contain `{token}`",
                "keep live frames on the CVPixelBuffer/gpui::surface path; reserve CPU images for thumbnails.",
            )

    require_contains(
        failures,
        video_wallpaper,
        wall_code,
        "std::thread::Builder::new",
        "decoder must run on a dedicated OS thread",
        "spawn `decode_video_wallpaper_frames` with std::thread::Builder, not GPUI background_executor.",
    )
    require_contains(
        failures,
        video_wallpaper,
        wall_raw,
        '"multisoul-video-wallpaper-decoder"',
        "decoder thread must have a stable diagnostic name",
        'name the thread "multisoul-video-wallpaper-decoder".',
    )
    if re.search(
        r"background_executor\s*\(\)\s*\.\s*spawn\s*\(\s*async\s+move\s*\{[^}]*decode_video_wallpaper_frames",
        wall_code,
        re.DOTALL,
    ):
        add_failure(
            failures,
            video_wallpaper,
            "decoder loop must not run inside GPUI background_executor().spawn",
            "move the synchronous AVAssetReader loop to the dedicated decoder thread.",
        )

    for needle, message, hint in (
        (
            "setAlwaysCopiesSampleData(false)",
            "AVAssetReaderTrackOutput must disable sample data copies",
            "call setAlwaysCopiesSampleData(false) before startReading.",
        ),
        (
            "kCVPixelBufferMetalCompatibilityKey",
            "output settings must request Metal-compatible pixel buffers",
            "include kCVPixelBufferMetalCompatibilityKey = true in output settings.",
        ),
        (
            "kCVPixelBufferIOSurfacePropertiesKey",
            "output settings must request IOSurface-backed pixel buffers",
            "include kCVPixelBufferIOSurfacePropertiesKey = empty dictionary in output settings.",
        ),
        (
            "CVPixelBufferGetIOSurface",
            "DecodedVideoFrame must validate IOSurface backing without retaining it",
            "call CVPixelBufferGetIOSurface as a null check; do not use core-video's owning get_io_surface() wrapper.",
        ),
        (
            "get_plane_count()",
            "DecodedVideoFrame must validate the expected 420f plane layout",
            "reject live frames that are not bi-planar.",
        ),
    ):
        require_contains(failures, video_decoder, decoder_code, needle, message, hint)

    if "get_io_surface()" in decoder_code:
        add_failure(
            failures,
            video_decoder,
            "live frame validation must not call CVPixelBuffer::get_io_surface()",
            "use a non-retaining CVPixelBufferGetIOSurface null check to avoid over-releasing CoreVideo-owned surfaces.",
        )

    require_contains(
        failures,
        chrome,
        chrome_code,
        "render_video_wallpaper_surface",
        "chrome must mount the low-copy video wallpaper surface",
        "call render_video_wallpaper_surface() for live video wallpaper.",
    )
    if "render_video_wallpaper_image" in chrome_code:
        add_failure(
            failures,
            chrome,
            "chrome must not call the old CPU image video wallpaper path",
            "remove render_video_wallpaper_image() from live wallpaper rendering.",
        )

    require_contains(
        failures,
        license_gate,
        license_gate_code,
        "video_wallpaper_surface",
        "license gate must mount the shared low-copy video wallpaper surface",
        "reuse video_wallpaper_surface() instead of introducing a CPU image frame path.",
    )
    require_contains(
        failures,
        license_gate,
        license_gate_code,
        "sync_license_video_background",
        "license gate must start its video background outside the GPUI foreground decoder path",
        "sync the shared video wallpaper worker before rendering the activation shell.",
    )
    for token in ("RenderImage", "render_image_from_rgba", "next_frame_rgba"):
        if token in license_gate_code:
            add_failure(
                failures,
                license_gate,
                f"license gate video background must not contain `{token}`",
                "keep license live frames on the shared CVPixelBuffer/gpui::surface path.",
            )

    if failures:
        print("ERROR: video wallpaper performance guardrail violations found.", file=sys.stderr)
        print("\n".join(failures), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
