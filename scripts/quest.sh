#!/usr/bin/env bash
set -euo pipefail
quest_monorepo=$(cd "$(dirname "$0")/.." && pwd)
exec bash "$quest_monorepo/apps/quest/scripts/quest.sh" "$@"
