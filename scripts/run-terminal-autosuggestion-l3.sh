#!/usr/bin/env bash
# Real packaged-app L3 smoke for terminal autosuggestion and Tab completion rendering.
#
# Prerequisites:
#   cargo build --release -p app
#   cargo packager -c Packager.dev.toml
#   Grant this shell Accessibility permission (System Events) and Screen Recording.
#
# Usage:
#   bash scripts/run-terminal-autosuggestion-l3.sh [app] [output-dir] [workspace]

set -euo pipefail

APP_PATH="${1:-build/dev/Norma OS.app}"
OUTPUT_DIR="${2:-artifacts/l3-terminal-autosuggestion}"
WORKSPACE_ROOT="${3:-$PWD}"

[ -d "$APP_PATH" ] || {
  echo "未找到 .app：$APP_PATH；先运行 cargo packager -c Packager.dev.toml" >&2
  exit 1
}
[ -d "$WORKSPACE_ROOT" ] || {
  echo "workspace 必须是存在的目录：$WORKSPACE_ROOT" >&2
  exit 1
}

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

APP_DIR="$(cd "$APP_PATH" && pwd -P)"
APP_EXECUTABLE="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP_DIR/Contents/Info.plist")"
APP_BINARY="$APP_DIR/Contents/MacOS/$APP_EXECUTABLE"

# `open -n` guarantees this run exercises the requested packaged binary instead
# of reusing an installed app that happens to share the same bundle identifier.
open -n "$APP_DIR"
sleep 2

TARGET_PID="$(pgrep -f -x "$APP_BINARY" | tail -1)"
[ -n "$TARGET_PID" ] || {
  echo "未找到刚启动的打包 App 进程：$APP_BINARY" >&2
  exit 1
}

WINDOW_POSITION="$(osascript - "$TARGET_PID" <<'APPLESCRIPT'
on run argv
    set targetPid to (item 1 of argv) as integer
    tell application "System Events"
        tell first application process whose unix id is targetPid
            set frontmost to true
            set {windowX, windowY} to position of window 1
            return (windowX as text) & " " & (windowY as text)
        end tell
    end tell
end run
APPLESCRIPT
)"
read -r WINDOW_X WINDOW_Y <<< "$WINDOW_POSITION"
# Launchpad keeps the Open Project button at a fixed content inset from the
# standard 1280x800 app window. CGEvent is required because GPUI's Metal canvas
# is not exposed as an AX button to System Events.
OPEN_PROJECT_X=$((WINDOW_X + 135))
OPEN_PROJECT_Y=$((WINDOW_Y + 340))
/usr/bin/swift -e 'import CoreGraphics; import Foundation
let x = Double(CommandLine.arguments[1])!
let y = Double(CommandLine.arguments[2])!
let point = CGPoint(x: x, y: y)
let source = CGEventSource(stateID: .hidSystemState)
CGEvent(mouseEventSource: source, mouseType: .leftMouseDown, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
usleep(80_000)
CGEvent(mouseEventSource: source, mouseType: .leftMouseUp, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)' "$OPEN_PROJECT_X" "$OPEN_PROJECT_Y"
sleep 1

# Target the exact process ID because developers commonly have several Norma OS
# bundles registered under the same bundle identifier. The captures deliberately
# bracket the physical Tab key so the pre-Tab cursor cell and post-Tab completion
# prompt are both reviewable.
WORKSPACE_NAME="$(basename "$WORKSPACE_ROOT")"
osascript - "$TARGET_PID" "$WORKSPACE_ROOT" "$WORKSPACE_NAME" "$OUTPUT_DIR" <<'APPLESCRIPT'
on run argv
    set targetPid to (item 1 of argv) as integer
    set workspacePath to item 2 of argv
    set workspaceName to item 3 of argv
    set outputDirectory to item 4 of argv
    tell application "System Events"
        tell first application process whose unix id is targetPid
            set frontmost to true
            keystroke "g" using {command down, shift down}
            delay 0.4
            keystroke workspacePath
            key code 36
            delay 0.8
            key code 36
            delay 2
            if name of window 1 does not contain workspaceName then
                error "未进入目标 workspace；当前窗口：" & name of window 1 number 2
            end if
            keystroke "1" using {command down}
            delay 2
            keystroke "g"
            delay 1
            do shell script "/usr/sbin/screencapture -x " & quoted form of (outputDirectory & "/terminal-g.png")
            key code 48
            delay 1
            do shell script "/usr/sbin/screencapture -x " & quoted form of (outputDirectory & "/terminal-g-tab.png")
            keystroke "n"
            key code 8 using {control down}
            delay 0.2
            keystroke "printf 'g\\033[90mexit\\033[39m\\033[4D'; sleep 5"
            key code 36
            delay 1
            do shell script "/usr/sbin/screencapture -x " & quoted form of (outputDirectory & "/terminal-gray-cursor.png")
        end tell
    end tell
end run
APPLESCRIPT

{
  printf 'result=passed\n'
  printf 'target_pid=%s\n' "$TARGET_PID"
  printf 'workspace=%s\n' "$WORKSPACE_ROOT"
  printf 'screenshots=terminal-g.png,terminal-g-tab.png,terminal-gray-cursor.png\n'
} > "$OUTPUT_DIR/l3-run-result.txt"

echo "Terminal autosuggestion L3 截图与执行记录已写入：$OUTPUT_DIR"
