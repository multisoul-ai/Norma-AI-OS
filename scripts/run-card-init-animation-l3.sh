#!/usr/bin/env bash
# Real packaged-app L3 smoke for Terminal / Agent card initialization animation.
#
# Prerequisites:
#   cargo build --release -p app
#   cargo packager -c Packager.dev.toml
#   Grant this shell Accessibility permission (System Events) and Screen Recording.
#
# Usage:
#   bash scripts/run-card-init-animation-l3.sh [app] [output-dir] [workspace] [terminal|agent]

set -euo pipefail

APP_PATH="${1:-build/dev/Norma OS.app}"
OUTPUT_DIR="${2:-artifacts/l3-card-init-animation}"
WORKSPACE_ROOT="${3:-$PWD}"
CARD_KIND="${4:-terminal}"

[ -d "$APP_PATH" ] || {
  echo "未找到 .app：$APP_PATH；先运行 cargo packager -c Packager.dev.toml" >&2
  exit 1
}
[ -d "$WORKSPACE_ROOT" ] || {
  echo "workspace 必须是存在的目录：$WORKSPACE_ROOT" >&2
  exit 1
}
case "$CARD_KIND" in
terminal | agent) ;;
*)
  echo "卡片类型只能是 terminal 或 agent，实际为：$CARD_KIND" >&2
  exit 1
  ;;
esac

mkdir -p "$OUTPUT_DIR"

record_failure() {
  local exit_code=$?
  {
    printf 'result=failed\n'
    printf 'exit_code=%s\n' "$exit_code"
    printf 'hint=Grant the invoking shell Accessibility and Screen Recording permission, then rerun.\n'
  } > "$OUTPUT_DIR/l3-run-result.txt"
  exit "$exit_code"
}
trap record_failure ERR

# `open -n` 强制启动刚打包的独立 .app 实例；不复用开发期 cargo run 进程。
open -n "$APP_PATH"
sleep 2

# 通过真实文件选择器进入 workspace，再从真实菜单/快捷键创建卡片。
# 该段需要 macOS 为当前终端授予辅助功能权限；若没有，osascript 会明确失败。
osascript - "$WORKSPACE_ROOT" "$CARD_KIND" "$OUTPUT_DIR" <<'APPLESCRIPT'
on run argv
    set workspacePath to item 1 of argv
    set cardKind to item 2 of argv
    set outputDirectory to item 3 of argv
    tell application "Norma OS" to activate
    delay 0.5
    tell application "System Events"
        tell process "Norma OS"
            keystroke "o" using {command down}
            delay 0.8
            keystroke "g" using {command down, shift down}
            delay 0.4
            keystroke workspacePath
            key code 36
            delay 0.8
            key code 36
            delay 2
            if cardKind is "terminal" then
                keystroke "1" using {command down}
            else
                click menu item "New Codex Agent" of menu 1 of menu bar item "Norma OS" of menu bar 1
            end if
            -- Keep the first capture in this same AppleScript transaction as
            -- the creation action; it avoids an extra shell round trip before
            -- the nominal 000ms capture.
            do shell script "/usr/sbin/screencapture -x " & quoted form of (outputDirectory & "/" & cardKind & "-000ms.png")
            delay 0.5
            do shell script "/usr/sbin/screencapture -x " & quoted form of (outputDirectory & "/" & cardKind & "-500ms.png")
            delay 0.5
            do shell script "/usr/sbin/screencapture -x " & quoted form of (outputDirectory & "/" & cardKind & "-1000ms.png")
        end tell
    end tell
end run
APPLESCRIPT

{
  printf 'result=passed\n'
  printf 'card_kind=%s\n' "$CARD_KIND"
  printf 'capture_schedule=creation-action then immediate, +500ms, +1000ms\n'
  printf 'note=000ms is initiated in the same System Events transaction; macOS capture latency still applies.\n'
} > "$OUTPUT_DIR/l3-run-result.txt"

echo "L3 截图与执行记录已写入：$OUTPUT_DIR"
