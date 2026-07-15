#!/usr/bin/env bash
# Real packaged-app L3 smoke for the macOS menu hierarchy and OpenCode creation path.
#
# Prerequisites:
#   cargo build --release -p app
#   cargo packager -c Packager.dev.toml
#   Grant this shell Accessibility permission (System Events) and Screen Recording.
#
# Usage:
#   bash scripts/run-menu-bar-l3.sh [app] [output-dir] [workspace]

set -euo pipefail

APP_PATH="${1:-build/dev/Norma OS.app}"
OUTPUT_DIR="${2:-artifacts/l3-menu-bar}"
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

# `open -n` 强制启动刚打包的独立 .app 实例；不复用开发期 cargo run 进程。
open -n "$APP_PATH"
sleep 2

# 先经真实 Open Project 选择器进入 Canvas；随后以 File › New 创建 OpenCode Agent，
# 并分别保存展开菜单和创建后的真实应用截图。
osascript - "$WORKSPACE_ROOT" "$OUTPUT_DIR" <<'APPLESCRIPT'
on run argv
    set workspacePath to item 1 of argv
    set outputDirectory to item 2 of argv
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

            click menu bar item "File" of menu bar 1
            delay 0.4
            click menu item "New" of menu 1 of menu bar item "File" of menu bar 1
            delay 0.4
            do shell script "/usr/sbin/screencapture -x " & quoted form of (outputDirectory & "/menu-hierarchy.png")

            click menu item "New OpenCode Agent" of menu 1 of menu item "New" of menu 1 of menu bar item "File" of menu bar 1
            delay 1
            do shell script "/usr/sbin/screencapture -x " & quoted form of (outputDirectory & "/opencode-created.png")
        end tell
    end tell
end run
APPLESCRIPT

{
  printf 'result=passed\n'
  printf 'screenshots=menu-hierarchy.png,opencode-created.png\n'
  printf 'menu_path=File > New > New OpenCode Agent\n'
} > "$OUTPUT_DIR/l3-run-result.txt"

echo "L3 菜单截图与执行记录已写入：$OUTPUT_DIR"
