#!/usr/bin/env bash
# Real packaged-app CPU sampler for the Norma OS P0/P1 performance gate.
#
# Record:
#   bash scripts/run-cpu-performance-l3.sh baseline --app "path/Norma OS.app" \
#     --scenario idle-seven-cards
#   bash scripts/run-cpu-performance-l3.sh candidate --app "path/Norma OS.app" \
#     --scenario idle-seven-cards
#
# Compare:
#   bash scripts/run-cpu-performance-l3.sh compare baseline-summary.json candidate-summary.json

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
gate_script="$repo_root/scripts/cpu-performance-gate.py"
default_output_root="$repo_root/artifacts/performance/cpu-p0-p1"

usage() {
  cat <<'USAGE'
usage:
  run-cpu-performance-l3.sh baseline|candidate [options]
  run-cpu-performance-l3.sh compare <baseline-summary.json> <candidate-summary.json> [gate-report.json]
  run-cpu-performance-l3.sh repeatability <reference-summary.json> <rerun-summary.json> [report.json]

record options:
  --app PATH                   packaged Norma OS.app to launch
  --pid PID                    sample an already prepared Norma OS process; derive its bundle
                               identity from the PID or mark it unverified
  --scenario NAME              idle-seven-cards, background-seven-cards,
                               one-active-terminal, two-active-terminals, or a manual name
  --duration SECONDS           sampling duration; release evidence must use 60 (default: 60)
  --output-root PATH           artifact root
  --commit SHA                 commit recorded for the sampled app
  --node-count N               required for manual scenarios
  --active-agent-count N       required metadata for manual Codex scenarios
  --active-terminal-count N    required metadata for manual terminal scenarios
  --no-prepare                 do not automate workspace/card creation
  --prepare-wait SECONDS       settle time after preparing the scene (default: 4)
  --sample-seconds SECONDS     /usr/bin/sample stack capture duration (default: 5)

Automatic preparation resolves the newly launched PID and tells System Events to operate
only that application process. The benchmark instance also uses an isolated HOME, so it can
coexist with a user's existing Norma OS window without sharing app storage.

Set NORMA_CPU_BENCHMARK_FRAME_DIAGNOSTICS=1 only for a separate frame-trace run. CPU
release evidence defaults to RUST_LOG=info without per-frame debug logging.
USAGE
}

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

require_positive_integer() {
  local name=$1
  local value=$2
  case "$value" in
    '' | *[!0-9]*) fail "$name must be a positive integer, got: $value" ;;
  esac
  [ "$value" -gt 0 ] || fail "$name must be greater than zero, got: $value"
}

if [ "$#" -lt 1 ]; then
  usage
  exit 64
fi

mode=$1
shift

if [ "$mode" = "compare" ]; then
  [ "$#" -ge 2 ] || {
    usage
    exit 64
  }
  baseline_summary=$1
  candidate_summary=$2
  gate_report=${3:-"$default_output_root/gate-report.json"}
  exec python3 "$gate_script" compare \
    --baseline "$baseline_summary" \
    --candidate "$candidate_summary" \
    --output "$gate_report" \
    --min-reduction-percent 50
fi

if [ "$mode" = "repeatability" ]; then
  [ "$#" -ge 2 ] || {
    usage
    exit 64
  }
  reference_summary=$1
  rerun_summary=$2
  repeatability_report=${3:-"$default_output_root/repeatability-report.json"}
  exec python3 "$gate_script" repeatability \
    --reference "$reference_summary" \
    --rerun "$rerun_summary" \
    --output "$repeatability_report" \
    --max-median-delta-percent 20
fi

case "$mode" in
  baseline | candidate) ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    usage
    exit 64
    ;;
esac

app_path="$repo_root/build/dev/Norma OS.app"
app_pid=
scenario=idle-seven-cards
duration_seconds=60
output_root=$default_output_root
sampled_commit=$(git -C "$repo_root" rev-parse HEAD)
node_count=
active_agent_count=
active_terminal_count=
automatic_prepare=true
launch_mode=runner-launch
frame_diagnostics=${NORMA_CPU_BENCHMARK_FRAME_DIAGNOSTICS:-0}
case "$frame_diagnostics" in
  0) frame_rust_log=${NORMA_CPU_BENCHMARK_RUST_LOG:-info} ;;
  1) frame_rust_log=${NORMA_CPU_BENCHMARK_RUST_LOG:-info,multisoul_frame=debug} ;;
  *) fail "NORMA_CPU_BENCHMARK_FRAME_DIAGNOSTICS must be 0 or 1, got: $frame_diagnostics" ;;
esac
metadata_frame_diagnostics=$frame_diagnostics
metadata_rust_log=$frame_rust_log
prepare_wait_seconds=4
stack_sample_seconds=5

while [ "$#" -gt 0 ]; do
  case "$1" in
    --app)
      [ "$#" -ge 2 ] || fail "--app requires a value"
      app_path=$2
      shift 2
      ;;
    --pid)
      [ "$#" -ge 2 ] || fail "--pid requires a value"
      app_pid=$2
      shift 2
      ;;
    --scenario)
      [ "$#" -ge 2 ] || fail "--scenario requires a value"
      scenario=$2
      shift 2
      ;;
    --duration)
      [ "$#" -ge 2 ] || fail "--duration requires a value"
      duration_seconds=$2
      shift 2
      ;;
    --output-root)
      [ "$#" -ge 2 ] || fail "--output-root requires a value"
      output_root=$2
      shift 2
      ;;
    --commit)
      [ "$#" -ge 2 ] || fail "--commit requires a value"
      sampled_commit=$2
      shift 2
      ;;
    --node-count)
      [ "$#" -ge 2 ] || fail "--node-count requires a value"
      node_count=$2
      shift 2
      ;;
    --active-agent-count)
      [ "$#" -ge 2 ] || fail "--active-agent-count requires a value"
      active_agent_count=$2
      shift 2
      ;;
    --active-terminal-count)
      [ "$#" -ge 2 ] || fail "--active-terminal-count requires a value"
      active_terminal_count=$2
      shift 2
      ;;
    --no-prepare)
      automatic_prepare=false
      shift
      ;;
    --prepare-wait)
      [ "$#" -ge 2 ] || fail "--prepare-wait requires a value"
      prepare_wait_seconds=$2
      shift 2
      ;;
    --sample-seconds)
      [ "$#" -ge 2 ] || fail "--sample-seconds requires a value"
      stack_sample_seconds=$2
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      fail "unknown option: $1"
      ;;
  esac
done

require_positive_integer "duration" "$duration_seconds"
require_positive_integer "prepare-wait" "$prepare_wait_seconds"
require_positive_integer "sample-seconds" "$stack_sample_seconds"
[ -x "$gate_script" ] || fail "CPU gate script is not executable: $gate_script"

case "$scenario" in
  idle-seven-cards)
    node_count=${node_count:-7}
    active_agent_count=${active_agent_count:-0}
    active_terminal_count=${active_terminal_count:-0}
    ;;
  background-seven-cards)
    node_count=${node_count:-7}
    active_agent_count=${active_agent_count:-0}
    active_terminal_count=${active_terminal_count:-0}
    ;;
  one-active-terminal)
    node_count=${node_count:-7}
    active_agent_count=${active_agent_count:-0}
    active_terminal_count=${active_terminal_count:-1}
    ;;
  two-active-terminals)
    node_count=${node_count:-7}
    active_agent_count=${active_agent_count:-0}
    active_terminal_count=${active_terminal_count:-2}
    ;;
  *)
    [ "$automatic_prepare" = false ] || fail "manual scenario requires --no-prepare: $scenario"
    [ -n "$node_count" ] || fail "manual scenario requires --node-count"
    [ -n "$active_agent_count" ] || fail "manual scenario requires --active-agent-count"
    [ -n "$active_terminal_count" ] || fail "manual scenario requires --active-terminal-count"
    ;;
esac

case "$node_count:$active_agent_count:$active_terminal_count" in
  *[!0-9:]* | *::* | :* | *:) fail "node/activity counts must be non-negative integers" ;;
esac

timestamp=$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "$output_root"
output_root=$(cd "$output_root" && pwd)
run_directory="$output_root/$timestamp-$mode-$scenario"
run_home="$run_directory/home"
mkdir -p "$run_home" "$run_directory"
benchmark_workspace="$run_directory/workspace"
mkdir -p "$benchmark_workspace"
app_cpu_csv="$run_directory/norma-cpu.csv"
window_server_cpu_csv="$run_directory/windowserver-cpu.csv"
summary_json="$run_directory/summary.json"
metadata_file="$run_directory/metadata.txt"
stack_sample_file="$run_directory/norma.sample.txt"
diagnostics_summary="$run_directory/frame-diagnostics-summary.txt"
run_result="$run_directory/run-result.txt"

launched_by_runner=false
cleanup() {
  if [ "$launched_by_runner" = true ] && [ -n "${app_pid:-}" ]; then
    if kill -0 "$app_pid" >/dev/null 2>&1; then
      kill -TERM "$app_pid" >/dev/null 2>&1 || true
    fi
  fi
}
trap cleanup EXIT INT TERM

find_new_norma_pid() {
  local existing_pids=$1
  local candidate
  while IFS= read -r candidate; do
    [ -n "$candidate" ] || continue
    case " $existing_pids " in
      *" $candidate "*) ;;
      *)
        printf '%s\n' "$candidate"
        return 0
        ;;
    esac
  done < <(pgrep -x norma-os || true)
  return 1
}

resolve_app_bundle_for_pid() {
  local process_pid=$1
  local bundle_path

  bundle_path=$(lsof -n -a -p "$process_pid" -d txt -Fn 2>/dev/null | \
    sed -n 's/^n//p' | \
    awk '
      /\/Contents\/MacOS\/[^/]+$/ && !found {
        sub(/\/Contents\/MacOS\/[^/]+$/, "")
        print
        found = 1
      }
    ')
  [ -n "$bundle_path" ] || return 1
  [ -d "$bundle_path" ] || return 1
  [ -f "$bundle_path/Contents/Info.plist" ] || return 1
  printf '%s\n' "$bundle_path"
}

requested_app_path=$app_path
observed_app_path=$app_path
app_identity_source=runner-launch

if [ -z "$app_pid" ]; then
  [ -d "$app_path" ] || fail "packaged app does not exist: $app_path"
  existing_pids=$(pgrep -x norma-os | tr '\n' ' ' || true)

  open -F -n \
    --env "HOME=$run_home" \
    --env "PATH=$PATH" \
    --env "MULTISOUL_FRAME_DIAGNOSTICS=$frame_diagnostics" \
    --env "RUST_LOG=$frame_rust_log" \
    --env "NORMA_SKIP_LICENSE=1" \
    --env "NORMA_CPU_BENCHMARK_SCENARIO=$scenario" \
    --env "NORMA_CPU_BENCHMARK_WORKSPACE=$benchmark_workspace" \
    "$app_path"
  launched_by_runner=true
  for _ in {1..100}; do
    app_pid=$(find_new_norma_pid "$existing_pids" || true)
    [ -n "$app_pid" ] && break
    sleep 0.1
  done
  [ -n "$app_pid" ] || fail "could not resolve the Norma OS PID launched from $app_path"
else
  automatic_prepare=false
  launch_mode=existing-pid
  metadata_frame_diagnostics=unverified
  metadata_rust_log=unverified
  if observed_app_path=$(resolve_app_bundle_for_pid "$app_pid"); then
    app_identity_source=pid-text-mapping
  else
    observed_app_path=unverified_existing_pid
    app_identity_source=unverified_existing_pid
  fi
fi

kill -0 "$app_pid" >/dev/null 2>&1 || fail "Norma OS PID is not running: $app_pid"

wait_for_benchmark_ready() {
  local process_pid=$1
  local log_path=$2
  local scene=$3
  local expected_nodes=$4
  local expected_active_agents=$5
  local expected_active_terminals=$6
  local ready_line=
  for _ in {1..200}; do
    kill -0 "$process_pid" >/dev/null 2>&1 || fail \
      "Norma OS exited before the benchmark scene became ready"
    if [ -f "$log_path" ]; then
      ready_line=$(rg 'cpu_benchmark_ready' "$log_path" | tail -1 || true)
    fi
    if [ -n "$ready_line" ]; then
      case "$ready_line" in
        *"scenario=\"$scene\""*"node_count=$expected_nodes"*"active_agent_count=$expected_active_agents"*"active_terminal_count=$expected_active_terminals"*)
          printf '%s\n' "$ready_line"
          return 0
          ;;
        *)
          fail "benchmark ready metadata does not match the requested scene: $ready_line"
          ;;
      esac
    fi
    sleep 0.1
  done
  fail "Norma OS did not report cpu_benchmark_ready within 20 seconds"
}

hide_benchmark_app() {
  local process_pid=$1
  osascript - "$process_pid" <<'APPLESCRIPT'
on run argv
    set targetPid to (item 1 of argv) as integer
    tell application "System Events"
        set appProcess to first application process whose unix id is targetPid
        set visible of appProcess to false
    end tell
end run
APPLESCRIPT
}

if [ "$automatic_prepare" = true ]; then
  benchmark_log="$run_home/.norma/logs/norma-os.$(date +%F).log"
  scene_evidence=$(
    wait_for_benchmark_ready \
      "$app_pid" \
      "$benchmark_log" \
      "$scenario" \
      "$node_count" \
      "$active_agent_count" \
      "$active_terminal_count"
  )
  printf '%s\n' "$scene_evidence" >"$run_directory/scene-evidence.txt"
  if [ "$scenario" = "background-seven-cards" ]; then
    hide_benchmark_app "$app_pid"
  fi
  sleep "$prepare_wait_seconds"
fi

window_server_pid=$(pgrep -x WindowServer | head -1)
[ -n "$window_server_pid" ] || fail "WindowServer process was not found"

printf 'timestamp_epoch,cpu\n' >"$app_cpu_csv"
printf 'timestamp_epoch,cpu\n' >"$window_server_cpu_csv"

sample_index=0
while [ "$sample_index" -lt "$duration_seconds" ]; do
  kill -0 "$app_pid" >/dev/null 2>&1 || fail "Norma OS exited during CPU sampling"
  sample_timestamp=$(date +%s)
  app_cpu=$(ps -p "$app_pid" -o %cpu= | tr -d '[:space:]')
  window_server_cpu=$(ps -p "$window_server_pid" -o %cpu= | tr -d '[:space:]')
  [ -n "$app_cpu" ] || fail "failed to read Norma OS CPU for PID $app_pid"
  [ -n "$window_server_cpu" ] || fail "failed to read WindowServer CPU"
  printf '%s,%s\n' "$sample_timestamp" "$app_cpu" >>"$app_cpu_csv"
  printf '%s,%s\n' "$sample_timestamp" "$window_server_cpu" >>"$window_server_cpu_csv"
  sample_index=$((sample_index + 1))
  sleep 1
done

/usr/bin/sample "$app_pid" "$stack_sample_seconds" -file "$stack_sample_file" >/dev/null 2>&1 || \
  fail "/usr/bin/sample failed for PID $app_pid"

host_seed=$(
  {
    ioreg -rd1 -c IOPlatformExpertDevice 2>/dev/null | awk -F'\"' '/IOPlatformUUID/{print $4}'
    sysctl -n hw.model
  } | shasum -a 256 | awk '{print $1}'
)
app_version=unverified_existing_pid
if [ "$app_identity_source" != "unverified_existing_pid" ] && [ -d "$observed_app_path" ]; then
  app_version=$(
    /usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
      "$observed_app_path/Contents/Info.plist" 2>/dev/null || printf 'unknown'
  )
fi

{
  printf 'stage=%s\n' "$mode"
  printf 'scenario=%s\n' "$scenario"
  printf 'commit=%s\n' "$sampled_commit"
  printf 'requested_app_path=%s\n' "$requested_app_path"
  printf 'observed_app_path=%s\n' "$observed_app_path"
  printf 'app_identity_source=%s\n' "$app_identity_source"
  printf 'app_version=%s\n' "$app_version"
  printf 'app_pid=%s\n' "$app_pid"
  printf 'duration_seconds=%s\n' "$duration_seconds"
  printf 'node_count=%s\n' "$node_count"
  printf 'active_agent_count=%s\n' "$active_agent_count"
  printf 'active_terminal_count=%s\n' "$active_terminal_count"
  printf 'launch_mode=%s\n' "$launch_mode"
  printf 'requested_frame_diagnostics=%s\n' "$frame_diagnostics"
  printf 'frame_diagnostics=%s\n' "$metadata_frame_diagnostics"
  printf 'rust_log=%s\n' "$metadata_rust_log"
  printf 'host_id=%s\n' "$host_seed"
  printf 'macos_product_version=%s\n' "$(sw_vers -productVersion)"
  printf 'macos_build_version=%s\n' "$(sw_vers -buildVersion)"
  printf 'hardware_model=%s\n' "$(sysctl -n hw.model)"
  printf 'logical_cpu=%s\n' "$(sysctl -n hw.logicalcpu)"
  printf 'memory_bytes=%s\n' "$(sysctl -n hw.memsize)"
  printf 'started_at_utc=%s\n' "$timestamp"
} >"$metadata_file"

log_file="$run_home/.norma/logs/norma-os.$(date +%F).log"
perf_log_args=()
if [ "$launch_mode" = "existing-pid" ]; then
  printf 'diagnostics_log=unverified_existing_pid\n' >"$diagnostics_summary"
elif [ -f "$log_file" ]; then
  perf_log_args=(--perf-log "$log_file")
  cp "$log_file" "$run_directory/frame-diagnostics.log"
  count_log_matches() {
    local pattern=$1
    local count
    count=$(rg -c "$pattern" "$log_file" || true)
    printf '%s\n' "${count:-0}"
  }
  {
    printf 'runtime_frame_ticks=%s\n' "$(count_log_matches 'runtime_frame_tick')"
    printf 'animation_frame_ticks=%s\n' "$(count_log_matches 'animation_frame_tick')"
    printf 'canvas_renders=%s\n' "$(count_log_matches 'canvas_render')"
    printf 'motion_samples=%s\n' "$(count_log_matches 'motion_sample')"
    printf 'repaint_summary_samples=%s\n' "$(count_log_matches 'frame_repaint_summary')"
    printf 'norma_perf_summary_samples=%s\n' "$(count_log_matches 'cpu_performance_summary')"
    printf 'norma_perf_high_cpu_diagnostics=%s\n' \
      "$(count_log_matches 'cpu_high_diagnostic_written')"
    printf 'last_repaint_summary=%s\n' \
      "$(rg 'frame_repaint_summary' "$log_file" | tail -1 || true)"
    printf 'last_norma_perf_summary=%s\n' \
      "$(rg 'cpu_performance_summary' "$log_file" | tail -1 || true)"
  } >"$diagnostics_summary"
else
  printf 'diagnostics_log=missing\n' >"$diagnostics_summary"
fi

python3 "$gate_script" summarize \
  --app-csv "$app_cpu_csv" \
  --window-server-csv "$window_server_cpu_csv" \
  "${perf_log_args[@]}" \
  --output "$summary_json" \
  --stage "$mode" \
  --scenario "$scenario" \
  --commit "$sampled_commit" \
  --host-id "$host_seed" \
  --duration-seconds "$duration_seconds" \
  --node-count "$node_count" \
  --active-agent-count "$active_agent_count" \
  --active-terminal-count "$active_terminal_count" >/dev/null

{
  printf 'result=passed\n'
  printf 'summary=%s\n' "$summary_json"
  printf 'stack_sample=%s\n' "$stack_sample_file"
  printf 'diagnostics=%s\n' "$diagnostics_summary"
} >"$run_result"

echo "CPU performance artifacts: $run_directory"
echo "Summary: $summary_json"
