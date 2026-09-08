#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/environment.sh"
quest_failed=0
if java -version 2>&1; then :; else quest_failed=1; fi
if command -v node >/dev/null; then node --version; else echo '缺少 Node.js 22.13+'; quest_failed=1; fi
if [[ -n "${ANDROID_HOME:-}" && -f "$ANDROID_HOME/platforms/android-34/android.jar" ]]; then
  echo "Android SDK: $ANDROID_HOME"
else
  echo '缺少 Android API 34；请通过 Android Studio SDK Manager 安装。'
  quest_failed=1
fi
if command -v adb >/dev/null; then adb version; else echo '缺少 Android platform-tools'; quest_failed=1; fi
exit "$quest_failed"
