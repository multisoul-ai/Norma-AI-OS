# Settings 页面视觉与导航刷新规格

## 问题

当前 Settings 面板把已实现页面平铺为无分组的列表，且将字符当作图标；窄窗口会让侧栏和内容区相互挤压。页面仍需维持既有的快捷键、语音、Agent、Hooks 和壁纸配置行为。

## 方案

保留覆盖在工作台之上的居中 Settings 面板，按用户任务组织可配置页面，并采用深色暖灰、低饱和橙色强调与轻量卡片化层级。普通控件继续即时生效并后台保存；本次不增加配置项、设置搜索或独立窗口。

## 导航

- `Appearance`：`Wallpaper`
- `Input & Controls`：`Keyboard Shortcuts`、`Voice`
- `AI & Automation`：`Agents`、`Agent Hooks`

`General`、`Runtime`、`Skills` 继续只作为内部兼容 fallback，不出现在导航中。
Settings 首次打开时进入 `Wallpaper`，因为它是第一个可见、可配置的入口；不得让用户落入隐藏 fallback 的空白内容区。

## 响应式行为

- 宽面板：保留带分组标题、图标和文字的左侧导航。
- 中等宽度：收紧面板边距与导航宽度，内容区保持可滚动的有效宽度。
- 窄面板：导航改为可横向滚动的紧凑顶栏；内容区占满剩余高度和宽度。

## 不变式

- 现有 Settings section 的选择、关闭、快捷键录制、语音即时保存、模型安装与壁纸选择行为不变。
- 现有 e2e `debug_selector` 保持稳定，避免测试和自动化点击失效。
- 采用 `gpui-component` 已提供的图标和常规控件能力；若该组件无法保留既有测试 selector 或行为，则以 GPUI 原语包裹交互 seam。

## 验收

1. 五个可配置入口都能从渲染树点击并进入原 section。
2. 已有 Voice 与 Wallpaper e2e 快照行为保持不变。
3. 纯布局在宽、中、窄三个区间为内容区分配正宽度，窄区间切换为紧凑顶栏。
