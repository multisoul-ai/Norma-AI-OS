# Norma OS Product Design System

> 状态：V2 / Current Direction
>
> 更新：2026-07-15
>
> 适用：`crates/ui`、`crates/app` 以及所有用户可见的 Norma OS 界面
>
> 视觉评审页：[design.html](design.html)
>
> 品牌规范：[brand/norma-os-brand-guidelines.md](brand/norma-os-brand-guidelines.md)

---

## 0. 设计结论

Norma OS 是一张会工作的桌面。

它不是传统 IDE、聊天窗口或数据仪表盘。它把 Terminal、AI Agent、Browser 和 File Preview 作为持续运行的对象放到同一张无限画布中，让用户始终知道：项目在哪里、什么正在运行、哪里需要自己介入。

V2 采用受 Apple 产品哲学启发的方法，但不复制具体 Apple 产品或页面：

- 产品先于装饰。
- 复杂能力以直接、安静的方式呈现。
- 通过比例、留白、材质和精度建立高级感。
- 高饱和色只承担身份、状态和动作语义。
- Norma 提供人情味与观点，不抢走工作内容。

一句话验收标准：

> 用户先看懂项目，再感受到品牌。

---

## 1. 体验原则

### 1.1 Product is the hero

视觉首先解释产品如何工作。任何图形、光效、角色或动效都必须服务于以下至少一项：

1. 内容可读。
2. 状态可辨。
3. 空间关系可理解。
4. 操作结果可预测。

无法满足其中任意一项的元素应被删除。

### 1.2 Calm by default

默认状态安静、稳定、低对比。只有用户操作、运行状态或需要注意的事件出现时，界面才提高视觉能量。

- 画布使用暖白或轻冷白雾面背景。
- 卡片使用近白玻璃或高不透明深色内容层。
- 阴影柔和、范围宽、边缘低对比。
- 不使用持续发光、持续漂浮或无状态含义的动画。

### 1.3 Depth through material

层级主要依靠材质与空间，而不是更多边框：

- Canvas：陶瓷白 / 雾面空间。
- Card shell：磨砂玻璃 / 半透明聚合物。
- Content well：高不透明、保证阅读。
- Toolbar：悬浮玻璃胶囊。
- Selected / Focused：精确的蓝色描边，不使用大面积色块。

### 1.4 Color is information

品牌色不平均分配。大部分界面由暖白、银、石墨和内容本身构成；彩色只用于状态、连接和关键动作。

### 1.5 Human, not cute

Norma 是有观点的 AI 伙伴。她可以冷静、聪明、略带傲娇，但不能被幼化，也不能作为与任务无关的装饰贴纸。

### 1.6 Precision is a feature

统一的圆角、间距、基线、阴影、描边与缓动是产品可信度的一部分。视觉值必须来自共享 token，不允许业务组件随手创造近似值。

---

## 2. 产品视觉模型

### 2.1 三层空间

| 层 | 对象 | 视觉职责 |
| --- | --- | --- |
| Space | 无限画布、背景、关系 | 提供方向、尺度和呼吸感 |
| Objects | Terminal、Agent、Browser、Preview 卡片 | 承载真实、持续运行的内容 |
| Signals | 选择、焦点、运行、等待、完成、错误 | 解释当前状态与下一步动作 |

三个层级不得混用。背景不能模拟状态，状态光不能成为内容底色，卡片内容不能承担画布导航。

### 2.2 四类核心卡片

| 类型 | 主要内容 | 默认表面 | 强调色 |
| --- | --- | --- | --- |
| Terminal | Shell 进程、命令、日志 | 深石墨内容层 | Cobalt / Cyan |
| Agent | 对话、推理、工具调用、任务队列 | 浅色或深色内容层 | Violet |
| Browser | 真实网页 | 浅色浏览器表面 | Cobalt |
| Preview | Markdown、文本、图片 | 纸张白 / 图像画布 | Mint |

卡片是可运行对象，不是静态截图。标题栏至少要表达类型、身份和当前状态，但不得挤压主要内容。

---

## 3. 颜色系统

### 3.1 品牌目标色

| Token | Hex | 语义 |
| --- | --- | --- |
| `warm_white` | `#F7F5F2` | 主背景、包装、明亮空间 |
| `canvas_mist` | `#E8EEF8` | 画布远端、雾面层次 |
| `graphite` | `#111214` | 深色界面、正文、精密材质 |
| `silver` | `#D9DADD` | 分隔、金属、辅助层 |
| `violet` | `#6A5CFF` | Agent、Tool Call、创造性操作 |
| `cobalt` | `#2F80ED` | Thinking、链接、导航、选择 |
| `acid` | `#D6FF3B` | Waiting、Active、需要继续的动作 |
| `coral` | `#FF6B7A` | Error、危险、需要关注 |
| `mint` | `#64E0B5` | Complete、成功、可用 |

推荐面积比例：

- 70%：暖白或石墨基础面。
- 20%：银色、灰阶、半透明材料。
- ≤10%：高饱和状态色。

### 3.2 当前 GPUI token 基线

当前实现位于 `crates/ui/src/theme.rs`。文档不得假装代码已经完成尚未实施的迁移。

| 当前 token | 当前值 | V2 使用方式 |
| --- | --- | --- |
| `canvas_from` | `#F8FBFF` | 浅色画布起点 |
| `canvas_to` | `#E8EEF8` + alpha | 浅色画布远端 |
| `node_fill` | 白色高透明度 | 卡片玻璃外壳 |
| `selected_stroke` | `#0A84FF` | 选择态外描边 |
| `focused_stroke` | `#006FE6` | 键盘/输入焦点内描边 |
| `accent_blue` | `#4B70FF` | Thinking / 信息 |
| `accent_violet` | `#9B55FF` | Tool Call / Agent |
| `accent_warm` | `#F2C26B` | Waiting 过渡色 |
| `accent_green` | `#60F078` | Complete 过渡色 |
| `destructive` | `#FF3C4D` | Error / 危险 |
| `accent_cyan` | `#27D7EA` | Live / 连接 |

后续代码迁移时，建议在不破坏现有调用方的前提下补充语义 token：

```rust
thinking
tool_call
waiting
complete
error
live_connection
```

语义 token 再映射到品牌色，业务组件不直接选择色相。

### 3.3 文本颜色

浅色界面：

- Primary：`#111214` / 92%–100%。
- Secondary：`#111214` / 60%–68%。
- Tertiary：`#111214` / 38%–46%。

深色界面：

- Primary：白色 / 92%–96%。
- Secondary：白色 / 62%–70%。
- Tertiary：白色 / 38%–48%。

正文不得使用低于 WCAG AA 的对比度。占位文字不是禁用文字；两者必须可区分。

---

## 4. 字体系统

### 4.1 字体栈

macOS 原生界面使用系统字体，确保与平台排版和字形渲染一致：

- UI / 品牌标题：SF Pro Display / 系统默认无衬线。
- 正文：SF Pro Text / 系统默认无衬线。
- 终端、路径、ID、时间：SF Mono / 等宽回退。
- 中文：PingFang SC。

### 4.2 字级

| Token | Size | Weight | Line Height | 用途 |
| --- | ---: | --- | ---: | --- |
| `display` | 56 | 700 | 1.02 | Launchpad、品牌页大标题 |
| `hero` | 40 | 700 | 1.08 | 设置页或空状态主标题 |
| `title_1` | 28 | 650 | 1.15 | 页面标题 |
| `title_2` | 20 | 600 | 1.25 | 面板标题 |
| `headline` | 15 | 600 | 1.33 | 卡片标题、强调文本 |
| `body` | 14 | 400 | 1.48 | 说明、列表、表单 |
| `callout` | 13 | 500 | 1.42 | 控件、次级信息 |
| `caption` | 11 | 500 | 1.36 | 状态、metadata |
| `micro` | 10 | 600 | 1.30 | 极小标签，不承载关键内容 |

规则：

- 大标题依赖尺度与留白，不使用描边、渐变字或外发光。
- 卡片标题保持单行，必要时省略中间路径而不是压缩字号。
- 代码内容不继承品牌展示字重。
- 同一层级只使用一个字号与一个主要字重。

---

## 5. 间距、圆角与尺寸

### 5.1 4pt 基线

```text
4, 8, 12, 16, 20, 24, 32, 40, 48, 64, 80, 96
```

常用语义：

| Token | Value | 用途 |
| --- | ---: | --- |
| `space_1` | 4 | 图标与微标签 |
| `space_2` | 8 | 紧凑控件内部 |
| `space_3` | 12 | 控件 padding、标题栏 |
| `space_4` | 16 | 卡片内部基本间距 |
| `space_6` | 24 | 面板内容组 |
| `space_8` | 32 | 大组件间距 |
| `space_12` | 48 | 页面区段 |
| `space_16` | 64 | 展示页留白 |

### 5.2 圆角

| 对象 | Radius |
| --- | ---: |
| 小图标底座、badge | 8 |
| 输入框、普通按钮 | 10 |
| 菜单、popover、设置分组 | 14 |
| Canvas 卡片 | 18 |
| 大型浮层 | 22 |
| 悬浮工具栏 | 高度的一半 |

嵌套圆角遵循同心原则：内层圆角 = 外层圆角 − 内边距。不要在 18px 卡片内放置 18px 的贴边内容层。

### 5.3 最小交互尺寸

- 图标按钮视觉尺寸可为 28–32px，命中区域不得小于 36×36px。
- 主要工具栏控件高度 40–44px。
- 文本输入高度至少 36px。
- 拖拽手柄必须有明确光标或视觉暗示，不依赖 1px 细线命中。

---

## 6. 材质与阴影

### 6.1 Canvas

浅色主题是默认品牌展示方向：

- 起点接近 `#F8FBFF`。
- 远端过渡到轻冷的 `#E8EEF8`。
- 可加入极轻的径向光，但不得形成可见彩色渐变斑块。
- 网格只在缩放或导航需要时出现，透明度建议 3%–6%。

### 6.2 Card shell

- 浅色：白色 88%–94% + 顶部白色高光。
- 深色：石墨 78%–88% + 低对比白色描边。
- 1px 光学边框，不使用 2px 常驻描边。
- 卡片必须有真实接触阴影，但不能像漂浮贴纸。

当前 `Theme` 阴影基线：

```text
y: 18
blur: 44
spread: -18
```

拖拽时可略微抬升；按下时应缩短阴影并降低位移，形成触感。

### 6.3 Content well

Terminal 与高密度文本区域应比玻璃外壳更不透明。内容可读性优先于材质展示。

- Terminal：深石墨、高不透明度。
- Markdown：纸张白或深灰阅读面。
- Browser：尊重网页自身背景。
- Agent 日志：按事件分组，不在整张卡片铺高饱和底色。

---

## 7. 卡片解剖

### 7.1 必备区域

```text
┌─────────────────────────────────┐
│ icon  title / identity   status │  title bar
├─────────────────────────────────┤
│                                 │
│         live content            │  content well
│                                 │
├─────────────────────────────────┤
│ optional contextual controls    │  footer / overlay
└─────────────────────────────────┘
```

### 7.2 标题栏

- 高度建议 40–44px。
- 类型图标 16px；状态点 6–8px。
- 标题使用 `headline` 或 `callout`。
- 关闭、更多等按钮默认低对比，hover 后才增强。
- 拖拽区域不能与文本选择和按钮命中冲突。

### 7.3 状态层级

视觉优先级从高到低：

1. Error / destructive confirmation。
2. Waiting for user。
3. Input focus。
4. Selected。
5. Running / tool call。
6. Hover。
7. Idle。

不同含义使用不同层表达，避免互相覆盖：

- Selection：外侧蓝色细描边。
- Input focus：内侧更清晰的蓝色 keyline。
- Runtime state：标题栏状态点、角部短光或局部边缘动画。
- Error：状态点 + 文本 + 必要时局部 coral，不把整卡染红。

---

## 8. 状态、动效与反馈

### 8.1 状态语言

| 状态 | 颜色 | 动态 | 文案要求 |
| --- | --- | --- | --- |
| Thinking | Cobalt | 2.0–2.8s 平稳脉冲 | 描述正在理解或推理 |
| Tool Call | Violet | 1.2–1.8s 单向轨道 | 显示工具或动作名称 |
| Waiting | Acid / Warm transitional | 1.8–2.4s 呼吸 | 明确等待谁、等待什么 |
| Complete | Mint / Green transitional | 280–420ms 绽放后静止 | 给出结果入口 |
| Error | Coral / Destructive | 一次短促闪烁后静止 | 说明原因与恢复动作 |

状态不能只依赖颜色。必须至少同时提供文字、图标、形态或运动中的一种。

### 8.2 时间与缓动

| 动作 | Duration | 曲线 |
| --- | ---: | --- |
| Hover / press | 100–140ms | ease-out |
| 选择 / 焦点 | 140–180ms | ease-out |
| Card 出现 | 240–320ms | decelerating spring |
| Card 重排 | 320–480ms | critically damped spring |
| Complete | 280–420ms | ease-out then settle |
| Modal | 180–240ms | ease-out |

拖拽必须直接跟手，不使用补间延迟。`reduce_motion` 开启时移除位移、缩放和循环运动，保留必要的颜色与透明度状态变化。

### 8.3 语音驱动窗口内缘光

语音内缘光遵循 `CONTEXT.md` 的产品定义：

- 由实际播放的 TTS PCM 能量驱动。
- 使用白色渐变光，不映射 Agent 状态色。
- 四边同步，快速提亮、平滑回落。
- 位于弹窗内容之下，不是原生窗口描边。
- 静音底光约 12% opacity / 30px；峰值约 55% / 38px。
- 纯文本结果或仅完成合成但尚未播放的音频不得触发。

白色语音光表达“Norma 正在说话”；彩色卡片状态表达“系统正在做什么”。两者不能混用。

---

## 9. 控件

### 9.1 按钮

| 类型 | 表面 | 使用 |
| --- | --- | --- |
| Primary | Graphite 或系统蓝 | 当前页面唯一主动作 |
| Secondary | 半透明白 / 细边框 | 次级动作 |
| Ghost | 透明 | 工具栏、低风险局部动作 |
| Destructive | Coral / red text | 删除、不可逆操作 |

- 同一面板最多一个 Primary。
- 图标必须来自 `crates/ui/src/icons.rs` 或统一资产库。
- 禁止用 Unicode 字符代替正式图标。
- Hover、pressed、disabled、focus-visible 都必须有独立状态。

### 9.2 输入框

- 默认浅灰或低对比玻璃表面。
- Focus 使用蓝色 1–2px keyline，不使用大面积蓝色 glow。
- 校验错误就地出现，保持用户输入。
- 异步保存失败必须显示可恢复反馈。

### 9.3 设置页

设置仍在工作台上下文中打开，但视觉从旧的深色霓虹面板迁移为安静的分组表面：

- 导航按用户任务组织，不按 crate 组织。
- 宽窗口使用稳定侧边栏；窄窗口使用紧凑导航。
- 普通设置即时生效并异步持久化。
- 模型安装、下载等长操作使用进度和明确状态按钮。
- 不提供无内容的占位栏目。

---

## 10. Norma 的产品内角色

### 10.1 固定识别

- 冷调浅紫色长马尾、整齐刘海。
- 大号白色 X 发夹、黑色蝴蝶结。
- 深海军蓝剪裁服装。
- 蓝紫色眼睛。
- 冷静、自信、略带傲娇。

### 10.2 出现位置

适合：

- Launchpad 或首次进入项目的品牌时刻。
- Agent 空状态与 onboarding。
- 任务完成、等待确认、错误恢复等情绪节点。
- Voice Preview 与角色声音设置。

不适合：

- Terminal、diff、日志正文背景。
- 每张 Agent 卡片的常驻大头像。
- 普通按钮、菜单项或无意义 loading。

### 10.3 比例

- 产品操作页：角色视觉面积通常不超过 12%。
- 品牌/空状态页：可提升到 30%–45%。
- Q 版只用于贴纸、摆件、彩蛋或轻量完成反馈。

---

## 11. 可访问性

- 正文和关键控件满足 WCAG AA 对比度。
- 状态不得只靠颜色区分。
- 所有键盘可操作控件提供明确 focus-visible。
- 动画遵守 reduce motion，语音 PCM 内缘光除外，其行为以产品规范为准。
- 最小正文 12px；长文建议不低于 14px。
- 触控板缩放必须有当前比例反馈与可恢复的 100% 动作。
- 屏幕阅读器标签描述动作，不重复可见图标名称。
- 错误信息包含原因、影响与恢复路径。

---

## 12. 工程落点

| 设计职责 | Canonical location |
| --- | --- |
| 颜色、阴影、主题 | `crates/ui/src/theme.rs` |
| 共享图标 | `crates/ui/src/icons.rs` |
| 卡片选择、焦点、运行视觉 | `crates/app/src/motion/node_visual.rs` |
| 工作状态边缘动画 | `crates/app/src/motion/working.rs` |
| 初始化动画 | `crates/app/src/motion/initialization.rs` |
| 画布与卡片渲染 | `crates/app/src/` 对应职责目录 |
| 产品词汇与语音内缘光 | `CONTEXT.md` |
| 品牌身份 | `docs/brand/` |

实现规则：

1. 域层不得依赖 UI token。
2. 新视觉 token 优先进入 `crates/ui`，业务组件只消费语义值。
3. 交互状态先由 runtime / app 事实决定，再映射视觉。
4. 不为视觉方便复制运行时状态。
5. 新的用户可观察流程按 `AGENTS.md` 要求补 e2e 验证。

---

## 13. 设计评审清单

提交界面前逐项回答：

### Product

- 用户能否在 3 秒内看懂当前项目和主要对象？
- Terminal、Agent、Browser、Preview 是否像真实运行对象？
- 是否明确显示正在运行、等待、完成或失败？

### Hierarchy

- 页面是否只有一个主视觉焦点？
- 是否可以删除至少一个装饰元素而不损失含义？
- 内容层是否比玻璃材质更易读？

### Color

- 高饱和色是否有明确语义？
- 是否避免同时大面积使用所有品牌色？
- Error、Waiting、Complete 是否不会混淆？

### Interaction

- Hover、Selected、Focused、Running 是否分别可辨？
- 拖拽是否跟手？
- 键盘焦点是否清楚？
- Reduce Motion 下是否仍可理解状态？

### Brand

- 是否使用 `Norma OS`，而不是旧品牌名？
- Norma 是否保持固定识别特征？
- 是否体现安静、精密、有人情味，而非通用霓虹科技感？
- 是否避免直接复制 Apple 标志、页面、包装或硬件？

---

## 14. 禁止事项

- 不使用 Apple 标志、Apple 字样或第三方品牌资产。
- 不一比一复刻 Apple 官网、发布会、包装或硬件外形。
- 不把大面积紫色渐变当成“AI 感”。
- 不使用常驻彩虹 glow、霓虹网格或装饰性轨道。
- 不把界面做成拥挤的 dashboard。
- 不在 Terminal、代码、diff 背后放人物或高饱和图像。
- 不使用随机圆角、随机透明度和随机动画时长。
- 不用颜色代替文字状态。
- 不把 Norma 幼化或当成无关贴纸。
- 不在正式界面和文档中使用旧品牌名。
