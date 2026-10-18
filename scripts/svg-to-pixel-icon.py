#!/usr/bin/env python3
"""把官方 SVG 的深色字形转成 24x24 整数像素 rect 图标（currentColor）。

用法: svg-to-pixel-icon.py <输入.svg> <输出.svg> [--grid 20] [--threshold 0.5]
依赖 ImageMagick 的 magick 命令做光栅化；输入输出路径全部由参数传入。
字形缩放到 grid x grid 的单元格，居中放入 24x24 画布；同一行连续的深色单元合并为一个 rect。
"""
import argparse
import subprocess
import sys

CANVAS = 24
RENDER = 640


def rasterize(src):
    out = subprocess.run(
        ["magick", "-background", "white", "-density", "384", src, "-resize",
         f"{RENDER}x{RENDER}!", "-colorspace", "Gray", "-depth", "8", "gray:-"],
        check=True, capture_output=True).stdout
    if len(out) != RENDER * RENDER:
        sys.exit(f"unexpected raster size {len(out)}")
    return out


def cells(raw, grid, threshold):
    step = RENDER / grid
    rows = []
    for gy in range(grid):
        row = []
        for gx in range(grid):
            x0, x1 = int(gx * step), int((gx + 1) * step)
            y0, y1 = int(gy * step), int((gy + 1) * step)
            total = dark = 0
            for y in range(y0, y1):
                base = y * RENDER
                for x in range(x0, x1):
                    total += 1
                    dark += raw[base + x] < 128
            row.append(dark / total >= threshold)
        rows.append(row)
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("input")
    parser.add_argument("output")
    parser.add_argument("--grid", type=int, default=20)
    parser.add_argument("--threshold", type=float, default=0.5)
    args = parser.parse_args()
    offset = (CANVAS - args.grid) // 2
    rows = cells(rasterize(args.input), args.grid, args.threshold)
    rects = []
    for y, row in enumerate(rows):
        x = 0
        while x < len(row):
            if row[x]:
                start = x
                while x < len(row) and row[x]:
                    x += 1
                rects.append((start + offset, y + offset, x - start))
            else:
                x += 1
    lines = [
        '<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="currentColor">',
        "  <!-- 由 scripts/svg-to-pixel-icon.py 从官方 SVG 转换 -->",
    ]
    lines += [f'  <rect x="{x}" y="{y}" width="{w}" height="1"/>' for x, y, w in rects]
    lines.append("</svg>")
    with open(args.output, "w") as handle:
        handle.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
