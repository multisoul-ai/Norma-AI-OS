# Multisoul Design System

> 状态: Draft
> 日期: 2026-06-21
> 适用范围: `apple/Sources/MultisoulApp`
> Review 页面: [`design.html`](design.html)
>
> 本规范基于当前 Multisoul 品牌视觉、`/Users/alan/Documents/mybook/multica/docs/design.md` 的产品 UI 纪律，以及现有 SwiftUI 代码中的 `WorkspaceGlassStyle.swift`、`LiquidGlassWorkspaceView.swift`、`CanvasNodeChrome.swift`。

---

## 1. 设计定位

Multisoul 是面向开发人员的 vibe coding 开发平台。它不是传统 IDE，也不是聊天窗口，而是一个让用户与多个智能 Agent 在同一空间里协作的空间工作台。

品牌体验要传达:

- 人与多个智能体在同一空间中协作。
- Agent 能在无限画布中生成内容、程序、文件、界面、图表和交付成果。
- 工作过程与结果直接呈现在用户面前。
- 视觉气质在玩具般的活力与高端创意科技品牌的精致感之间保持平衡。

SwiftUI 界面需要同时满足两种要求:

| 层面 | 目标 | 设计表现 |
| --- | --- | --- |
| 工作工具 | 高信息密度、可读、可靠 | 深色中性层级、稳定状态、克制动效 |
| 品牌世界 | 年轻、鲜明、未来、触感 | 轨道、节点、霓虹信号、玻璃材质 |

判断标准:

- 用户正在读代码、终端、diff、日志时，视觉必须退后。
- 用户正在理解 Agent 状态、空间关系、任务节奏时，品牌语言可以前进。
- 颜色不是装饰，颜色是 Agent 角色、状态、焦点和协作节奏的信号。

---

## 2. 核心原则

### 2.1 克制承载高级感

继承 Multica 的第一原则: 默认做减法。每一个描边、发光、渐变、图标、动效都必须有存在理由。

视觉优先级:

1. 内容可读性。
2. 操作状态可辨识。
3. 空间层级清楚。
4. 品牌氛围。

如果一个视觉元素没有帮助用户理解内容、状态、空间或操作，它就是噪音。

### 2.2 层次靠灰度，颜色是信号

工作区 80%-90% 的面积由深色玻璃、中性背景和内容本身构成。高饱和色只用于小面积信号:

- 当前选择。
- 输入焦点。
- Agent 状态。
- 任务轨道。
- 成功、警告、错误。

同一屏可以出现多种品牌色，但必须有语义分工，不能成为彩色装饰墙。

### 2.3 一致性通过 token 实现

禁止在业务组件中随手写 `Color(red:)`、随机 opacity、随机 radius、随机 animation。

所有常用视觉值应来自共享入口:

- `WorkspaceGlassPalette`
- `WorkspaceMotion`
- `CanvasNodeMetrics`
- `liquidGlassSurface(...)`
- `GlassIconButton`
- 后续建议新增 `WorkspaceTypography`、`WorkspaceSpacing`、`WorkspaceRadius`

### 2.4 过程可见，状态可信

Multisoul 的核心是多个 Agent 正在同一空间里一起工作。UI 必须让用户清楚看到:

- 哪个 Agent 正在运行。
- 哪个节点被选中。
- 哪个节点拥有输入焦点。
- 哪些结果已经完成。
- 哪些内容需要用户介入。

状态层级不能被 hover、动画或装饰覆盖。

---

## 3. 视觉母题

### 3.1 Orbit / Node / Canvas

| 母题 | 含义 | SwiftUI 使用方式 |
| --- | --- | --- |
| Orbit | 多任务节奏、上下文轨道、Agent 编排 | 背景装饰、关系线、选中态边缘流光 |
| Node | Agent 个体、Live Node、产物容器 | 状态点、节点 chrome、工具按钮、badge |
| Canvas | 无限空间、实时工作台 | 深色网格、空间背景、节点布局 |

规则:

- Orbit 主要用于背景、空状态、关系可视化和品牌页面，不进入每个普通按钮。
- Node 可以作为小面积状态点，尺寸必须稳定，不能撑开布局。
- Canvas 网格保持低对比，不与内容竞争。

### 3.2 品牌气质

关键词:

- dark glass
- neon signal
- tactile toy energy
- premium creative technology
- spatial productivity

Multisoul 可以有趣，但不能幼稚；可以鲜明，但不能嘈杂。

---

## 4. 色彩系统

### 4.1 当前代码基线

现有 `WorkspaceGlassPalette`:

```swift
WorkspaceGlassPalette.cardBase
WorkspaceGlassPalette.cardHighlight
WorkspaceGlassPalette.cardShade
WorkspaceGlassPalette.menuTint
WorkspaceGlassPalette.selectedAccent
WorkspaceGlassPalette.warmRim
```

这些值是当前 SwiftUI 视觉基线。新增组件必须优先复用，不能创建近似但不同的颜色。

### 4.2 推荐补齐的语义色

```swift
enum WorkspaceBrandColor {
    static let graphite = Color(red: 0.020, green: 0.035, blue: 0.045)
    static let panel = Color(red: 0.055, green: 0.105, blue: 0.085)
    static let textPrimary = Color.white.opacity(0.96)
    static let textSecondary = Color.white.opacity(0.68)
    static let textTertiary = Color.white.opacity(0.48)

    static let orbitCyan = Color(red: 0.153, green: 0.843, blue: 0.918)
    static let agentGreen = Color(red: 0.376, green: 0.941, blue: 0.471)
    static let pulseOrange = Color(red: 1.000, green: 0.475, blue: 0.196)
    static let soulViolet = Color(red: 0.608, green: 0.333, blue: 1.000)
    static let syncBlue = Color(red: 0.294, green: 0.439, blue: 1.000)
    static let warmSignal = Color(red: 0.950, green: 0.760, blue: 0.420)
    static let destructive = Color(red: 1.000, green: 0.235, blue: 0.300)
}
```

### 4.3 语义分工

| Token | 视觉 | 含义 | 使用场景 |
| --- | --- | --- | --- |
| `pulseOrange` / `selectedAccent` | 橙色 | 当前、选中、主操作 | 选中工具、当前 Agent、主要 CTA |
| `orbitCyan` | 青色 | 连接、实时、信息 | 运行轨道、同步、信息状态 |
| `agentGreen` | 绿色 | 成功、健康、完成 | done、ok、可用状态 |
| `soulViolet` | 紫色 | 生成、创造、AI 能量 | 生成中、创意 Agent、实验能力 |
| `syncBlue` | 蓝色 | 外部资源、Web、同步 | Web node、远程状态、链接 |
| `warmSignal` | 暖黄 | 提醒、rim light | subtle warning、玻璃边缘高光 |
| `destructive` | 红色 | 错误、危险 | 删除、失败、不可恢复操作 |

规则:

- 大面积背景不使用高饱和品牌色。
- 错误和危险必须使用 `destructive`，不要用橙色替代。
- 每个节点最多有一个主 accent。
- 文本颜色最多 3 层: primary / secondary / semantic。

### 4.4 背景与网格

工作区背景默认深色:

- App 根背景使用接近黑的深灰，不使用纯黑作为唯一层级。
- 画布背景可以使用深色图像、空间渐变或低对比几何网格。
- 网格线建议使用白色 4%-8% opacity 或青色 4%-6% opacity。
- 背景图像必须通过暗色遮罩保证 terminal、diff、code 可读。

禁止:

- 大面积彩虹渐变。
- 装饰性霓虹色块压过工作内容。
- 在终端、代码、diff 内容区铺高饱和背景。

---

## 5. 字体与排版

### 5.1 字体家族

| 角色 | SwiftUI 字体 | 用途 |
| --- | --- | --- |
| UI 默认 | `.system(..., design: .default)` | 工具栏、菜单、标题、表单 |
| 代码/终端 | `.system(..., design: .monospaced)` | Terminal、命令、路径、ID、日志 |
| 品牌展示 | `.system(..., weight: .heavy)` | 欢迎页、空状态、品牌展示 |

现有 helper:

```swift
workspaceMonospace(_ size: CGFloat, weight: Font.Weight = .regular)
```

等宽信息应继续使用这个 helper，不要在各组件里重复创建 monospaced 字体。

### 5.2 App 内字号

| Token | Size | Weight | 用途 |
| --- | ---: | --- | --- |
| `micro` | 10 | `.semibold` | segmented item、极小状态 |
| `caption` | 11 | `.medium` | metadata、状态说明 |
| `chrome` | 12 | `.medium` / `.semibold` | 顶栏、按钮、标题栏 |
| `body` | 13 | `.regular` | 列表、短说明 |
| `bodyStrong` | 13 | `.medium` | 选中项、菜单项 |
| `title` | 16 | `.semibold` | node 标题、面板标题 |
| `display` | 28+ | `.heavy` | 仅品牌、欢迎、空状态 |

规则:

- 工作区常规 UI 不使用大标题。
- `display` 只允许出现在不承载密集工作内容的页面。
- 同一组件内最多 2 个字号。
- 正文默认不用 `.bold`；需要强调时优先用颜色、位置或 `.medium`。

### 5.3 文本可读性

- 按钮、segment、toolbar 文案必须 `lineLimit(1)`。
- 紧凑控件必须设置 `minimumScaleFactor(0.78...0.85)`。
- 不允许文字撑大固定格式控件。
- 中文界面不得依赖英文单词宽度调参。

---

## 6. 间距、尺寸与圆角

### 6.1 4px 网格

所有尺寸基于 4px 网格，允许 6px 作为紧凑内部间距。

| 值 | 含义 | 使用 |
| ---: | --- | --- |
| 4 | 紧密关联 | 图标与短文字、小状态点 |
| 6 | 紧凑内部 | small button padding、segment 内距 |
| 8 | 标准内部 | icon button radius、菜单项 |
| 10 | 同组间距 | 顶栏 HStack spacing |
| 12 | 组件内 padding | panel 内部 |
| 16 | 小节间距 | node 内容留白 |
| 24 | 玻璃组间距 | `GlassEffectContainer(spacing:)` |
| 32 | 大区域留白 | overlay 与窗口边缘 |

### 6.2 圆角

| Token | 值 | 用途 |
| --- | ---: | --- |
| `radiusTiny` | 6 | segment、badge |
| `radiusControl` | 8 | icon button、dropdown item |
| `radiusPanel` | 12 | 小浮层、提示面板 |
| `radiusNode` | 16 | Live Node、terminal/web/text node |
| `radiusToolbar` | 23 | 顶部 floating toolbar |
| `radiusPill` | 999 | 状态胶囊、头像、圆形节点 |

当前代码基线:

```swift
CanvasNodeMetrics.titleBarHeight = 26
CanvasNodeMetrics.cornerRadius = 16
GlassIconButton default size = 30
WorkspaceTopBar cornerRadius = 23
```

新增组件不要创建近似但不同的值。

### 6.3 容器分隔策略

从轻到重:

1. 间距。
2. 透明度变化。
3. 单像素玻璃描边。
4. 背景 material / glass surface。
5. 阴影。

如果一个区域需要靠多层描边、多层阴影和背景色才能分开，说明布局层级需要重新设计。

---

## 7. Liquid Glass 规范

### 7.1 使用入口

所有自定义玻璃面优先使用:

```swift
.liquidGlassSurface(
    cornerRadius: ...,
    tint: ...,
    isInteractive: ...,
    shadowRadius: ...
)
```

不要在业务组件中复制 `.background(.ultraThinMaterial)`、gradient overlay、stroke 和 shadow 组合。

### 7.2 macOS 26+ 与 fallback

`liquidGlassSurface` 已提供:

- macOS 26+: `.glassEffect(.regular.tint(...).interactive(...), in: shape)`
- macOS 13-25: `.ultraThinMaterial` + tint overlay + stroke + shadow fallback

规则:

- 新增 Liquid Glass API 必须有 availability gate。
- fallback 必须保持内容可读。
- `Reduce Transparency` 打开时，文字对比仍必须足够。

### 7.3 GlassEffectContainer

多个浮动玻璃元素同屏出现时:

```swift
if #available(macOS 26.0, *) {
    GlassEffectContainer(spacing: 24) {
        overlayContent
    }
} else {
    overlayContent
}
```

规则:

- 不嵌套 `GlassEffectContainer`。
- `spacing` 与视觉间距匹配，默认 24。
- 只有会共同运动或相邻融合的玻璃控件放在同一个 container。

### 7.4 交互玻璃

`.interactive(true)` 只用于真正可点击、可聚焦、可拖拽的元素:

- 顶栏。
- 工具按钮。
- 可选中的 node chrome。
- segmented control。

装饰性背景、静态卡片、纯展示 logo 不使用 interactive。

---

## 8. 组件规范

### 8.1 Workspace Root

结构:

```text
WorkspaceBackdropView
InfiniteCanvasView(opacity: 0.94, background: clear)
WorkspaceOverlay / GlassEffectContainer
```

规则:

- 背景负责品牌氛围。
- Canvas 负责工作内容。
- Overlay 负责工具 chrome。
- Overlay z-index 高于 node，但不能吞掉画布必要事件。

### 8.2 Top Bar

当前基线:

- 高度约 44-46。
- `cornerRadius: 23`。
- `HStack(spacing: 10)`。
- 横向 padding 14。
- title size 12。
- icon button size 24-30。

规则:

- Top bar 是工作区唯一最高层级 toolbar。
- 文案短，不写说明句。
- 工具按钮必须有 `.help(...)` 和 `accessibilityLabel(...)`。
- 布局模式使用 segmented control，不使用一排普通按钮。

### 8.3 GlassIconButton

使用现有 `GlassIconButton`，不要新建风格相近的按钮。

| 状态 | 视觉 |
| --- | --- |
| rest | 近透明 fill，白色 icon 0.86 |
| hover | tint fill 0.18，stroke 0.36，轻微 scale |
| selected | tint fill 0.32，stroke 0.54，icon 1.0 |
| pressed | fill/stroke 增强，scale 0.92，offsetY 1 |

规则:

- Icon-only button 必须有 tooltip/help。
- `isSelected` 不得被 hover 降级。
- tint 默认白色，状态类按钮用语义色。
- 按钮尺寸不要低于 24。

### 8.4 Live Node

Live Node 是 Multisoul 的核心组件，优先级高于所有装饰。

基线:

- corner radius 16。
- title bar height 26。
- 内容区保持高对比。
- selected/focused 通过 title tint、stroke、shadow 和 scale 表达。

| 状态 | 视觉 |
| --- | --- |
| rest | 弱 title、无显著 stroke、shadow 18 |
| hover | surface overlay 0.08、stroke 0.38、scale 1.008 |
| selected | surface overlay 0.13、stroke 0.72、shadow 24 |
| focused | surface overlay 0.16、stroke 0.86、shadow 28 |
| pressed | stroke 至少 1.25、offsetY 1、scale 0.985 |

规则:

- focused 永远强于 selected，selected 永远强于 hover。
- hover 已选中节点时，不能让 selected/focused 样式变淡。
- 终端和 diff 内容区不使用高饱和背景。
- 节点标题栏可用 accent，内容区保持克制。

### 8.5 Agent 状态点

Agent 状态点采用 Node 母题。

| 状态 | 颜色 | 文案 |
| --- | --- | --- |
| idle | `textTertiary` | Idle |
| running | `orbitCyan` | Running |
| generating | `soulViolet` | Generating |
| done | `agentGreen` | Done |
| blocked | `warmSignal` | Needs input |
| failed | `destructive` | Failed |

规则:

- 小圆点尺寸 6-8。
- 文字 size 11-12。
- 状态点不能只靠颜色，需要 tooltip 或文字。

### 8.6 Terminal / Code / Diff

终端是工作内容，不是品牌展示区。

规则:

- 背景深灰，避免纯黑。
- 字体使用 monospaced。
- Prompt / command 可用 warm 或 cyan 小面积强调。
- success/error 使用语义色。
- Diff 删除红、增加绿，透明背景不超过 20%。
- 不给终端正文加玻璃模糊层。

### 8.7 Empty State / Onboarding

空状态可以使用更强品牌表达:

- 大号 Multisoul 标题。
- Orbit / Node 插图。
- 1 个 primary action。
- 2-3 个 secondary action。

仍需遵守:

- 不写长篇说明。
- 不使用多张卡片堆叠。
- 不把空状态做成营销落地页。

---

## 9. 交互状态与动效

### 9.1 状态层级

```text
rest -> hover -> pressed -> selected -> focused -> disabled
```

含义:

- hover: 我注意到你了。
- pressed: 我正在操作你。
- selected: 你属于当前选择。
- focused: 输入、键盘或运行上下文属于你。

### 9.2 Hover

Hover 轻微、即时:

- 增加 fill opacity。
- 增加 stroke opacity。
- 允许极轻 scale，但只限 icon button 和 node chrome。
- 普通列表、菜单、表单 hover 不使用 scale。

时长:

- hover in: 0.12s。
- hover out: 0.18s。

### 9.3 Pressed

Pressed 表达触感:

- icon button: scale 0.92，offsetY 1。
- node chrome: scale 0.985，offsetY 1。
- 普通按钮: 不超过 1px 下移。

时长 0.10s，使用 `WorkspaceMotion.pressAnimation(...)`。

### 9.4 Selected / Focused 不被 Hover 覆盖

实现规则:

- 状态 resolver 先判断 focused，再 selected，再 hover。
- hover 只能在 selected/focused 基础上微增强，不能降级。
- 复杂状态使用 `resolve(...)` 函数集中计算，不在 View 链中散写条件。

检查方法:

1. 选中一个节点。
2. hover 到该节点。
3. 移开鼠标。
4. selected/focused 应始终可辨识，不能闪回普通 hover。

### 9.5 Motion Policy

新增动效必须走:

```swift
WorkspaceMotion.pressAnimation(...)
WorkspaceMotion.hoverAnimation(...)
WorkspaceMotion.chromeAnimation(...)
WorkspaceMotion.stackIndicatorAnimation(...)
WorkspaceMotion.lifecycleAnimation(...)
WorkspaceMotion.layoutAnimation(...)
```

不要在组件中随手写 `.spring(...)` 或 `.easeOut(...)`。

| Scope | 当前值 | 用途 |
| --- | ---: | --- |
| press | 0.10 | 触感反馈 |
| hover in | 0.12 | hover 进入 |
| hover out | 0.18 | hover 离开 |
| chrome | response 0.24 | 工具 chrome 状态 |
| stack indicator | response 0.26 | stack tab |
| lifecycle | response 0.34 | 节点出现/消失 |
| layout | response 0.42 | 布局变化 |
| reduced fade | 0.16 | Reduce Motion fallback |

### 9.6 Reduce Motion

`accessibilityReduceMotion` 为 true 时:

- 关闭空间位移。
- 节点 transition 改为 opacity。
- layout animation 返回 nil 或 fade。
- 不使用 matched geometry 造成大幅空间飞行。

---

## 10. 图标

SwiftUI 统一使用 SF Symbols。

规则:

- 工具按钮优先用系统 symbol，不手写 SVG。
- 图标默认继承当前状态色。
- 装饰性 icon 不超过 32。
- 标准工具按钮 icon 约为 button size 的 42%，沿用 `GlassIconButtonStyle`。
- 不熟悉的 icon 必须有 `.help(...)`。

常用映射:

| 功能 | SF Symbol |
| --- | --- |
| Select | `cursorarrow` |
| Pan | `hand.draw` 或 `hand.point.up.left` |
| Layout | `square.grid.2x2` |
| Float | `pin.fill` / `pin.slash.fill` |
| Settings | `gearshape` |
| Agent | `sparkles` 或 `brain.head.profile` |
| Review | `checkmark.seal` |
| Web | `globe` |
| Terminal | `terminal` |

---

## 11. SwiftUI 实现约束

### 11.1 不复制视觉链

禁止在业务组件中反复出现:

```swift
.background(.ultraThinMaterial)
.overlay(...)
.shadow(...)
```

应改为:

```swift
.liquidGlassSurface(
    cornerRadius: WorkspaceRadius.panel,
    tint: WorkspaceGlassPalette.menuTint.color,
    isInteractive: true
)
```

### 11.2 状态集中计算

复杂组件使用 equatable chrome model:

```swift
struct ComponentChrome: Equatable {
    static func resolve(
        isSelected: Bool,
        isFocused: Bool,
        isHovered: Bool,
        isPressed: Bool
    ) -> ComponentChrome {
        ...
    }
}
```

这样可以保证 hover、selected、focused 的优先级稳定。

### 11.3 Layout 先于 Glass

`glassEffect` 或 `liquidGlassSurface` 必须在 frame/padding/layout 之后应用。

正确:

```swift
Text("Status")
    .padding(.horizontal, 12)
    .frame(height: 28)
    .liquidGlassSurface(cornerRadius: 14, tint: tint, isInteractive: true)
```

错误:

```swift
Text("Status")
    .liquidGlassSurface(cornerRadius: 14)
    .padding(.horizontal, 12)
```

### 11.4 文件组织

推荐:

```text
Workspace/
  WorkspaceGlassStyle.swift
  WorkspaceTypography.swift
  WorkspaceMetrics.swift
  WorkspaceMotion.swift
Canvas/
  CanvasNodeChrome.swift
```

保持单个手写 Swift 文件不超过 500 行。

---

## 12. 可访问性

必须验证:

- Reduce Transparency: 玻璃降级后文字仍可读。
- Reduce Motion: 大幅空间动画关闭。
- Keyboard Focus: 可交互元素有明确 focus 状态。
- VoiceOver: icon-only button 有 `accessibilityLabel`。
- Contrast: 终端、diff、代码文本优先保证对比度。

原则:

- 状态不能只靠颜色表达，需要文字、图标、tooltip 或位置共同表达。
- 低透明模式下不能出现白字叠亮背景导致不可读。
- motion 是辅助理解，不是唯一的信息载体。

---

## 13. 反模式清单

禁止:

- 在组件中硬编码品牌色。
- 玻璃效果套玻璃效果。
- 对非交互装饰使用 `.interactive(true)`。
- 大面积高饱和背景。
- 让 hover 覆盖 selected/focused。
- 在 terminal/code 内容区使用装饰性渐变。
- 字体尺寸随窗口宽度缩放。
- 固定格式控件被文字撑开。
- 无 tooltip 的 icon-only button。
- 忽略 Reduce Motion / Reduce Transparency。
- 新建与 `GlassIconButton` 外观相近但行为不同的按钮。

---

## 14. UI 变更检查清单

提交任何 SwiftUI UI 变更前检查:

- [ ] 是否使用共享 palette / radius / motion / glass modifier?
- [ ] 颜色是否有语义，而不是装饰?
- [ ] 同一屏高饱和色是否没有过量竞争?
- [ ] 文本层级是否不超过 primary / secondary / semantic?
- [ ] 工作区常规 UI 是否保持 10-16pt 的信息密度?
- [ ] icon-only button 是否有 `.help` 和 `accessibilityLabel`?
- [ ] hover 是否比 selected/focused 弱?
- [ ] selected/focused 在 hover 时是否不会降级?
- [ ] `.interactive(true)` 是否只用于可交互元素?
- [ ] `GlassEffectContainer` 是否没有嵌套?
- [ ] Reduce Motion 下是否没有大幅空间动画?
- [ ] Reduce Transparency 下文字是否仍可读?
- [ ] 终端、代码、Diff 是否没有被装饰性视觉干扰?
- [ ] 新增 Swift 文件是否已在 `apple/project.yml` 注册并通过 xcodegen?

---

## 15. 与 Multica 规范的关系

从 Multica 继承:

- 克制即高级。
- 层次靠灰度，颜色是信号。
- 一致性大于个性。
- hover / active / disabled / invalid 状态必须统一。
- 不要用随机字号、随机颜色、随机动效制造层级。

Multisoul 的差异:

- 允许更强品牌色，但只作为 Agent / 状态 / 轨道信号。
- 允许玻璃材质和轻微发光，但必须通过共享 token 管控。
- 允许 icon button 和 node chrome 有轻微 scale/press 触感，但普通列表和菜单仍保持克制。
- 允许品牌页、空状态、沉浸空间使用大标题和轨道视觉，但工作区主界面保持生产力密度。

---

## 16. Review HTML 使用方式

`docs/design.html` 是本规范的视觉 review board，用来快速检查:

- 色彩 token 是否形成统一但鲜明的品牌系统。
- 玻璃层级是否清晰。
- Live Node、Agent 状态、终端、diff 是否符合工作区气质。
- hover、selected、focused、disabled 等状态是否有正确强弱关系。
- 规范是否能被 SwiftUI 代码直接翻译。

本地打开即可预览:

```bash
open /Users/alan/Documents/mybook/multisoul-desktop/docs/design.html
```

---

## 17. 第一批落地建议

1. 从 `WorkspaceGlassStyle.swift` 中拆出 `WorkspaceMotion.swift`，降低单文件复杂度。
2. 新增 `WorkspaceMetrics.swift`，集中定义 radius、spacing、button size、node chrome size。
3. 新增 `WorkspaceBrandColor` 或扩展 `WorkspaceGlassPalette`，补齐 cyan / green / violet / blue / destructive 语义色。
4. 将顶栏 `.bold` 文案逐步收敛为 `.semibold`，保留品牌页和空状态的大字重。
5. 给未来 Agent 状态组件建立统一 `AgentStatusBadge`，不要在各节点里散写状态点。
