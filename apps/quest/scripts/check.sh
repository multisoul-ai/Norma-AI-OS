#!/usr/bin/env bash
set -euo pipefail
quest_root=$(cd "$(dirname "$0")/.." && pwd)
source "$quest_root/scripts/environment.sh"
cd "$quest_root"
npm run test:host
./gradlew :core:test :app:assembleDebug :app:lintDebug --console=plain
