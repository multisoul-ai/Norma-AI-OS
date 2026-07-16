#!/usr/bin/env bash
# Launchpad 高清实验入口契约：基准计划必须覆盖当前基线、同源分辨率、编码和帧率变量。
#
# 数据构造（含关键数值的推导过程）：
#   baseline       = 1280x720 / H.264 / 24fps / current overlay
#   resolution set = 720p + 1080p + 1440p + 2160p（四档，同源、同编码、同帧率）
#   codec pair     = H.264 + HEVC CRF20 + HEVC CRF18 at 1440p/30fps
#                    （CRF20 比体积；CRF18 逼近 H.264 CRF18 质量）
#   fps pair       = HEVC 1440p at 30fps + 24fps（只改变帧率）
#
# 执行过程（逐步说明系统如何处理）：
#   1. 调用 run-launchpad-hd-benchmark.sh --plan 输出实验矩阵
#   2. 逐项匹配必须存在的精确计划行
#   3. 检查计划不得包含 GIF，避免把有损调色板路径混入视频低拷贝实验
#   4. 真实编码一个 64x36 / 1fps 的 H.264 冒烟文件，覆盖 Bash 3.2 空 codec 参数路径
#   5. dry-run 静态图矩阵，确认 720p–4K PNG 与 1440p/4K JPEG 来自同一 4K 源
#
# 预期结果：
#   - 正断言：四档同源分辨率、1440p 编码/质量对和帧率对全部存在
#   - 正断言：当前 home.mp4 基线被单独记录
#   - 正断言：H.264 冒烟编码成功输出非空文件
#   - 正断言：静态图矩阵保留原始 4K PNG，并生成高质量 4K JPEG
#   - 负断言：计划中不存在 GIF 方案

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNNER="$ROOT/scripts/run-launchpad-hd-benchmark.sh"

if [[ ! -x "$RUNNER" ]]; then
  echo "benchmark runner should exist and be executable: $RUNNER" >&2
  exit 1
fi

plan="$($RUNNER --plan)"

require_line() {
  local expected="$1"
  if ! grep -Fqx "$expected" <<<"$plan"; then
    echo "benchmark plan should contain exact row: $expected" >&2
    exit 1
  fi
}

require_line $'baseline-current\thome.mp4\t1280x720\th264\t24\tcurrent'
require_line $'resolution-720\tsource-720-h264.mp4\t1280x720\th264\t30\tbalanced'
require_line $'resolution-1080\tsource-1080-h264.mp4\t1920x1080\th264\t30\tbalanced'
require_line $'resolution-1440\tsource-1440-h264.mp4\t2560x1440\th264\t30\tbalanced'
require_line $'resolution-2160\tsource-2160-h264.mp4\t3840x2160\th264\t30\tbalanced'
require_line $'codec-1440-hevc\tsource-1440-hevc-30.mp4\t2560x1440\thevc\t30\tbalanced'
require_line $'codec-1440-hevc-crf18\tsource-1440-hevc-crf18-30.mp4\t2560x1440\thevc-crf18\t30\tbalanced'
require_line $'fps-1440-hevc-24\tsource-1440-hevc-24.mp4\t2560x1440\thevc\t24\tbalanced'

if grep -Eqi 'gif' <<<"$plan"; then
  echo "benchmark plan must not include GIF because it bypasses the live 420f surface path" >&2
  exit 1
fi

dry_run="$($RUNNER --prepare-media --dry-run)"

for required in \
  'scale=1280:720' \
  'scale=1920:1080' \
  'scale=2560:1440' \
  'libx264' \
  'libx265' \
  'source-1440-hevc-crf18-30.mp4' \
  'fps=24'; do
  if ! grep -Fq "$required" <<<"$dry_run"; then
    echo "media preparation dry-run should expose controlled variable: $required" >&2
    exit 1
  fi
done

if grep -Eqi 'scale=3840:2160.*source-2160-h264' <<<"$dry_run"; then
  echo "2160p H.264 reference must remain the original source instead of being re-encoded" >&2
  exit 1
fi

image_dry_run="$($RUNNER --prepare-images --dry-run)"
for required in \
  'source-720.png' \
  'source-1080.png' \
  'source-1440.png' \
  'source-2160.png' \
  'source-1440-q2.jpg' \
  'source-2160-q2.jpg' \
  'yuvj444p'; do
  if ! grep -Fq "$required" <<<"$image_dry_run"; then
    echo "image preparation dry-run should expose controlled candidate: $required" >&2
    exit 1
  fi
done

if grep -Eqi 'scale=3840:2160.*source-2160.png' <<<"$image_dry_run"; then
  echo "2160p PNG reference must remain the original source instead of being resampled" >&2
  exit 1
fi

smoke_root="$(mktemp -d /tmp/norma-hd-benchmark-smoke.XXXXXX)"
trap 'rm -rf "$smoke_root"' EXIT
NORMA_HD_ARTIFACT_ROOT="$smoke_root" "$RUNNER" --smoke-encode >/dev/null
if [[ ! -s "$smoke_root/media/smoke-h264.mp4" ]]; then
  echo "H.264 smoke encode should produce a non-empty output file" >&2
  exit 1
fi

echo "launchpad HD benchmark plan contract passed"
