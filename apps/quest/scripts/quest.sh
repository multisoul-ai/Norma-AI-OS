#!/usr/bin/env bash
set -euo pipefail
quest_root=$(cd "$(dirname "$0")/.." && pwd)
source "$quest_root/scripts/environment.sh"
cd "$quest_root"
quest_command="${1:-help}"
if [[ $# -gt 0 ]]; then shift; fi
case "$quest_command" in
  setup) npm ci; bash scripts/doctor.sh ;;
  doctor) bash scripts/doctor.sh ;;
  mock) exec node dev-host/main.mjs "$@" ;;
  test) npm run test:host; ./gradlew :core:test --console=plain "$@" ;;
  check) exec bash scripts/check.sh ;;
  build) exec ./gradlew :app:assembleDebug --console=plain "$@" ;;
  install|run)
    ./gradlew :app:assembleDebug --console=plain
    adb get-state >/dev/null
    adb install -r app/build/outputs/apk/debug/app-debug.apk
    if [[ "$quest_command" == run ]]; then
      adb reverse tcp:8787 tcp:8787
      adb shell am start -n ai.multisoul.norma.quest.debug/ai.multisoul.norma.quest.QuestActivity
    fi
    ;;
  logs) exec adb logcat -s NormaQuest AndroidRuntime ;;
  *) echo '用法：scripts/quest.sh {setup|doctor|mock|test|check|build|install|run|logs}'; [[ "$quest_command" == help ]] ;;
esac
