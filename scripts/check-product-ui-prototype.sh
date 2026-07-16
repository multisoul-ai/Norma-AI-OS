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

# Canvas V4：四类 Live Node 共享统一的 Instrument Glass 卡片体系。
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

# 场景：评审者可以在同一份桌面原型中切换 6 套品牌配色。
#
# 数据构造（主题数量来自产品验收）：
#   light_theme_count = Ceramic Mist(1) + Pearl Lilac(1) + Sand Coral(1) = 3
#   dark_theme_count  = Sapphire Night(1) + Midnight Violet(1) + Forest Mint(1) = 3
#   total_theme_count = light_theme_count(3) + dark_theme_count(3) = 6
#
# 执行过程：
#   1. 从 HTML 公共文档读取 6 个主题选择按钮。
#   2. 分别统计 light / dark 模式标记，确认两组数量各为 3。
#   3. 通过 URL 的 theme 参数渲染指定主题，观察根节点公开的 data-theme 状态。
#
# 预期结果：
#   - 6 个已评审主题都存在；任何一个缺失时失败。
#   - 浅色必须恰好 3 个，深色必须恰好 3 个，避免模式分组失衡。
#   - URL 可直接定位主题，方便截图和设计评审。
assert_contains 'data-theme-option="ceramic-mist"' "Ceramic Mist light theme is missing"
assert_contains 'data-theme-option="pearl-lilac"' "Pearl Lilac light theme is missing"
assert_contains 'data-theme-option="sand-coral"' "Sand Coral light theme is missing"
assert_contains 'data-theme-option="sapphire-night"' "Sapphire Night dark theme is missing"
assert_contains 'data-theme-option="midnight-violet"' "Midnight Violet dark theme is missing"
assert_contains 'data-theme-option="forest-mint"' "Forest Mint dark theme is missing"

light_theme_count="$(rg -o 'data-theme-option="[^"]+" data-theme-mode="light"' "$PROTOTYPE" | wc -l | tr -d ' ')"
dark_theme_count="$(rg -o 'data-theme-option="[^"]+" data-theme-mode="dark"' "$PROTOTYPE" | wc -l | tr -d ' ')"
test "$light_theme_count" -eq 3 || fail "expected exactly 3 light themes, found $light_theme_count"
test "$dark_theme_count" -eq 3 || fail "expected exactly 3 dark themes, found $dark_theme_count"

test -n "$CHROME" && test -x "$CHROME" || fail "Chrome Headless Shell is unavailable for rendered checks"

for screen in launchpad canvas settings; do
  rendered="$($CHROME --headless --disable-gpu --no-first-run --no-default-browser-check --dump-dom "file://$PROTOTYPE#$screen" 2>/dev/null)"
  printf '%s' "$rendered" | rg -q "<section[^>]*class=\"[^\"]*is-active[^\"]*\"[^>]*data-screen=\"$screen\"[^>]*aria-hidden=\"false\"" \
    || fail "#$screen does not activate its target screen"
done

for theme_spec in \
  "ceramic-mist light" \
  "pearl-lilac light" \
  "sand-coral light" \
  "sapphire-night dark" \
  "midnight-violet dark" \
  "forest-mint dark"; do
  read -r theme mode <<< "$theme_spec"
  rendered_theme="$($CHROME --headless --disable-gpu --no-first-run --no-default-browser-check --dump-dom "file://$PROTOTYPE?theme=$theme#canvas" 2>/dev/null)"
  rg -q "<html[^>]*data-theme=\"$theme\"[^>]*data-theme-mode=\"$mode\"" <<< "$rendered_theme" \
    || fail "theme query does not activate $theme in $mode mode"
done

printf 'product-ui prototype checks passed\n'
