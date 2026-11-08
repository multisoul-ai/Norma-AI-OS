#!/usr/bin/env python3
"""把已定稿的品牌静帧采样成 100×41×32 的双色 ASCII 初始化资源。

每格 1 字节：低 7 位是 ASCII，最高位是强调色。前 22 帧按各品牌自己的成形顺序
揭开，第 22–31 帧保持稳定画面，供 806ms 交叉淡入时已经停住。
"""

from __future__ import annotations

import math
from pathlib import Path

from PIL import Image

ROWS = 41
COLS = 100
FRAMES = 32
HOLD_FROM = 22
FRAME_BYTES = ROWS * COLS
ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/app/assets/init-animation/source"
OUT = ROOT / "crates/app/assets/init-animation"
PREVIEW = Path("/tmp/norma-brand-ascii-preview")

# name, file, classification
BRANDS = (
    "cursor",
    "grok",
    "opencode",
    "infcode",
    "kimi",
    "zcode",
    "pi",
    "hermes",
)


def luma(r: int, g: int, b: int) -> float:
    return (r + g + b) / 3


def chroma(r: int, g: int, b: int) -> int:
    return max(r, g, b) - min(r, g, b)


def ink_mask(image: Image.Image) -> list[list[bool]]:
    px = image.load()
    w, h = image.size
    mask = [[False] * w for _ in range(h)]
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a > 40 and luma(r, g, b) > 36:
                mask[y][x] = True
    return mask


def classify(name: str, image: Image.Image) -> tuple[list[tuple[int, int, bool]], tuple[int, int, int, int]]:
    px = image.load()
    w, h = image.size
    mask = ink_mask(image)
    saturated = False
    has_gray = False
    has_white = False
    for y in range(h):
        for x in range(w):
            if not mask[y][x]:
                continue
            r, g, b, _ = px[x, y]
            if chroma(r, g, b) > 36:
                saturated = True
            elif luma(r, g, b) >= 190:
                has_white = True
            elif luma(r, g, b) >= 70:
                has_gray = True
    split_gray = has_white and has_gray and not saturated
    cells: list[tuple[int, int, bool]] = []
    for y in range(h):
        for x in range(w):
            if not mask[y][x]:
                continue
            r, g, b, _ = px[x, y]
            if saturated and chroma(r, g, b) > 36:
                accent = True
            elif split_gray:
                accent = luma(r, g, b) >= 190
            elif saturated:
                accent = False
            else:
                accent = True
            cells.append((x, y, accent))
    xs = [c[0] for c in cells]
    ys = [c[1] for c in cells]
    box = (min(xs), min(ys), max(xs), max(ys))
    return cells, box


def sample_grid(image: Image.Image, cells: list[tuple[int, int, bool]], box: tuple[int, int, int, int]):
    minx, miny, maxx, maxy = box
    bw = max(1, maxx - minx + 1)
    bh = max(1, maxy - miny + 1)
    pixel_aspect = bw / bh
    target_rows = 24
    target_cols = max(8, round(target_rows * 2 * pixel_aspect))
    if target_cols > 88:
        scale = 88 / target_cols
        target_cols = 88
        target_rows = max(8, round(target_rows * scale))
    if target_rows > 36:
        target_rows = 36
        target_cols = min(88, max(8, round(target_rows * 2 * pixel_aspect)))
    origin_c = (COLS - target_cols) // 2
    origin_r = (ROWS - target_rows) // 2
    grid = [[{"accent": 0, "neutral": 0, "n": 0} for _ in range(COLS)] for _ in range(ROWS)]
    for x, y, accent in cells:
        col = origin_c + int((x - minx) / bw * target_cols)
        row = origin_r + int((y - miny) / bh * target_rows)
        if col >= COLS or row >= ROWS:
            continue
        slot = grid[row][col]
        slot["n"] += 1
        if accent:
            slot["accent"] += 1
        else:
            slot["neutral"] += 1
    # 每格对应的源像素面积，用来把计数变成覆盖率。
    cell_area = max(1.0, (bw / target_cols) * (bh / target_rows))
    packed_cells = []
    for row in range(ROWS):
        for col in range(COLS):
            slot = grid[row][col]
            if slot["n"] <= 0:
                continue
            coverage = min(1.0, slot["n"] / cell_area)
            if coverage < 0.12:
                continue
            accent = slot["accent"] >= slot["neutral"]
            nx = (col - origin_c + 0.5) / target_cols
            ny = (row - origin_r + 0.5) / target_rows
            packed_cells.append(
                {
                    "row": row,
                    "col": col,
                    "accent": accent,
                    "coverage": coverage,
                    "nx": nx,
                    "ny": ny,
                }
            )
    neutral_count = sum(1 for cell in packed_cells if not cell["accent"])
    if 0 < neutral_count < 8:
        for cell in packed_cells:
            cell["accent"] = True
    return packed_cells


def glyph_for(coverage: float) -> int:
    if coverage < 0.35:
        return ord(":")
    if coverage < 0.7:
        return ord("+")
    return ord("#")


def reveal_order(name: str, cell: dict) -> float:
    nx, ny = cell["nx"], cell["ny"]
    if name == "cursor":
        if cell["accent"]:
            return 0.62 + 0.38 * nx
        ang = math.atan2(nx - 0.5, -(ny - 0.5))
        return ((ang + 2 * math.pi) % (2 * math.pi)) / (2 * math.pi) * 0.58
    if name == "grok":
        dist = abs(nx + ny - 1.0)
        if dist < 0.18:
            return 0.55 * nx
        ang = math.atan2(ny - 0.5, nx - 0.5)
        return 0.55 + 0.45 * ((ang + math.pi) / (2 * math.pi))
    if name == "pi":
        return ny
    if name == "hermes":
        return (nx + ny) / 2
    if name == "opencode":
        if cell["accent"]:
            dist = math.hypot(nx - 0.5, ny - 0.5)
            return max(0.0, 0.6 - dist)
        return 0.65 + 0.35 * ny
    if name == "zcode":
        return ny
    if name == "kimi":
        if cell["accent"]:
            return 0.78 + 0.22 * nx
        return nx * 0.72
    if name == "infcode":
        if cell["accent"]:
            return min(0.22, ny * 0.3)
        return 0.28 + 0.72 * ny
    return ny


def build_frames(name: str, cells: list[dict]) -> bytes:
    # 空格才是不可见格。0 会被解码成 NUL，整张网格都会被画出来。
    blob = bytearray([0x20]) * (FRAMES * FRAME_BYTES)
    orders = [reveal_order(name, cell) for cell in cells]
    low = min(orders)
    span = max(orders) - low or 1.0
    for cell, order in zip(cells, orders):
        cell["order"] = (order - low) / span
    for frame in range(FRAMES):
        # 第 0 帧就要露出第一笔，第 21 帧（index）收束成完整标。
        limit = 1.0 if frame >= HOLD_FROM else (frame + 1) / HOLD_FROM
        base = frame * FRAME_BYTES
        for cell in cells:
            if cell["order"] > limit:
                continue
            glyph = glyph_for(cell["coverage"])
            if cell["accent"]:
                glyph |= 0x80
            blob[base + cell["row"] * COLS + cell["col"]] = glyph
    # 稳定帧必须逐字节相同。
    stable = blob[(HOLD_FROM - 1) * FRAME_BYTES : HOLD_FROM * FRAME_BYTES]
    for frame in range(HOLD_FROM, FRAMES):
        start = frame * FRAME_BYTES
        blob[start : start + FRAME_BYTES] = stable
    return bytes(blob)


def preview(name: str, blob: bytes, accent_rgb: tuple[int, int, int]) -> None:
    frame = blob[(FRAMES - 1) * FRAME_BYTES :]
    scale_x, scale_y = 8, 16
    image = Image.new("RGB", (COLS * scale_x, ROWS * scale_y), (15, 15, 17))
    px = image.load()
    neutral = (195, 195, 196)
    for row in range(ROWS):
        for col in range(COLS):
            value = frame[row * COLS + col]
            if value == 0 or (value & 0x7F) == 0x20:
                continue
            color = accent_rgb if value & 0x80 else neutral
            x0, y0 = col * scale_x, row * scale_y
            for y in range(y0 + 1, y0 + scale_y - 1):
                for x in range(x0 + 1, x0 + scale_x - 1):
                    px[x, y] = color
    PREVIEW.mkdir(exist_ok=True)
    image.save(PREVIEW / f"{name}.png")


def main() -> None:
    accents = {
        "cursor": (155, 140, 255),
        "grok": (244, 244, 245),
        "opencode": (255, 255, 255),
        "infcode": (244, 208, 74),
        "kimi": (47, 107, 255),
        "zcode": (255, 255, 255),
        "pi": (244, 244, 245),
        "hermes": (244, 244, 245),
    }
    for name in BRANDS:
        image = Image.open(SOURCE / f"{name}.png").convert("RGBA")
        cells, box = classify(name, image)
        grid = sample_grid(image, cells, box)
        blob = build_frames(name, grid)
        if len(blob) != FRAMES * FRAME_BYTES:
            raise SystemExit(f"{name} size {len(blob)}")
        (OUT / f"{name}-brand-ascii.bin").write_bytes(blob)
        preview(name, blob, accents[name])
        accent = sum(1 for c in grid if c["accent"])
        neutral = sum(1 for c in grid if not c["accent"])
        print(f"{name}: cells={len(grid)} accent={accent} neutral={neutral} box={box}")


if __name__ == "__main__":
    main()
