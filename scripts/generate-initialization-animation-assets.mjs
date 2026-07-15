#!/usr/bin/env node
/**
 * Build the compact 32 × 41 × 100 initialization-animation assets.
 *
 * Terminal data comes from the official ghostty-org/website repository's
 * `terminals/home/animation_frames/frame_017.txt` through `frame_048.txt`.
 * Agent data is a deterministic, duotone ASCII interpretation of each
 * bundled brand image: darker details form the neutral layer and the brand
 * color forms the accent layer.
 *
 * Usage:
 *   node scripts/generate-initialization-animation-assets.mjs [ghostty-website-root]
 */

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { inflateSync } from "node:zlib";

const [ghosttyRoot] = process.argv.slice(2);

const root = path.resolve(import.meta.dirname, "..");
const outputDirectory = path.join(root, "crates/app/assets/init-animation");
const sourceDirectory = path.join(outputDirectory, "source");
const logoPath = path.join(root, "crates/app/assets/icons/logo.png");
const codexBrandPath = path.join(sourceDirectory, "codex-brand.png");
const claudeCodeBrandPath = path.join(sourceDirectory, "claude-code-brand.png");
const rows = 41;
const columns = 100;
const frames = 32;
const accentBit = 0x80;
const middleDot = 0x7f;
const ghosttyWebsiteCommit = "10bfc9b9d240ec43bcf983d99f9bf405c3478101";
const ghosttyFramesSha256 = "d417e2c1aa7b2f00b12ba236024f4ec99b18a4c770dbb3ee19ddd1a7793a3f70";
const normaLogoSha256 = "950ce77367e5838bd24ba83444843d4352a3896ab9f4ee1161f98506c29ece8b";

fs.mkdirSync(outputDirectory, { recursive: true });
if (ghosttyRoot) {
  fs.writeFileSync(
    path.join(outputDirectory, "ghostty-terminal-16-47.bin"),
    buildGhosttyFrames(ghosttyRoot),
  );
}
fs.writeFileSync(
  path.join(outputDirectory, "norma-agent-duotone.bin"),
  buildNormaFrames(logoPath),
);
fs.writeFileSync(
  path.join(outputDirectory, "codex-brand-ascii.bin"),
  buildCodexBrandFrames(codexBrandPath),
);
fs.writeFileSync(
  path.join(outputDirectory, "claude-code-brand-ascii.bin"),
  buildClaudeCodeBrandFrames(claudeCodeBrandPath),
);
fs.writeFileSync(
  path.join(outputDirectory, "cursor-brand-ascii.bin"),
  buildCursorBrandFrames(),
);

function buildGhosttyFrames(websiteRoot) {
  const revision = execFileSync("git", ["-C", websiteRoot, "rev-parse", "HEAD"], {
    encoding: "utf8",
  }).trim();
  if (revision !== ghosttyWebsiteCommit) {
    throw new Error(
      `Ghostty website must be pinned at ${ghosttyWebsiteCommit}; received ${revision || "no revision"}`,
    );
  }
  const directory = path.join(websiteRoot, "terminals/home/animation_frames");
  const output = Buffer.alloc(frames * rows * columns);
  const sourceHash = createHash("sha256");
  for (let frame = 0; frame < frames; frame += 1) {
    const number = String(frame + 17).padStart(3, "0");
    const source = fs.readFileSync(path.join(directory, `frame_${number}.txt`));
    sourceHash.update(source);
    const text = source.toString("utf8");
    const lines = text.replace(/\r/g, "").split("\n");
    if (lines.at(-1) === "") lines.pop();
    if (lines.length !== rows) {
      throw new Error(`Ghostty frame ${number} must have ${rows} rows; got ${lines.length}`);
    }
    lines.forEach((line, row) => {
      const cells = parseGhosttyLine(line);
      if (cells.length !== columns) {
        throw new Error(
          `Ghostty frame ${number}, row ${row} must have ${columns} cells; got ${cells.length}`,
        );
      }
      cells.forEach((cell, column) => {
        output[(frame * rows + row) * columns + column] = cell;
      });
    });
  }
  const sourceDigest = sourceHash.digest("hex");
  if (sourceDigest !== ghosttyFramesSha256) {
    throw new Error(
      `Ghostty frame source hash mismatch; expected ${ghosttyFramesSha256}, received ${sourceDigest}`,
    );
  }
  return output;
}

function parseGhosttyLine(line) {
  const marked = line
    .replaceAll('<span class="b">', "\u0001")
    .replaceAll("</span>", "\u0002");
  let accent = false;
  const cells = [];
  for (const character of marked) {
    if (character === "\u0001") {
      accent = true;
    } else if (character === "\u0002") {
      accent = false;
    } else {
      cells.push(encodeCell(character, accent));
    }
  }
  return cells;
}

function buildNormaFrames(sourceLogoPath) {
  const source = fs.readFileSync(sourceLogoPath);
  const sourceDigest = createHash("sha256").update(source).digest("hex");
  if (sourceDigest !== normaLogoSha256) {
    throw new Error(
      `Norma logo hash mismatch; expected ${normaLogoSha256}, received ${sourceDigest}`,
    );
  }
  return buildAsciiMorphFrames(resizePngRgba(source, 82, rows), 82, normaLogoCell);
}

function buildCodexBrandFrames(sourcePath) {
  return buildAsciiMorphFrames(
    resizePngRgba(fs.readFileSync(sourcePath), 60, rows, { preserveDisplayAspect: true }),
    60,
    codexBrandCell,
  );
}

function buildClaudeCodeBrandFrames(sourcePath) {
  return buildAsciiMorphFrames(
    resizePngRgba(fs.readFileSync(sourcePath), 50, rows, { preserveDisplayAspect: true }),
    50,
    claudeCodeBrandCell,
  );
}

function buildCursorBrandFrames() {
  const base = Array.from({ length: rows }, () => Array(columns).fill(encodeCell(" ", false)));
  const left = 20;
  const width = 60;
  const height = rows;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const nx = (x + 0.5) / width;
      const ny = (y + 0.5) / height;
      const hex = ny >= Math.abs(nx - 0.5) * 0.56 && ny <= 1 - Math.abs(nx - 0.5) * 0.56;
      if (!hex) continue;
      const arrow = nx > 0.12 && nx < 0.85 && ny > 0.20 && ny < 0.83
        && ny < 0.46 + (nx - 0.12) * 0.52
        && ny > 0.32 - (nx - 0.12) * 0.04;
      const boundary = Math.min(
        ny - Math.abs(nx - 0.5) * 0.56,
        1 - Math.abs(nx - 0.5) * 0.56 - ny,
      ) < 0.075;
      const glyphs = "·.:=+*#%@";
      const glyph = glyphs[Math.min(glyphs.length - 1, Math.floor((1 - ny * 0.35) * glyphs.length))];
      base[y][x + left] = encodeCell(glyph, arrow && !boundary);
    }
  }
  return buildAsciiMorphFramesFromBase(base);
}

function resizePngRgba(input, targetWidth, targetHeight, options = {}) {
  if (input.toString("hex", 0, 8) !== "89504e470d0a1a0a") {
    throw new Error("Norma logo must be a PNG");
  }
  let offset = 8;
  let width;
  let height;
  let channels;
  const idat = [];
  while (offset < input.length) {
    const length = input.readUInt32BE(offset);
    const type = input.toString("ascii", offset + 4, offset + 8);
    const data = input.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      if (data[8] !== 8 || ![2, 6].includes(data[9]) || data[12] !== 0) {
        throw new Error("source image must be a non-interlaced 8-bit RGB or RGBA PNG");
      }
      channels = data[9] === 6 ? 4 : 3;
    } else if (type === "IDAT") {
      idat.push(data);
    }
    offset += length + 12;
  }
  if (!width || !height || idat.length === 0) throw new Error("Norma logo PNG is incomplete");

  const rowBytes = width * channels;
  const filtered = inflateSync(Buffer.concat(idat));
  if (filtered.length !== height * (rowBytes + 1)) throw new Error("unexpected PNG data length");
  const decoded = Buffer.alloc(width * height * 4);
  let sourceOffset = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = filtered[sourceOffset];
    sourceOffset += 1;
    for (let x = 0; x < rowBytes; x += 1) {
      const raw = filtered[sourceOffset + x];
      const left = x >= channels ? decoded[y * rowBytes + x - channels] : 0;
      const up = y > 0 ? decoded[(y - 1) * rowBytes + x] : 0;
      const upLeft = y > 0 && x >= channels ? decoded[(y - 1) * rowBytes + x - channels] : 0;
      decoded[y * rowBytes + x] = unfilterPngByte(filter, raw, left, up, upLeft);
    }
    sourceOffset += rowBytes;
  }

  const { preserveDisplayAspect = false } = options;
  const cellWidth = 8;
  const lineHeight = 16;
  const scale = preserveDisplayAspect
    ? Math.min(targetWidth * cellWidth / width, targetHeight * lineHeight / height)
    : null;
  const renderedWidth = preserveDisplayAspect
    ? Math.max(1, Math.min(targetWidth, Math.round(width * scale / cellWidth)))
    : targetWidth;
  const renderedHeight = preserveDisplayAspect
    ? Math.max(1, Math.min(targetHeight, Math.round(height * scale / lineHeight)))
    : targetHeight;
  const horizontalInset = Math.floor((targetWidth - renderedWidth) / 2);
  const verticalInset = Math.floor((targetHeight - renderedHeight) / 2);
  const pixels = [];
  for (let y = 0; y < targetHeight; y += 1) {
    const row = [];
    for (let x = 0; x < targetWidth; x += 1) {
      if (
        x < horizontalInset || x >= horizontalInset + renderedWidth
        || y < verticalInset || y >= verticalInset + renderedHeight
      ) {
        row.push({ r: 255, g: 255, b: 255 });
        continue;
      }
      const sourceX = Math.min(
        width - 1,
        Math.floor((x - horizontalInset + 0.5) * width / renderedWidth),
      );
      const sourceY = Math.min(
        height - 1,
        Math.floor((y - verticalInset + 0.5) * height / renderedHeight),
      );
      const index = (sourceY * width + sourceX) * channels;
      const alpha = channels === 4 ? decoded[index + 3] / 255 : 1;
      row.push({
        r: Math.round(decoded[index] * alpha + 255 * (1 - alpha)),
        g: Math.round(decoded[index + 1] * alpha + 255 * (1 - alpha)),
        b: Math.round(decoded[index + 2] * alpha + 255 * (1 - alpha)),
      });
    }
    pixels.push(row);
  }
  return pixels;
}

function unfilterPngByte(filter, raw, left, up, upLeft) {
  if (filter === 0) return raw;
  if (filter === 1) return (raw + left) & 0xff;
  if (filter === 2) return (raw + up) & 0xff;
  if (filter === 3) return (raw + Math.floor((left + up) / 2)) & 0xff;
  if (filter === 4) return (raw + paeth(left, up, upLeft)) & 0xff;
  throw new Error(`unsupported PNG filter ${filter}`);
}

function paeth(left, up, upLeft) {
  const estimate = left + up - upLeft;
  const leftDistance = Math.abs(estimate - left);
  const upDistance = Math.abs(estimate - up);
  const upLeftDistance = Math.abs(estimate - upLeft);
  if (leftDistance <= upDistance && leftDistance <= upLeftDistance) return left;
  if (upDistance <= upLeftDistance) return up;
  return upLeft;
}

function buildAsciiMorphFrames(pixels, sourceWidth, cellForPixel) {
  const base = Array.from({ length: rows }, () => Array(columns).fill(encodeCell(" ", false)));
  const left = Math.floor((columns - sourceWidth) / 2);
  for (let y = 0; y < rows; y += 1) {
    for (let x = 0; x < sourceWidth; x += 1) {
      base[y][x + left] = cellForPixel(pixels[y][x]);
    }
  }

  return buildAsciiMorphFramesFromBase(base);
}

function buildAsciiMorphFramesFromBase(base) {
  const output = Buffer.alloc(frames * rows * columns);
  for (let frame = 0; frame < frames; frame += 1) {
    for (let y = 0; y < rows; y += 1) {
      for (let x = 0; x < columns; x += 1) {
        output[(frame * rows + y) * columns + x] = morphCell(base[y][x], x, y, frame);
      }
    }
  }
  return output;
}

function normaLogoCell(pixel) {
  const max = Math.max(pixel.r, pixel.g, pixel.b);
  const min = Math.min(pixel.r, pixel.g, pixel.b);
  const average = (pixel.r + pixel.g + pixel.b) / 3;
  if (average > 210 && max - min < 35) return encodeCell("X", true);
  if (max >= 86) return encodeCell(" ", false);
  const glyphs = "·.:=+*#%@";
  const strength = 1 - max / 86;
  return encodeCell(glyphs[Math.min(glyphs.length - 1, Math.floor(strength * glyphs.length))], false);
}

function codexBrandCell(pixel) {
  const max = Math.max(pixel.r, pixel.g, pixel.b);
  const min = Math.min(pixel.r, pixel.g, pixel.b);
  const average = (pixel.r + pixel.g + pixel.b) / 3;
  if (max - min < 42 || pixel.b < 150 || pixel.b <= pixel.r * 1.15) {
    return encodeCell(" ", false);
  }
  const glyphs = "·.:=+*#%@";
  const glyph = glyphs[Math.min(glyphs.length - 1, Math.floor((1 - average / 255) * glyphs.length))];
  return encodeCell(glyph, average >= 155);
}

function claudeCodeBrandCell(pixel) {
  const average = (pixel.r + pixel.g + pixel.b) / 3;
  if (average < 56) return encodeCell("#", false);
  if (pixel.r > pixel.g * 1.35 && pixel.r > pixel.b * 1.55 && pixel.g > 55) {
    const glyphs = "·.:=+*#%@";
    const glyph = glyphs[Math.min(glyphs.length - 1, Math.floor((1 - average / 255) * glyphs.length))];
    return encodeCell(glyph, true);
  }
  return encodeCell(" ", false);
}

function morphCell(cell, x, y, frame) {
  if ((cell & 0x7f) === 0x20) return cell;
  if (frame >= 29) return cell;
  const dx = (x - (columns - 1) / 2) / ((columns - 1) / 2);
  const dy = (y - (rows - 1) / 2) / ((rows - 1) / 2);
  const distance = Math.hypot(dx, dy);
  const progress = frame / 29;
  const threshold = progress * 1.55 - 0.05;
  const jitter = hash(x, y, 0) * 0.22 - 0.11;
  if (distance <= threshold + jitter) return cell;
  if (distance <= threshold + jitter + 0.13) {
    const noise = "·.+~";
    return encodeCell(noise[Math.floor(hash(x, y, frame) * noise.length)], (cell & accentBit) !== 0);
  }
  return encodeCell(" ", false);
}

function hash(x, y, frame) {
  let value = Math.imul(x + 11, 374761393) ^ Math.imul(y + 7, 668265263) ^ Math.imul(frame + 3, 2246822519);
  value = Math.imul(value ^ (value >>> 13), 1274126177);
  return ((value ^ (value >>> 16)) >>> 0) / 0x1_0000_0000;
}

function encodeCell(character, accent) {
  let code;
  if (character === "·") {
    code = middleDot;
  } else {
    code = character.codePointAt(0);
  }
  if (code === undefined || code > middleDot) {
    throw new Error(`character ${JSON.stringify(character)} cannot fit the compact ASCII asset`);
  }
  return code | (accent ? accentBit : 0);
}
