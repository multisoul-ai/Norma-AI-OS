# Norma OS Core UI V2：配色与评审素材

> 状态：当前设计基线  
> 更新日期：2026-08-08
> 默认浅色主题：**Paper Blue**  
> 交互原型：[docs/product-ui.html](../../product-ui.html)

本文是 Norma OS 三个核心页面与六套主题配色的维护入口。它负责说明视觉决策、主题色值、素材清单和截图再生成方式；原型中的业务文案与交互只用于演示，不替代 GPUI 的业务状态。

## 1. Source of truth

优先级从高到低：

1. 品牌原则：[`docs/design.md`](../../design.md) 与 [`docs/brand/`](../../brand/)
2. 主题规范与素材清单：本文
3. HTML 视觉原型：[`docs/product-ui.html`](../../product-ui.html)
4. 原生实现：`crates/ui/src/theme.rs` 与 `crates/app/src/`

调整配色时先更新本文与 HTML 原型；设计确认后再映射到 GPUI token。不得为了视觉演示复制或改写 runtime、canvas、terminal 的业务状态。

## 2. 视觉方向

Norma OS 是面向 vibe coding 的 macOS AI 伙伴；空间工作台是伙伴协作的交互形态，不是网页仪表盘。主题需要满足以下原则：

- 中性色承担大面积背景，品牌色只用于焦点、状态、选择与短暂反馈。
- 高饱和色的可见面积原则上不超过界面的 10%。
- 玻璃壳负责折射环境，Content well 负责内容可读性，两层不能合并。
- 浅色主题避免大面积粉彩；壁纸只作为低对比环境纹理。
- Terminal 在所有主题中保持深色内容井，避免 ANSI 色彩和日志对比失真。
- Dark theme 不是简单反色，需要独立控制玻璃边缘、文字和内容井透明度。

## 3. 当前主题

### Light

| 主题 | Base | Environment | Text | Accent | Signal | 适用场景 |
|---|---|---|---|---|---|---|
| **Paper Blue** | `#FFFEFB` / `#F5F4F1` | `#D4EAF7` | `#313D44` | Cobalt `#2F80ED` | Mint `#64E0B5` | 默认主题，长时间开发与日常工作 |
| **Linen Sage** | `#F9F8F3` / `#EBEDE6` | `#CDD8CE` | `#1D231F` | Sage `#3D9978` | Mint `#64E0B5` | 更自然、低刺激的工作环境 |
| **Silver Violet** | `#FAFAF9` / `#F0F0F2` | `#DCDDE1` | `#232328` | Violet `#6A5CFF` | Mint `#64E0B5` | 精密工具感与冷静的视觉层级 |

### Dark

| 主题 | Base | Raised glass | Text | Accent | Signal | 适用场景 |
|---|---|---|---|---|---|---|
| **Sapphire Night** | `#071324` | `#0E2338` | `#EFF7FF` | `#54A2FF` | Mint `#64E0B5` | 深色默认候选，蓝色信息密度高 |
| **Midnight Violet** | `#0C0C1B` | `#231F3C` | `#F8F5FF` | `#8B7CFF` | Mint `#64E0B5` | Agent 与自动化场景更突出 |
| **Forest Mint** | `#091716` | `#112D27` | `#F1FCF8` | Mint `#64E0B5` | Acid `#D6FF3B` | 低光环境与持续运行状态 |

品牌基础色仍以 `Warm White #F7F5F2`、`Graphite #111214`、`Violet #6A5CFF`、`Cobalt #2F80ED`、`Acid #D6FF3B`、`Coral #FF6B7A`、`Mint #64E0B5` 为准。主题中的派生色只用于环境或提高可读性。

## 4. Token 职责

| Token | 职责 | 禁止用途 |
|---|---|---|
| `--theme-canvas-overlay` | 压低壁纸彩度并确定环境温度 | 不承载业务状态 |
| `--theme-shell` | 卡片外层玻璃与环境折射 | 不直接放长文本 |
| `--theme-content` | Agent、Browser 等内容井 | 不取代 Terminal 深色井 |
| `--theme-header` | 卡片标题栏层级 | 不做大面积强调色 |
| `--theme-accent` | 焦点框、选择、工具调用信号 | 不作为页面背景 |
| `--theme-success` | 运行成功、同步完成等正向状态 | 不表示普通选中 |
| `--theme-dock` | 底部命令 Dock | 不随单张卡片状态变化 |

## 5. 原型评审入口

打开带 `review=1` 的链接可显示页面和主题切换器：

- [Paper Blue](../../product-ui.html?review=1&theme=paper-blue#canvas)
- [Linen Sage](../../product-ui.html?review=1&theme=linen-sage#canvas)
- [Silver Violet](../../product-ui.html?review=1&theme=silver-violet#canvas)
- [Sapphire Night](../../product-ui.html?review=1&theme=sapphire-night#canvas)
- [Midnight Violet](../../product-ui.html?review=1&theme=midnight-violet#canvas)
- [Forest Mint](../../product-ui.html?review=1&theme=forest-mint#canvas)

截图使用 `clean=1` 隐藏评审控件，例如：

```text
docs/product-ui.html?clean=1&theme=paper-blue#canvas
```

## 6. 素材清单

所有评审图固定为 `1600 × 1000` PNG。文件名使用稳定的 kebab-case theme id，不在文件名中加入“final”“new”等临时后缀。

### 核心页面

| 文件 | 内容 |
|---|---|
| [`launchpad.png`](launchpad.png) | 项目启动页，使用默认浅色主题 |
| [`canvas.png`](canvas.png) | 画布页，使用默认浅色主题 |
| [`settings.png`](settings.png) | 设置页，使用默认浅色主题 |

### 主题截图

| 模式 | 文件 |
|---|---|
| Light | [`themes/paper-blue.png`](themes/paper-blue.png) |
| Light | [`themes/linen-sage.png`](themes/linen-sage.png) |
| Light | [`themes/silver-violet.png`](themes/silver-violet.png) |
| Dark | [`themes/sapphire-night.png`](themes/sapphire-night.png) |
| Dark | [`themes/midnight-violet.png`](themes/midnight-violet.png) |
| Dark | [`themes/forest-mint.png`](themes/forest-mint.png) |

`Ceramic Mist`、`Pearl Lilac`、`Sand Coral` 已退出当前设计基线，其截图不应继续保留在 canonical `themes/` 目录。历史版本由 Git 记录，不另建 `final-v2` 一类重复目录。

## 7. 素材维护流程

修改 `docs/product-ui.html` 后，在仓库根目录执行：

```bash
bash scripts/render-product-ui-assets.sh
bash scripts/check-product-ui-prototype.sh
```

渲染脚本会：

1. 使用本机 Puppeteer Chrome Headless Shell；也可通过 `CHROME=/path/to/chrome-headless-shell` 指定。
2. 原子覆盖 3 张核心页面和 6 张主题截图。
3. 清理已淘汰的浅色主题截图。
4. 固定窗口尺寸、URL 参数和输出路径，保证评审素材可重复生成。

不要手工裁切、调色或覆盖这些 PNG。若截图不正确，应修改 HTML 原型或渲染脚本后重新生成。

## 8. 主题变更检查表

- [ ] 主题 id、显示名称和 light/dark 分组已更新。
- [ ] `docs/product-ui.html` 中的 CSS token、选择器和主题切换按钮一致。
- [ ] 本文的色值、预览链接和素材清单已更新。
- [ ] 已运行 `scripts/render-product-ui-assets.sh`。
- [ ] 已逐张检查内容对比、玻璃边缘、Terminal ANSI 色和 Dock 图标。
- [ ] 已运行 `scripts/check-product-ui-prototype.sh`。
- [ ] 已淘汰主题的 PNG 已从 canonical 目录移除。

## 9. 参考

- [`docs/design.md`](../../design.md)
- [`docs/brand/`](../../brand/)
- [BairesDev AI Colors](https://www.bairesdev.com/tools/ai-colors)
- [BairesDev Palette App](https://www.bairesdev.com/tools/color-palette-app/)
