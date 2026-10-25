#!/usr/bin/env bash
# 桌面 E2E 薄入口：构建 SwiftPM 驱动并运行场景。
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
scenario="workspace-lifecycle"
repeat=1
mode=run

while [[ $# -gt 0 ]]; do
  case "$1" in
    --visual)
      mode=visual
      shift
      ;;
    --repeat)
      repeat="${2:?缺少 --repeat 的次数}"
      shift 2
      ;;
    --repeat=*)
      repeat="${1#*=}"
      shift
      ;;
    -h|--help)
      echo "用法：scripts/desktop-e2e.sh [module|scenario] [--repeat N] [--visual]"
      echo "--visual 由当前多模态 Agent 看图并写响应，当前支持 canvas-presentation-layout。"
      echo "默认场景：workspace-lifecycle"
      echo "传入模块名时，按该模块登记顺序跑完全部场景。"
      exit 0
      ;;
    *)
      scenario="$1"
      shift
      ;;
  esac
done

if ! [[ "$repeat" =~ ^[1-9][0-9]*$ ]]; then
  echo "--repeat 必须是正整数" >&2
  exit 2
fi

app_binary="$repo_root/target/debug/norma-os"
artifact_root="$repo_root/artifacts/desktop-workspace-e2e"
package="$repo_root/test/desktop-e2e"
build_path="$repo_root/target/desktop-e2e-spm"
# 驱动经 debug App 的测试控制通道收发键鼠与截图，不需要辅助功能/屏幕录制授权，
# 因此直接运行 SwiftPM 产物，无需打包或签名。
driver="$build_path/debug/norma-e2e"

if [[ ! -x "$app_binary" ]]; then
  echo "缺少 ${app_binary}；请先 cargo build -p app（debug 构建，不要使用 production-data 或 --release）" >&2
  exit 2
fi

swift build --package-path "$package" --build-path "$build_path" --configuration debug

list_file="$(mktemp)"
if ! "$driver" list "$scenario" >"$list_file"; then
  cat "$list_file" >&2
  rm -f "$list_file"
  exit 2
fi
scenarios=()
while IFS= read -r line; do
  [[ -n "$line" ]] && scenarios+=("$line")
done <"$list_file"
rm -f "$list_file"
if [[ ${#scenarios[@]} -eq 0 ]]; then
  echo "模块或场景 ${scenario} 没有可运行的用例" >&2
  exit 2
fi

passes=0
total=$(( ${#scenarios[@]} * repeat ))
echo "选择 ${scenario}，共 ${#scenarios[@]} 个场景，各连续 ${repeat} 次。证据目录 ${artifact_root}"
for ((i = 1; i <= repeat; i++)); do
  for scenario_name in "${scenarios[@]}"; do
    echo "===== ${scenario_name} 第 ${i}/${repeat} 次 ====="
    seconds_start=$(date +%s)
    set +e
    "$driver" "$mode" "$scenario_name" "$app_binary" "$artifact_root"
    code=$?
    set -e
    elapsed=$(( $(date +%s) - seconds_start ))
    echo "第 ${i} 次 ${scenario_name} 退出码 ${code}，墙钟 ${elapsed}s"
    if [[ "$code" -eq 0 ]]; then
      passes=$((passes + 1))
    elif [[ "$code" -eq 3 ]]; then
      echo "ENV 失败，停止连跑。已通过 ${passes}/${total}。" >&2
      exit 3
    fi
  done
done

echo "通过率 ${passes}/${total}"
[[ "$passes" -eq "$total" ]]
