#!/usr/bin/env bash
# PROTOTYPE — Launchpad HD benchmark; do not ship this runner in production.
#
# Question: which resolution/codec/frame-rate/overlay combination materially
# improves Launchpad fidelity while keeping real macOS playback efficient?
#
# Media-generation entry point:
#   bash scripts/run-launchpad-hd-benchmark.sh --prepare-media

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARTIFACT_ROOT="${NORMA_HD_ARTIFACT_ROOT:-/tmp/norma-launchpad-hd-benchmark}"
MEDIA_DIR="$ARTIFACT_ROOT/media"
IMAGE_DIR="$ARTIFACT_ROOT/image-media"
SOURCE_VIDEO="$ROOT/crates/app/assets/wallpapers/1406443.mp4"
SOURCE_IMAGE="$ROOT/crates/app/assets/wallpapers/1411385.png"

print_plan() {
  printf '%s\n' \
    $'baseline-current\thome.mp4\t1280x720\th264\t24\tcurrent' \
    $'resolution-720\tsource-720-h264.mp4\t1280x720\th264\t30\tbalanced' \
    $'resolution-1080\tsource-1080-h264.mp4\t1920x1080\th264\t30\tbalanced' \
    $'resolution-1440\tsource-1440-h264.mp4\t2560x1440\th264\t30\tbalanced' \
    $'resolution-2160\tsource-2160-h264.mp4\t3840x2160\th264\t30\tbalanced' \
    $'codec-1440-hevc\tsource-1440-hevc-30.mp4\t2560x1440\thevc\t30\tbalanced' \
    $'codec-1440-hevc-crf18\tsource-1440-hevc-crf18-30.mp4\t2560x1440\thevc-crf18\t30\tbalanced' \
    $'fps-1440-hevc-24\tsource-1440-hevc-24.mp4\t2560x1440\thevc\t24\tbalanced'
}

ffmpeg_path() {
  if [[ -n "${NORMA_HD_FFMPEG:-}" ]]; then
    printf '%s\n' "$NORMA_HD_FFMPEG"
    return
  fi
  if command -v ffmpeg >/dev/null 2>&1; then
    command -v ffmpeg
    return
  fi

  local isolated_python=/tmp/norma-hd-benchmark-python
  if ! PYTHONPATH="$isolated_python" python3 -c 'import imageio_ffmpeg' >/dev/null 2>&1; then
    python3 -m pip install \
      --disable-pip-version-check \
      --target "$isolated_python" \
      imageio-ffmpeg==0.6.0 >&2
  fi
  PYTHONPATH="$isolated_python" python3 -c \
    'import imageio_ffmpeg; print(imageio_ffmpeg.get_ffmpeg_exe())'
}

encode_variant() {
  local ffmpeg="$1"
  local output="$2"
  local width="$3"
  local height="$4"
  local codec="$5"
  local fps="$6"
  local dry_run="$7"
  local encoder quality

  case "$codec" in
    h264)
      encoder=libx264
      quality=18
      ;;
    hevc)
      encoder=libx265
      quality=20
      ;;
    hevc-crf18)
      encoder=libx265
      quality=18
      ;;
    *)
      echo "unsupported codec: $codec" >&2
      exit 2
      ;;
  esac

  if [[ "$dry_run" == true ]]; then
    printf 'ffmpeg -i %s -vf scale=%s:%s:flags=lanczos,fps=%s -c:v %s -crf %s -pix_fmt yuv420p %s\n' \
      "$SOURCE_VIDEO" "$width" "$height" "$fps" "$encoder" "$quality" "$output"
    return
  fi

  local args=(
    -nostdin \
    -hide_banner \
    -loglevel error \
    -stats \
    -y \
    -i "$SOURCE_VIDEO" \
    -map 0:v:0 \
    -an \
    -vf "scale=${width}:${height}:flags=lanczos,fps=${fps}" \
    -c:v "$encoder" \
    -preset fast \
    -crf "$quality" \
    -pix_fmt yuv420p \
    -color_primaries bt709 \
    -color_trc bt709 \
    -colorspace bt709 \
    -movflags +faststart
  )
  if [[ "$codec" == hevc || "$codec" == hevc-crf18 ]]; then
    args+=(-tag:v hvc1 -x265-params log-level=error)
  fi
  args+=("$output")
  "$ffmpeg" "${args[@]}"
}

prepare_media() {
  local dry_run=false
  if [[ "${1:-}" == --dry-run ]]; then
    dry_run=true
  fi

  if [[ ! -f "$SOURCE_VIDEO" ]]; then
    echo "4K source video is missing: $SOURCE_VIDEO" >&2
    exit 1
  fi

  local ffmpeg=ffmpeg
  if [[ "$dry_run" == false ]]; then
    mkdir -p "$MEDIA_DIR"
    ffmpeg="$(ffmpeg_path)"
  fi

  encode_variant "$ffmpeg" "$MEDIA_DIR/source-720-h264.mp4" 1280 720 h264 30 "$dry_run"
  encode_variant "$ffmpeg" "$MEDIA_DIR/source-1080-h264.mp4" 1920 1080 h264 30 "$dry_run"
  encode_variant "$ffmpeg" "$MEDIA_DIR/source-1440-h264.mp4" 2560 1440 h264 30 "$dry_run"

  if [[ "$dry_run" == true ]]; then
    printf 'copy original %s %s\n' "$SOURCE_VIDEO" "$MEDIA_DIR/source-2160-h264.mp4"
  else
    cp -p "$SOURCE_VIDEO" "$MEDIA_DIR/source-2160-h264.mp4"
  fi

  encode_variant "$ffmpeg" "$MEDIA_DIR/source-1440-hevc-30.mp4" 2560 1440 hevc 30 "$dry_run"
  encode_variant "$ffmpeg" "$MEDIA_DIR/source-1440-hevc-crf18-30.mp4" 2560 1440 hevc-crf18 30 "$dry_run"
  encode_variant "$ffmpeg" "$MEDIA_DIR/source-1440-hevc-24.mp4" 2560 1440 hevc 24 "$dry_run"
}

encode_image_variant() {
  local ffmpeg="$1"
  local output="$2"
  local width="$3"
  local height="$4"
  local format="$5"
  local dry_run="$6"

  if [[ "$dry_run" == true ]]; then
    if [[ "$format" == jpeg ]]; then
      printf 'ffmpeg -i %s -vf scale=%s:%s:flags=lanczos -q:v 2 -pix_fmt yuvj444p %s\n' \
        "$SOURCE_IMAGE" "$width" "$height" "$output"
    else
      printf 'ffmpeg -i %s -vf scale=%s:%s:flags=lanczos %s\n' \
        "$SOURCE_IMAGE" "$width" "$height" "$output"
    fi
    return
  fi

  local args=(
    -nostdin
    -hide_banner
    -loglevel error
    -y
    -i "$SOURCE_IMAGE"
    -frames:v 1
    -vf "scale=${width}:${height}:flags=lanczos"
  )
  if [[ "$format" == jpeg ]]; then
    args+=(-q:v 2 -pix_fmt yuvj444p)
  fi
  args+=("$output")
  "$ffmpeg" "${args[@]}"
}

prepare_images() {
  local dry_run=false
  if [[ "${1:-}" == --dry-run ]]; then
    dry_run=true
  fi

  if [[ ! -f "$SOURCE_IMAGE" ]]; then
    echo "4K source image is missing: $SOURCE_IMAGE" >&2
    exit 1
  fi

  local ffmpeg=ffmpeg
  if [[ "$dry_run" == false ]]; then
    mkdir -p "$IMAGE_DIR"
    ffmpeg="$(ffmpeg_path)"
  fi

  encode_image_variant "$ffmpeg" "$IMAGE_DIR/source-720.png" 1280 720 png "$dry_run"
  encode_image_variant "$ffmpeg" "$IMAGE_DIR/source-1080.png" 1920 1080 png "$dry_run"
  encode_image_variant "$ffmpeg" "$IMAGE_DIR/source-1440.png" 2560 1440 png "$dry_run"

  if [[ "$dry_run" == true ]]; then
    printf 'copy original %s %s\n' "$SOURCE_IMAGE" "$IMAGE_DIR/source-2160.png"
  else
    cp -p "$SOURCE_IMAGE" "$IMAGE_DIR/source-2160.png"
  fi

  encode_image_variant "$ffmpeg" "$IMAGE_DIR/source-1440-q2.jpg" 2560 1440 jpeg "$dry_run"
  encode_image_variant "$ffmpeg" "$IMAGE_DIR/source-2160-q2.jpg" 3840 2160 jpeg "$dry_run"
}

print_usage() {
  echo "usage: $0 --plan | --prepare-media [--dry-run] | --prepare-images [--dry-run] | --smoke-encode" >&2
}

case "${1:-}" in
  --plan)
    print_plan
    ;;
  --prepare-media)
    prepare_media "${2:-}"
    ;;
  --prepare-images)
    prepare_images "${2:-}"
    ;;
  --smoke-encode)
    mkdir -p "$MEDIA_DIR"
    encode_variant "$(ffmpeg_path)" "$MEDIA_DIR/smoke-h264.mp4" 64 36 h264 1 false
    ;;
  *)
    print_usage
    exit 2
    ;;
esac
