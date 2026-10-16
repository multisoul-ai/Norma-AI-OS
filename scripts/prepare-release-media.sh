#!/usr/bin/env bash
# Non-notarized packaging defaults to full; release-local.sh --notarize defaults to light.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
LOCAL_TEST=false
if [ "${1:-}" = "--local-test" ] && [ "$#" -eq 1 ]; then
  LOCAL_TEST=true
elif [ "$#" -ne 0 ]; then
  echo "usage: prepare-release-media.sh [--local-test]" >&2
  exit 1
fi
PROFILE="${NORMA_MEDIA_PROFILE:-full}"
case "$PROFILE" in
  full|wallpapers-on-demand|light) ;;
  *) echo "invalid NORMA_MEDIA_PROFILE: $PROFILE" >&2; exit 1 ;;
esac
if [ "$PROFILE" != full ] && [ "$LOCAL_TEST" = false ]; then
  python3 scripts/verify-release-media.py --profile "$PROFILE"
fi
python3 scripts/prepare-release-media.py --profile "$PROFILE"
