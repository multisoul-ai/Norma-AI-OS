#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROTOTYPE="$ROOT/docs/ion-fireworks.html"
ION_BROWSER_BIN="${ION_BROWSER_BIN:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"

fail() {
  printf 'ion-fireworks prototype check failed: %s\n' "$1" >&2
  exit 1
}

assert_contains() {
  local pattern="$1"
  local description="$2"
  rg -q --fixed-strings -- "$pattern" "$PROTOTYPE" || fail "$description"
}

assert_absent() {
  local pattern="$1"
  local description="$2"
  if rg -q --fixed-strings -- "$pattern" "$PROTOTYPE"; then
    fail "$description"
  fi
}

# 场景：离子派发原型必须从底部发射，并分别命中三张卡片中央。
#
# 数据构造（数量来自用户要求）：
#   launch_origin_count = Command Dock(1)
#   target_card_count   = Research(1) + Build(1) + Verify(1) = 3
#   expected_hit_count  = target_card_count(3)
#
# 执行过程：
#   1. 读取单文件 HTML，确认发射器、三张目标卡片与粒子 Canvas 存在。
#   2. 用真实浏览器执行自动发射序列，等待三个 projectile 生命周期结束。
#   3. 读取页面暴露的运行时验收状态，逐张确认命中点与实时卡片中心重合。
#
# 预期结果：
#   - Research、Build、Verify 三张卡片必须逐一存在。
#   - 三张卡片必须逐一进入命中状态，不能只用总数或循环等泛化结果代替。
#   - 每次命中的几何误差必须为 0.00px，证明落点就是卡片中央。
#   - 完成后发射按钮恢复可用，用户可以再次播放。
test -f "$PROTOTYPE" || fail "docs/ion-fireworks.html is missing"
assert_contains 'id="ion-field"' "particle canvas is missing"
assert_contains 'id="launch-control"' "bottom launch control is missing"
assert_contains 'data-target-card="research"' "Research target card is missing"
assert_contains 'data-target-card="build"' "Build target card is missing"
assert_contains 'data-target-card="verify"' "Verify target card is missing"
assert_contains 'aria-live="polite"' "launch status is not announced to assistive technology"
assert_contains 'function cardCenter(card)' "card-center resolver is missing"
assert_contains 'function cubicPoint(p0, p1, p2, p3, t)' "cubic projectile path is missing"
assert_contains 'function drawHelixTrail(projectile, eased, progress)' "dual-helix plasma trail is missing"
assert_contains 'function drawIonCore(projectile, head, tangent, progress)' "volumetric ion core is missing"
assert_contains 'function drawImpactBloom(ring, now)' "layered impact bloom is missing"
assert_contains 'function spawnFireworkBurst(card, color, cardIndex, point)' "firework-style impact burst is missing"
assert_contains 'function drawBurstSpark(particle, now)' "secondary firework sparkle renderer is missing"
assert_contains 'function buildArcLengthTable(card, sampleCount = 48)' "arc-length reparameterization is missing"
assert_contains 'function pointAtTravel(card, travel, table)' "constant-distance curve lookup is missing"
assert_contains 'function naturalFlightProgress(value)' "natural asymmetric flight profile is missing"
assert_contains '@media (prefers-reduced-motion: reduce)' "Reduced Motion fallback is missing"
assert_absent 'context.ellipse(0, 0, 6.5, 2.8' "legacy needle-shaped projectile is still present"
assert_absent 'function easeOutQuart(value)' "legacy front-loaded easing is still present"
assert_absent '<script src=' "prototype must not depend on external scripts"
assert_absent 'https://' "prototype must be fully self-contained"
assert_absent 'launchControl.disabled' "launch control must remain interruptible during motion"

test -x "$ION_BROWSER_BIN" || fail "Google Chrome is unavailable at $ION_BROWSER_BIN"

rendered="$("$ION_BROWSER_BIN" \
  --headless=new \
  --disable-gpu \
  --hide-scrollbars \
  --no-first-run \
  --no-default-browser-check \
  --window-size=1440,960 \
  --virtual-time-budget=5000 \
  --dump-dom \
  "file://$PROTOTYPE?autoplay=1&test-clock=1" 2>/dev/null)"

research_hit_count="$(rg -o 'data-target-card="research"[^>]*data-hit="true"[^>]*data-impact-error="0.00"' <<< "$rendered" | wc -l | tr -d ' ')"
build_hit_count="$(rg -o 'data-target-card="build"[^>]*data-hit="true"[^>]*data-impact-error="0.00"' <<< "$rendered" | wc -l | tr -d ' ')"
verify_hit_count="$(rg -o 'data-target-card="verify"[^>]*data-hit="true"[^>]*data-impact-error="0.00"' <<< "$rendered" | wc -l | tr -d ' ')"
speed_ratios="$(rg -o 'data-speed-ratio="[0-9.]+"' <<< "$rendered" | sed 's/[^0-9.]//g')"
flight_durations="$(rg -o 'data-flight-duration="[0-9]+"' <<< "$rendered" | sed 's/[^0-9]//g')"
burst_counts="$(rg -o 'data-burst-count="[0-9]+"' <<< "$rendered" | sed 's/[^0-9]//g')"

test "$research_hit_count" -eq 1 || fail "Research card was not hit at its center"
test "$build_hit_count" -eq 1 || fail "Build card was not hit at its center"
test "$verify_hit_count" -eq 1 || fail "Verify card was not hit at its center"
rg -q 'data-sequence="complete"' <<< "$rendered" || fail "launch sequence did not complete"
rg -q 'id="launch-control"[^>]*data-launching="false"' <<< "$rendered" || fail "launch control did not return to its idle state"

# 飞行速度验收：弧长重参数化后，只保留轻微的“前快后柔”非对称速度变化。
#   max_allowed_ratio = fastest_cruise_speed / slowest_cruise_speed = 2.40
#   ratio_count        = Research(1) + Build(1) + Verify(1) = 3
# 这里排除起飞/磁吸收束两端，只检查 8%–92% 的可见巡航段。
speed_ratio_count="$(wc -l <<< "$speed_ratios" | tr -d ' ')"
test "$speed_ratio_count" -eq 3 || fail "expected 3 projectile speed diagnostics, found $speed_ratio_count"
while IFS= read -r speed_ratio; do
  awk -v ratio="$speed_ratio" 'BEGIN { exit !(ratio >= 1.00 && ratio <= 2.40) }' \
    || fail "projectile cruise speed ratio $speed_ratio exceeds the 2.40 continuity budget"
done <<< "$speed_ratios"

# 烟花节奏验收：三枚离子快速升空，并在卡片中心形成足够清晰的双层爆发。
#
# 数据构造（阈值来自一次短促的庆祝反馈）：
#   minimum_flight_ms = 460ms  （保证弧线仍可辨识）
#   maximum_flight_ms = 560ms  （比旧版 700–860ms 至少缩短 140ms）
#   minimum_burst     = primary_shell(20) + secondary_stars(8) = 28 particles
#
# 执行过程：
#   1. 分别读取 Research、Build、Verify 的实际飞行时长与爆发粒子数。
#   2. 逐枚校验飞行时长，防止任一路径重新变得拖沓。
#   3. 逐张校验爆发规模，确保每个命中都具备烟花的径向层次。
#
# 预期结果：
#   - 三枚离子的飞行时长都在 460–560ms，不能只验证平均值。
#   - 三张卡片的爆发粒子都不少于 28 个，不能只验证总量。
flight_duration_count="$(wc -l <<< "$flight_durations" | tr -d ' ')"
test "$flight_duration_count" -eq 3 || fail "expected 3 flight duration diagnostics, found $flight_duration_count"
while IFS= read -r flight_duration; do
  awk -v duration="$flight_duration" 'BEGIN { exit !(duration >= 460 && duration <= 560) }' \
    || fail "projectile flight duration ${flight_duration}ms is outside the 460–560ms budget"
done <<< "$flight_durations"

burst_count_total="$(wc -l <<< "$burst_counts" | tr -d ' ')"
test "$burst_count_total" -eq 3 || fail "expected 3 firework burst diagnostics, found $burst_count_total"
while IFS= read -r burst_count; do
  awk -v count="$burst_count" 'BEGIN { exit !(count >= 28) }' \
    || fail "impact burst contains only $burst_count particles; expected at least 28"
done <<< "$burst_counts"

# 场景：系统开启 Reduced Motion 时保留因果反馈，但不得播放跨屏飞行。
#
# 数据构造（降级反馈数量来自三张目标卡片）：
#   dock_feedback_count = 1
#   target_pulse_count  = Research(1) + Build(1) + Verify(1) = 3
#   projectile_count    = 0
#
# 执行过程：
#   1. 通过查询参数强制启用 Reduced Motion，模拟系统辅助功能偏好。
#   2. 运行同一发射入口，由页面选择无位移的 Dock flash + card pulse 路径。
#   3. 等待三个卡片状态更新完毕并读取公开的 motion mode。
#
# 预期结果：
#   - 页面必须明确进入 reduced 模式，不能误跑完整飞行路径。
#   - 三张卡片仍逐一收到回执，保留派发因果与完成反馈。
rendered_reduced="$("$ION_BROWSER_BIN" \
  --headless=new \
  --disable-gpu \
  --hide-scrollbars \
  --no-first-run \
  --no-default-browser-check \
  --window-size=1440,960 \
  --virtual-time-budget=1600 \
  --dump-dom \
  "file://$PROTOTYPE?autoplay=1&test-clock=1&reduce-motion=1" 2>/dev/null)"

rg -q '<body[^>]*data-sequence="complete"[^>]*data-motion-mode="reduced"' <<< "$rendered_reduced" \
  || fail "Reduced Motion sequence did not complete in reduced mode"
rg -q 'data-target-card="research"[^>]*data-hit="true"' <<< "$rendered_reduced" \
  || fail "Research card did not receive Reduced Motion feedback"
rg -q 'data-target-card="build"[^>]*data-hit="true"' <<< "$rendered_reduced" \
  || fail "Build card did not receive Reduced Motion feedback"
rg -q 'data-target-card="verify"[^>]*data-hit="true"' <<< "$rendered_reduced" \
  || fail "Verify card did not receive Reduced Motion feedback"

printf 'ion-fireworks prototype checks passed\n'
