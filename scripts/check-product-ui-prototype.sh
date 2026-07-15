#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROTOTYPE="$ROOT/docs/product-ui.html"
CHROME="${CHROME:-$(find "$HOME/.cache/puppeteer/chrome-headless-shell" -type f -name chrome-headless-shell 2>/dev/null | sort -r | head -n 1)}"

fail() {
  printf 'product-ui check failed: %s\n' "$1" >&2
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

test -f "$PROTOTYPE" || fail "docs/product-ui.html is missing"

# 场景：核心页面原型必须覆盖用户给出的三个产品 seam。
#
# 数据构造：
#   page_count = Launchpad(1) + Canvas(1) + Settings(1) = 3
#   card_types  = Terminal(1) + Agent(1) + Browser(1) + Preview(1) = 4
#
# 执行过程：
#   1. 从 HTML 公共文档读取三个 screen 标记。
#   2. 检查 Canvas 是否显式呈现四类 Live Node。
#   3. 检查 URL hash 是否能把每个页面渲染为 active screen。
#
# 预期结果：
#   - 三个页面都存在，缺少任意页面时失败。
#   - 四类卡片都存在，避免原型只美化当前 Terminal 截图。
#   - 每个 hash 都只激活目标页面，确保评审链接可直接定位。
assert_contains 'data-screen="launchpad"' "Launchpad screen is missing"
assert_contains 'data-screen="canvas"' "Canvas screen is missing"
assert_contains 'data-screen="settings"' "Settings screen is missing"
assert_contains '>TERMINAL<' "Terminal live card is missing"
assert_contains '>AGENT<' "Agent live card is missing"
assert_contains '>BROWSER<' "Browser live card is missing"
assert_contains '>PREVIEW<' "Preview live card is missing"

# macOS 桌面应用验收：三页必须共享原生窗口语言，演示导航默认不可见。
assert_contains 'class="mac-titlebar"' "native macOS titlebar is missing"
assert_contains 'class="traffic-lights"' "macOS traffic lights are missing"
assert_contains '.prototype-nav { display: none;' "web-like prototype navigation must be hidden by default"

# 玻璃材质验收：Glass shell 负责半透明与背景模糊，Content well 单独保证可读性。
assert_contains '--glass-shell: rgba(255, 255, 255, .44);' "glass shell token is missing"
assert_contains 'backdrop-filter: blur(36px) saturate(155%);' "canvas glass blur is missing"
assert_contains '.canvas-object::before' "specular glass highlight is missing"
assert_contains 'class="terminal-well content-well"' "Terminal content well is not separated from its glass shell"
assert_absent 'brand/assets/source-references/' "prototype must use the product AppIcon asset, not brand source references"
assert_absent 'multisoul' "prototype must not expose the retired brand name"

# Canvas V3：使用项目真实壁纸建立可折射环境，控制坞不得与卡片混排。
assert_contains '../crates/app/assets/wallpapers/1331865.png' "Canvas must use an existing product wallpaper"
assert_contains 'class="canvas-command-dock"' "bottom command dock is missing"
assert_contains 'class="dock-voice"' "Voice must be integrated into the command dock"
assert_absent 'class="canvas-connection' "persistent diagram connections must be removed"
assert_absent 'class="voice-bar"' "standalone Voice bar must be removed to prevent card overlap"

# 场景：四类 Live Node 必须共享同一套可实现的 Instrument Glass 卡片体系。
#
# 数据构造（含关键数值的推导过程）：
#   shell opacity       = 0.70：保留 30% 环境色，但不能让四个壁纸区域主导卡片色相。
#   titlebar opacity    = 0.78：比 shell 高 8%，建立稳定标题层级。
#   light well opacity  = 0.90：比 shell 高 20%，保证 Agent/Browser/Preview 内容可读。
#   dark well opacity   = 0.92：Terminal 比浅色内容井再高 2%，保持代码对比度。
#   radius pair         = outer(22px) - inner(20px) = 2px，只补偿 1px 外边框与 1px 内高光。
#   content inset       = 0px：内容井紧接 44px 标题栏，不引入真实实现不存在的第二层卡框。
#
# 执行过程（逐步说明系统如何处理）：
#   1. 从原型公共文档读取卡片系统 token。
#   2. 检查 Canvas V4 外壳、标题栏和内容井是否消费统一 token。
#   3. 拒绝 V3 的 22% 超透明外壳，避免壁纸色相把四类卡片拆成四套视觉语言。
#
# 预期结果：
#   - 正断言：外壳、标题栏、浅/深内容井都有独立且统一的 token。
#   - 正断言：22/20px 圆角形成 2px 光学补偿，内容井不额外侵占业务内容 bounds。
#   - 负断言：不再使用 rgba(241, 247, 255, .22) 的过度透明外壳。
assert_contains '--card-shell: rgba(236, 242, 249, .70);' "cohesive card shell token is missing"
assert_contains '--card-titlebar: rgba(248, 250, 253, .78);' "stable card titlebar token is missing"
assert_contains '--card-surface-light: rgba(250, 251, 253, .90);' "shared light content-well token is missing"
assert_contains '--card-surface-dark: rgba(9, 14, 22, .92);' "terminal content-well token is missing"
assert_contains '--card-radius: 22px;' "card outer radius token is missing"
assert_contains '--card-well-radius: 20px;' "card inner radius token is missing"
assert_contains '--card-well-inset: 0px;' "card content inset must match native content geometry"
assert_contains 'background: var(--card-shell);' "Canvas cards do not consume the shared shell token"
assert_contains 'background: var(--card-titlebar);' "Canvas titlebars do not consume the shared titlebar token"
assert_contains 'background: var(--card-surface-dark);' "Terminal does not consume the dark content-well token"
assert_contains 'background: var(--card-surface-light);' "light content wells do not consume one shared surface token"
assert_contains 'height: calc(100% - 44px);' "content wells must begin directly below the 44px titlebar"
assert_contains 'border-radius: 0 0 var(--card-well-radius) var(--card-well-radius);' "content wells must share the shell bottom geometry"
assert_absent 'background: rgba(241, 247, 255, .22);' "V3 over-transparent card shell still allows wallpaper hue drift"

# 品牌目标色来自 docs/design.md §3.1，逐项用已评审字面量校验，避免同源计算。
assert_contains '--warm-white: #f7f5f2;' "warm-white token is missing or changed"
assert_contains '--graphite: #111214;' "graphite token is missing or changed"
assert_contains '--violet: #6a5cff;' "violet token is missing or changed"
assert_contains '--cobalt: #2f80ed;' "cobalt token is missing or changed"
assert_contains '--acid: #d6ff3b;' "acid token is missing or changed"
assert_contains '--coral: #ff6b7a;' "coral token is missing or changed"
assert_contains '--mint: #64e0b5;' "mint token is missing or changed"

test -n "$CHROME" && test -x "$CHROME" || fail "Chrome Headless Shell is unavailable for rendered checks"

for screen in launchpad canvas settings; do
  rendered="$($CHROME --headless --disable-gpu --no-first-run --no-default-browser-check --dump-dom "file://$PROTOTYPE#$screen" 2>/dev/null)"
  printf '%s' "$rendered" | rg -q "<section[^>]*class=\"[^\"]*is-active[^\"]*\"[^>]*data-screen=\"$screen\"[^>]*aria-hidden=\"false\"" \
    || fail "#$screen does not activate its target screen"
done

printf 'product-ui prototype checks passed\n'
