# Dispatch Receipt 离子派发特效 SPEC

- 日期：2026-08-01
- 状态：Dock、语音 Handoff 与同 Workspace Agent-to-Agent 入口已实现，待真实 App 最终验收
- 目标版本：首版
- 适用范围：Command Dock、Norma Agent 任务派发、Agent CLI 子委派、语音 Agent 续聊、Agent Card、Canvas 动效
- 技术基线：Rust 2021、GPUI 0.2.2、gpui-component 0.5.1
- 关联调研：[`2026-08-01-gpui-particle-projectile-effects.md`](../research/2026-08-01-gpui-particle-projectile-effects.md)
- 视觉基准：[`ion-fireworks.html`](../ion-fireworks.html)

## 1. 需求背景

Norma OS 已能通过 `DispatchAgentTasks` 将新任务路由到指定 Agent Card，也能通过 `reply_agent` 把语音续聊写入已有会话。当前反馈主要依赖文字、语音和卡片状态，用户不一定能立即理解“这条任务或后续消息被送到了哪张卡片”。

本功能增加一段非阻塞的空间反馈：当 Norma 接受一次面向可见 Agent Card 的任务派发，成功提交一次语音来源的 Agent 续聊，或同一 Workspace 内的 Agent 通过受权 CLI 委派子任务后，从真实视觉来源发射一个带微型 receipt 轮廓的离子核心，沿双螺旋弧线飞向目标卡片中心；命中后播放分层离子爆发、冲击波和卡片轮廓脉冲。

这个特效表达的是：

> Norma 已将任务或语音续聊消息路由给目标 Agent，目标卡片已经获得可识别的 Handoff 回执。

它不表达 Agent 已开始执行，也不表达任务成功完成。

### 1.1 产品目标

- 让用户在不阅读详细文字的情况下，立即看到任务或语音续聊消息被送往哪张 Agent Card。
- 用 Dock/来源 Agent 与目标卡片之间的连续轨迹建立清晰的空间因果关系。
- 不阻塞派发、不改变任务状态、不抢焦点，也不影响高频工作流。
- 复用现有 `MotionStore` 和 GPUI 帧调度，不建立第二套动画循环。

### 1.2 第一性原理判断

该特效存在的理由不是“增加炫酷感”，而是解决一个真实的信息缺口：用户发出委派意图后，需要确认系统选中了哪个目标。若界面已经能在同一注意区域清楚标示目标，则特效应保持克制或降级，而不应成为强制庆祝动画。

## 2. 功能概述与范围

### 2.1 In Scope

- Norma 来源的单 Agent 与多 Agent 任务派发 receipt 可视化。
- 同一 Workspace 内，带可信 `source_node_id` 的 Agent CLI 子委派可视化。
- 语音来源 `reply_agent` 成功写入或排队后的 Handoff 可视化。
- 从 Command Dock 中心或来源 Agent Card 朝向目标的边缘，到目标 Agent Card 中心的动态弧线飞行。
- 命中时的粒子爆发、飞行头消散和目标卡片轮廓脉冲。
- `Submitted`、`Queued`、`Untracked` 三种可接受 receipt 的视觉反馈。
- Reduced Motion 降级方案。
- 目标移动、Canvas 平移/缩放、窗口变化时的动态终点解析。
- 并发上限、粒子预算、闲置停帧与诊断指标。

### 2.2 Out of Scope

- 移动或复制真实 Agent Card；飞行物只是 receipt 视觉载体。
- 用该特效表示 Agent 开始执行、完成、失败或输出结果。
- `Manual` 来源的卡片队列面板加任务使用 Dock 飞行动画。
- 跨 Workspace/跨窗口绘制一条伪造的连续飞行；当前静默降级为业务回执，不在目标窗口伪装成 Dock 发射。
- 为效果自动聚焦目标、平移 Canvas 或改变用户选择。
- 额外的声音、TTS 或触觉反馈；语音输入只作为 Handoff 来源标记。
- 引入 Bevy、Macroquad、Hanabi 等完整粒子引擎。
- 建设跨功能通用的公共粒子框架。

## 3. 触发语义

### 3.1 用户行为不是直接触发器

以下行为本身都不直接创建粒子特效：

- 用户按下发送或结束一次语音输入；
- Norma/Agent 产生普通文本回复，或语音回合只得到 Norma 自身回复而未调用 `reply_agent`；
- LLM 判断“应该委派”，但尚未执行派发命令；
- 目标 Agent 开始输出、进入工作态或完成任务；
- `ListAgents`、`ActivateAgent`、任务状态查询或取消命令成功。

用户输入只是上游原因。业务触发事件必须是以下真实成功回执之一：

1. `CardCommand::DispatchAgentTasks` 完成协调、提交尝试和最终 receipt 分类后，产生至少一条状态可接受、且来源为 `Norma` 或同 Workspace 可信 `Agent` 的 dispatch receipt。
2. `AgentInputSource::Voice` 穿透 `NormaSession` 与 `ToolContext` 到达 `reply_agent`，且 Adapter 返回带 `reply_id`、`node_id` 的 `sent` 或 `queued` 回执。

### 3.2 触发入口

可能到达 Handoff presentation seam 的上游入口包括：

1. 用户在 Command Dock 通过文字或语音明确要求 Norma 将任务交给某个 Agent；Norma 的工具调用转换为 `AdapterCommand::AssignAgentTask`，再转换为 `CardCommand::DispatchAgentTasks`。
2. Norma 协议或内部调用直接提交 `CardCommand::DispatchAgentTasks`。
3. 用户通过语音继续与已有 Agent Card 对话；`AgentCommand::UserTurn` 明确携带 `AgentInputSource::Voice`，Norma 选择 `AdapterCommand::ReplyAgent` 并获得成功回执。
4. 卡片队列 UI 以 `AgentDispatchSource::manual()` 添加任务。
5. 某个 Agent 通过受权 CLI/协议，以 `AgentDispatchSource::agent(...)` 委派子任务。

前两类中被标记为 `AgentDispatchSource::norma()` 的派发，以及第 3 类明确标记为 Voice 且真实 `sent`/`queued` 的续聊播放 Dock 飞行。第 5 类在来源与目标位于同一 Workspace、两张卡片均可见时播放 Agent Card → Agent Card 飞行。第 4 类仍正常派发但不创建本特效；跨 Workspace Agent 委派和非语音 `reply_agent` 也不创建飞行。

业务代码的统一判定 seam 是：

```text
execute_coordinated_agent_command
  → CardCommand::DispatchAgentTasks
  → AgentTaskCoordinator::dispatch_from
  → 本地提交/排队与失败恢复
  → 计算最终 AgentTaskReceiptStatus
  → 在业务回执确定后，按独立 DispatchVisualOrigin 生成视觉副作用

AgentCommand::UserTurn(input_source=Voice)
  → NormaSession / ToolContext 保留来源
  → AdapterCommand::ReplyAgent
  → reply_adapter 返回 sent / queued + reply_id + node_id
  → spawn_agent_handoff
```

两条路径最终复用同一个 `spawn_agent_handoff` presentation seam。视觉事件必须在最终 `AgentTaskReceiptStatus` 或 `reply_agent` 成功状态确定后生成，不能在用户按下发送、语音弹窗显示/消失、预检开始或协调器刚创建内部 task 时提前生成。

### 3.3 正向触发条件

任务派发 receipt 必须同时满足以下条件：

1. 命令类型为 `CardCommand::DispatchAgentTasks`。
2. 来源为 `Norma`，或来源为同一 Workspace 中带可信 `source_node_id` 的 `Agent`。
3. 最终状态为 `Submitted`、`Queued` 或 `Untracked`。

语音续聊回执必须同时满足以下条件：

1. 输入来源为 `AgentInputSource::Voice`，且实际命令为 `AdapterCommand::ReplyAgent`。
2. Adapter 返回 `reply_id`、目标 `node_id`，状态为 `sent` 或 `queued`。

两条路径还必须满足共同的 presentation 条件：App 当前位于 Canvas 屏幕，且目标 `node_id` 仍对应一张可见 Agent Card。Agent-to-Agent 来源还要求来源 `node_id` 对应同一 Canvas 中的可见 Agent Card。窗口是否活跃不参与触发或保留判定；窗口失活时，成功 Handoff 仍立即进入同一条流星时间线。

状态视觉语义：

| Receipt 状态 | 是否触发 | 视觉语义 |
| --- | --- | --- |
| `Submitted` | 是 | 已提交给可跟踪 Agent |
| `Queued` | 是 | 已进入目标 Agent 的队列/等待提交 |
| `Untracked` | 是 | 已交付，但无法自动观察终态 |
| `Failed` | 否 | 不得用命中特效暗示成功 |
| `Cancelled` / `CancelRequested` | 否 | 属于取消流程，不是新派发 |
| `Done` / `Interrupted` | 否 | 属于后续任务生命周期 |
| `reply_agent: sent` | 是，仅 Voice | 语音续聊已写入目标 Agent 会话 |
| `reply_agent: queued` | 是，仅 Voice | 语音续聊已进入目标 Agent 的等待队列 |
| `reply_agent: failed/cancelled/unknown` | 否 | 不得用命中特效暗示续聊成功 |

### 3.4 多目标派发

- 每条正向 receipt 对应一个目标特效。
- 同一批 receipt 按 assignment 顺序发射，间隔 40ms；间隔只影响视觉，不延迟业务派发或 receipt 返回。
- 同时最多绘制 3 个飞行/命中特效。
- 待播放视觉事件最多保留 8 个；超过上限的事件可跳过动画，但不得丢失业务 receipt、队列记录或状态展示。
- 跳过或合并特效不得被记录为派发失败。

## 4. 端到端触发流程

```text
用户 / 来源 Agent
        │ 文字、语音或受权 CLI 委派
        ▼
Norma Adapter / Protocol
        │ CardCommand::DispatchAgentTasks
        ▼
App Agent Runtime
        │
        ├──► AgentTaskCoordinator::dispatch_from
        │          │
        │          └──► 真实 AgentTaskReceipt(node_id, state)
        │
        ├──► 提交/排队/失败恢复 ──► 最终业务 receipt
        │
        └──► 仅在 Submitted / Queued / Untracked 后
                    │ DispatchVisualOrigin（独立视觉参数）
                    ▼
             MotionStore / DispatchEffectStore
                    │ 只读 presentation
                    ▼
             GPUI Canvas Overlay ──► 目标 Agent Card

失败、离屏、跨 Workspace 或视觉容量溢出：
业务 receipt 按原路径返回；视觉分支跳过，不反写业务状态。
```

语音续聊不创建伪任务，也不改写上述 dispatch 业务流程；它在 `reply_adapter` 返回真实 `sent`/`queued` 后，携带 `voice-reply:{reply_id}` 与目标 `node_id` 进入同一个 Motion presentation seam。语音识别弹窗只是上游展示状态，它提前隐藏、折叠或重新渲染都不得取消已经进入 Adapter 的 Handoff 动效。

以下详细步骤描述新任务派发分支：

1. 用户通过 Dock 发出自然语言或语音指令。
2. 上游解析只有在明确形成 `DispatchAgentTasks` 命令时才进入派发流程；普通消息继续走普通 Dialogue。
3. App 校验目标名称、启用状态、Agent 卡片激活状态、委派约束和队列预算。
4. `AgentTaskCoordinator::dispatch_from` 生成包含 `node_id` 的领域 receipt。
5. App 尝试将任务写入目标卡片或放入队列，并处理可安全恢复的失败。
6. App 将领域状态和提交结果归并为最终 wire receipt 状态。
7. 对每条满足 §3.3 的 receipt 创建 `DispatchReceiptVisualEvent`；创建特效不影响 receipt 返回。
8. 调用方触发 `cx.notify()`，下一次 render 发现 Motion 活跃并启动现有动画帧驱动。
9. `MotionStore::tick(delta)` 推进飞行和命中状态；Canvas 只读取 presentation 并绘制。
10. 最后一个特效结束后，Motion activity 归零，帧调度恢复 idle。

## 5. 起点与终点

### 5.1 起点：Command Dock 中心

飞行起点使用屏幕坐标，不进入 Canvas 世界坐标：

```text
x = window.viewport_width / 2
y = window.viewport_height - DOCK_BOTTOM_INSET - DOCK_HEIGHT / 2
```

按当前布局：

```text
DOCK_BOTTOM_INSET = 22px
DOCK_HEIGHT = 52px
y = viewport_height - 48px
```

实现时必须把 Dock 尺寸常量与 origin 计算收敛到同一职责，避免渲染布局和特效锚点分别维护魔数。效果 overlay 位于 Agent Card 上方、Workspace chrome 下方，因此 receipt 在 Dock 中心生成但被 Dock 遮住，离开 Dock 后才可见，形成“从 Dock 内发射”的效果。

窗口尺寸在飞行期间变化时，起点可随当前 Dock 中心重新解析；首帧必须与当前可见 Dock 对齐。

### 5.2 起点：来源 Agent Card 朝向目标的边缘

同 Workspace Agent-to-Agent 委派保存 `DispatchVisualOrigin::AgentNode(source_node_id)`，每帧读取来源卡片当前 presentation rect，并沿“来源中心 → 目标中心”的方向与矩形边界求交。发射点落在朝向目标的一侧边缘，避免从卡片中央穿出，也不会因左右布局变化而反转因果。

来源卡片必须仍是同一 Canvas 中可见的 Agent Card。来源离屏、被删除或不再是 Agent 时，仅跳过/取消视觉副作用；已形成的业务 receipt、队列和任务 lineage 均保持原样。跨 Workspace 因不存在单一连续屏幕坐标系，不绘制伪造飞行。

### 5.3 终点：目标 Agent Card 当前视觉中心

业务 receipt 提供稳定的目标 `node_id`。特效不得在 spawn 时只保存一个固定屏幕点，而应保存 `NodeId`，每帧按以下顺序解析：

1. 从 Runtime/Canvas Document 读取目标 Agent Card 的 canonical rect。
2. 从 `MotionStore` 读取该节点当前 presentation rect，而不是只读最终 layout target。
3. 取 presentation rect 的中心点。
4. 通过当前 `Viewport::canvas_to_screen` 转换为屏幕坐标。

因此以下操作发生时，终点仍与用户看到的卡片中心一致：

- 卡片正在布局动画或被拖动；
- Canvas 正在平移或缩放；
- 窗口尺寸发生变化。

渲染层不得为了获取终点再次调用会修改 Motion 状态的 `resolve_node_visual_at`。应增加只读的 current presentation rect 查询 seam，保持 render 纯净。

### 5.4 端点不可用

- receipt 创建前目标不存在或不是 Agent Card：不创建特效，并记录结构化诊断；业务结果保持原样。
- Agent-to-Agent 来源不存在、不是 Agent Card 或中心离屏：不创建特效；业务结果保持原样。
- 飞行中来源被删除：立即取消相关 effect，不在目标处播放悬空爆炸。
- 飞行中目标被删除：特效快速淡出，不播放爆炸，不自动恢复卡片。
- 目标中心在可视区域外：首版不自动平移 Canvas；不播放跨屏飞行，仅保留现有 receipt/队列状态反馈。
- 飞行中目标离开可视区域：取消爆炸并淡出飞行物。

## 6. 界面与交互设计

### 6.1 视觉阶段

#### Flight

- 时长：固定 480ms；作为跨屏空间说明动效，优先保证轨迹可辨识。
- 路径：Dock 来源沿既有上拱贝塞尔飞向目标；Agent 来源按任意方向向量与稳定法线生成轻微上拱曲线。两者都用 48 段查找表做弧长重参数，避免参数曲线造成明显忽快忽慢。
- 动态：路程使用 `p + sin(πp) × 0.09` 的自然前冲节奏，保持起终点精确并在接近目标时收束。
- 组成：一个带微型卡片轮廓的白热离子核心、两道椭圆环、两股相差 π 的螺旋、5 个尾迹离子点，以及深色衬底/主题色/白色核心三层主轨迹。
- 尾迹：最多保留最近 19% 路程，主轨迹采样 24 段、每股螺旋采样 22 段；最后 16% 路程缩短尾迹并将核心收束 24%。
- 发射：飞行前 22% 在真实 origin 播放一次短促 plume 与 5 道沿初始切线展开的放射线；Dock 来源保持原有垂直向上方向。
- receipt 可采用微型卡片/纸片轮廓，但不能与真实可拖拽卡片混淆，也不参与命中测试。

#### Impact

- 时长：固定 430ms，以便辨识参考动画中的主爆发和延迟次级火花。
- 目标中心产生固定 28 颗短寿命粒子：20 颗主粒子（16 Comet + 4 Mote）和延迟 52–86ms 出现的 8 颗 Spark。
- 粒子使用指数阻尼、轻微切向 twist 和小幅重力偏移；方向、距离、寿命在 spawn 时按稳定 seed 一次性生成。
- 命中中心同时绘制扩散光晕、8 道星芒、冲击波、3 段旋转弧和短促水平 flare；所有层都随生命周期淡出。
- 目标卡片外侧绘制一次轮廓脉冲；不缩放真实卡片，不改变布局、焦点或 hitbox。
- 爆发应表现为能量“命中/收到”，不能配合任务完成文案或完成音效，避免被理解为庆祝任务结束。

#### Finished

- 清理 effect slot 和粒子数据。
- 如果没有其他 Canvas 动效，停止逐帧刷新。

### 6.2 状态差异

- `Submitted`：使用主题 accent/cyan 的标准强度。
- `Queued`：降低亮度与爆发半径，表达“已接收但在等待”。
- `Untracked`：使用中性主题色，避免暗示系统能继续确认终态。
- 不得只依赖颜色区分状态；真实状态仍由文字、badge 或队列 UI 承担，粒子只负责定位目标。

### 6.3 交互约束

- 动画不阻塞输入，用户可在播放期间拖动、缩放、输入或再次派发。
- overlay 不注册鼠标事件、不创建可聚焦元素、不遮挡 Dock 或卡片。
- 不自动选择、不聚焦、不滚动画布。
- 连续高频派发受 §3.4 的容量限制，不延长业务响应时间。

### 6.4 Reduced Motion

当 `MotionStore.reduce_motion` 为 true：

- 取消跨屏位移动画、长尾迹和爆炸散射。
- Dock 来源在 Dock 中心播放一次 120–160ms 的低幅度亮度反馈。
- Agent 来源在来源卡片外沿播放一次 120–160ms 的低幅度轮廓反馈。
- 可见目标卡片播放一次 120–160ms 的边框/透明度脉冲。
- 两端反馈可以同帧发生，以保留“从这里派到那里”的因果，但不得制造大范围运动。

该设计落实 Apple 的空间一致性、即时反馈与 Reduced Motion 原则；同时遵守高频动效应短促、非阻塞、低干扰的设计工程约束。

## 7. 技术实现方案

### 7.1 技术选型

采用“App 内轻量状态机 + 单个 GPUI Canvas overlay”的组合：

- `MotionStore` 负责 effect 生命周期、时间推进、容量和 presentation。
- GPUI Canvas 负责尾迹、receipt、粒子和脉冲绘制。
- 不为每颗粒子创建 GPUI element。
- 不使用 GPUI `Animation` 关键帧驱动主飞行或爆炸。
- 不引入第三方游戏/粒子运行时。

理由：动态目标需要每帧根据 `NodeId` 和 `Viewport` 重算落点；多粒子需要共享生命周期、限流和一次绘制；现有 `MotionStore::tick` 已经提供统一、可测试的帧驱动。

### 7.2 数据与事件模型

建议 App 层新增：

```rust
struct DispatchReceiptVisualEvent {
    task_ref: String,
    target_node_id: NodeId,
    status: DispatchReceiptVisualStatus,
    origin: DispatchVisualOrigin,
}

enum DispatchVisualOrigin {
    CommandDock,
    AgentNode(NodeId),
}

enum DispatchReceiptVisualStatus {
    Submitted,
    Queued,
    Untracked,
}
```

`task_ref` 只用于幂等/诊断，不在逐帧绘制中格式化或显示任务正文。视觉事件不进入 `agent`、`canvas`、`runtime` 或 `storage` crate，不新增持久化。

### 7.3 MotionStore 集成

实现结构：

```text
MotionStore
├── nodes
├── initializations
└── dispatch_effects: DispatchEffectStore
    ├── active effects
    └── bounded pending events
```

集成要求：

- `MotionStore::tick(delta)` 同步推进节点动画、初始化动画和 dispatch effects。
- 对 effect 的单帧 `delta` 设置 32–50ms 上限，避免调度停顿后穿越终点。
- `MotionStore::activity_summary().canvas_active` 包含 `dispatch_effects.is_active()`。
- `retain_nodes` 或等价清理逻辑在来源/目标删除时取消相关 effect。
- `MotionStore` 提供只读 `current_node_rect(NodeId)` 或等价查询。
- render 只读取 `DispatchEffectPresentation`，不得修改 effect 状态。
- 不创建独立 timer、后台线程或 `request_animation_frame` 循环。

### 7.4 渲染层级

根元素顺序调整为：

```text
workspace backdrop
canvas + cards
dispatch effect overlay
workspace chrome（Dock / top bar / minimap）
error/settings/voice overlays
```

`render_dispatch_effects` 只在存在 presentation 时返回 Canvas element；无特效时不挂载空的全屏绘制层。

### 7.5 初步代码结构

```text
crates/app/src/motion/
├── mod.rs
└── dispatch_receipt.rs
    ├── DispatchEffectStore / DispatchEffect
    ├── DispatchEffectPhase / DispatchEffectPresentation
    └── DispatchImpactParticle[28] / DispatchImpactParticleKind

crates/app/src/effects/
├── mod.rs
├── dispatch_receipt.rs（overlay、动态 target、paint plan）
└── dispatch_receipt/
    ├── geometry.rs（Bezier、48 段弧长表、双螺旋）
    ├── flight.rs（离子核心、尾迹、发射 plume）
    ├── impact.rs（冲击层、28 粒子解析运动）
    └── primitives.rs（主题样式与低成本绘制原语）

crates/app/src/app/
├── render.rs
├── chrome/dock.rs
└── agent_runtime/commands.rs
```

模块职责：

| 模块 | 职责 |
| --- | --- |
| Dialogue / Adapter / Norma Protocol | 将明确委派意图转换为 `DispatchAgentTasks`；不决定动画 |
| `agent::AgentTaskCoordinator` | 校验并生成真实领域 receipt 和稳定目标 `node_id`；零 GPUI |
| `app::agent_runtime::commands` | 汇总提交结果，计算最终 wire status，生成 App 层视觉事件 |
| `MotionStore` | 管理 effect 状态、时序、限流和调度活跃度 |
| Runtime / Canvas Document | 提供目标卡片 canonical rect 和节点存活状态 |
| Viewport | 将目标中心从 Canvas 坐标转换到屏幕坐标 |
| Dispatch Effect Renderer | 用 GPUI Canvas 绘制只读 presentation |
| Animation Driver | 以现有 16ms Canvas cadence 驱动 tick，并在结束后停帧 |
| Command Dock | 提供统一的视觉 origin 几何定义，仍位于 effect 上层 |
| 来源 Agent Card | 提供当前 presentation rect 与面向目标的边缘 origin；不改变真实布局 |

### 7.6 幂等与重复事件

- 同一 `task_ref + target_node_id` 在一个 effect 生命周期内最多创建一次视觉事件。
- 重试若返回既有业务 receipt，不得重复播放成多次“新派发”。
- 去重仅影响视觉，不影响领域任务幂等和业务回执。

## 8. 性能考虑

### 8.1 预算与容量

- 单次命中粒子：固定 28（20 主粒子 + 8 次级火花）。
- 单个飞行主尾迹采样 24 段、双螺旋各 22 段、弧长查找表 48 段。
- 同时 active effect：最多 3。
- pending visual event：最多 8。
- 总活动爆发粒子：最多 3 × 28 = 84；每个 effect 内用 `[DispatchImpactParticle; 28]` 固定数组，无逐帧粒子分配。

### 8.2 分配与复用

- 首版使用预分配 `Vec`、固定数组或小容量 slot store；不建设重量级通用对象池。
- effect 完成后复用 slot；spawn 时一次性计算粒子方向、速度、寿命和确定性 seed。
- 帧循环内禁止随机数生成、字符串格式化、日志正文拼接和无界容器增长。
- 尾迹按贝塞尔曲线和弧长查找表解析采样，不维护每帧 append 的轨迹历史。

### 8.3 绘制成本

- 所有特效共享一个全屏 Canvas overlay。
- Mote 与离子核心使用 `paint_quad`；需要方向感的 Comet/Spark 使用短 `PathBuilder` 线段，避免无旋转 quad 形成圆点爆炸。
- 同宽 Comet 合并为最多 4 条断续 path，8 个 Spark 合并为 1 条 path；单个 Flight 最多约 9 条 path，单个 Impact 峰值 11 条 path 和约 30 个 quad，3 路并发约 33 paths + 90 quads。
- paint plan 按每 effect 16 paths / 36 quads 预分配，避免标准帧内扩容；普通粒子不使用独立 blur/shadow。
- 不使用每帧 CPU RGBA 图片生成，不引入 `RenderImage` 路径。
- overlay 不参与布局和输入命中。

### 8.4 帧调度

- 复用现有 Canvas active cadence（当前 16ms）。
- 特效为空时 `canvas_active` 不得因本功能保持 true。
- 目标不可见或非 Canvas 屏幕时不创建/不继续无意义的旧特效；窗口失活不取消已接受的 Handoff 特效。
- 不把任何 SQLite、网络、文件 IO 或重 CPU 工作放入 `MotionStore::tick` 或 paint。

### 8.5 观测指标

在现有 frame diagnostics 上增加或扩展以下可选指标：

- active/pending dispatch effect 数量；
- 当前粒子数量；
- effect update/build/paint 耗时；
- 因容量、目标不可见或 Reduced Motion 跳过的数量；
- effect 生命周期结束后是否仍存在连续帧请求。

建议目标：在基线 Mac 上，1 个特效增量 update + presentation build + paint p95 小于 0.75ms，3 个并发小于 1.5ms；该数值需在实现后以真实 App 测量确认。

实现已接入 `MULTISOUL_FRAME_DIAGNOSTICS=1`：`motion_sample` 输出 dispatch active、pending、粒子、跳过/取消计数与 update 耗时，`dispatch_effect_paint` 输出 effect/path/quad 数量及 build、paint、total 耗时。2026-08-01 在真实开发 App 的 Paper Blue Impact 固定帧采样 8 次，实际为 10 paths + 30 quads，build + paint p95/max = 0.187041ms；该样本证明单 effect 低于 0.75ms 目标，但 3 路并发 p95 仍需在目标验收机器实测，不能由单帧预览外推替代。

## 9. 状态、错误与边界情况

| 场景 | 预期行为 |
| --- | --- |
| 用户发送普通消息 | 不触发 |
| 模型只回复文字、未调用派发或 `reply_agent` | 不触发 |
| 语音 `reply_agent` 返回 `sent` / `queued` | 命中真实目标 Agent Card，不创建 coordinated task |
| 非语音 `reply_agent` 成功 | 不触发语音 Handoff 特效 |
| 语音识别弹窗提前消失 | 不影响已进入 Adapter 的成功 Handoff 特效 |
| 未知/禁用/不活跃目标导致预检失败 | 不触发，保留原错误反馈 |
| 最终 receipt 为 `Failed` | 不触发成功命中 |
| 任务进入目标队列 | 以 `Queued` 风格触发 |
| Untracked Agent 接收任务 | 以中性风格触发，不暗示后续可跟踪 |
| 一次派发多个 Agent | 按 receipt 顺序错峰播放并遵守容量上限 |
| 同 Workspace Agent CLI 委派 | 从来源 Agent 朝向目标的卡片边缘发射，并命中目标中心 |
| 跨 Workspace Agent CLI 委派 | 业务照常路由；不在目标窗口伪造 Dock/来源流星 |
| 来源 Agent 离屏或被删除 | 跳过/取消视觉，不改变业务 receipt，不在目标处爆炸 |
| 目标飞行中移动 | 终点跟随当前 presentation center |
| Canvas 平移/缩放 | 每帧重算目标屏幕坐标 |
| 目标删除或移出可见区 | 淡出/取消，不爆炸、不自动导航 |
| Reduced Motion | Dock 或来源 Agent outline + 目标 pulse，无跨屏飞行 |
| 动画期间再次派发 | 独立加入有界 effect store，不阻塞前一个任务 |
| 窗口失活但仍在 Canvas | 继续播放已接受 Handoff 的流星，不因失活取消 |
| 窗口切换到非 Canvas | 取消旧动画，不在恢复后补播 |

## 10. 验收标准

### 10.1 触发正确性

- [ ] 用户发送普通文字或语音消息，既未形成 `DispatchAgentTasks`、也未形成 Voice `reply_agent` 成功回执时，特效状态不存在。
- [ ] Voice `reply_agent` 返回 `sent` 或 `queued` 时创建且只创建一个对应目标的特效，不创建 coordinated task。
- [ ] 窗口失活但仍位于 Canvas 且目标可见时，成功 Voice Handoff 继续创建、保留并推进对应特效。
- [ ] 非语音 `reply_agent` 即使成功，也不创建 Voice Handoff 特效。
- [ ] 语音识别弹窗的显示、隐藏或折叠状态不参与 Handoff 动效判定。
- [ ] Norma 来源的 `Submitted` receipt 创建且只创建一个对应目标的特效。
- [ ] Norma 来源的 `Queued` receipt 创建一个队列风格特效。
- [ ] Norma 来源的 `Untracked` receipt 创建一个中性风格特效。
- [ ] `Failed` receipt 不创建飞行或命中爆炸。
- [ ] `Manual` 来源不创建飞行特效。
- [ ] 同 Workspace 可信 `Agent` 来源创建 Agent Card → Agent Card 飞行，来源与目标 node_id 精确匹配。
- [ ] 跨 Workspace `Agent` 委派不在目标窗口伪造 Command Dock 飞行，业务 receipt 仍正常返回。
- [ ] `ListAgents`、`ActivateAgent`、状态查询、取消和任务完成事件均不创建本特效。
- [ ] 同一 `task_ref + target_node_id` 重复进入视觉 seam 时不会重复播放。

### 10.2 空间与交互

- [ ] 首帧起点位于实际 Command Dock 中心，Dock 布局常量改变时不会产生独立魔数漂移。
- [ ] Agent-to-Agent 首帧起点位于来源卡片朝向目标的边缘，左右方向都不退化为卡片中心。
- [ ] 终点使用 receipt 的 `node_id` 对应 Agent Card 当前 presentation rect 中心。
- [ ] 飞行期间拖动目标卡片，receipt 仍命中移动后的视觉中心。
- [ ] 飞行期间平移或缩放 Canvas，终点仍与屏幕上的卡片中心一致。
- [ ] 特效不改变卡片真实 bounds、焦点、选择、输入路由或 hitbox。
- [ ] effect overlay 位于卡片之上、Dock/chrome 之下，并且不拦截鼠标事件。
- [ ] 目标删除或离开可见区后不播放悬空爆炸。
- [ ] 来源 Agent 删除或离开可见区后取消飞行，目标仍存在也不播放悬空爆炸。

### 10.3 动效与无障碍

- [ ] 默认 Flight 在 480ms 完成，Impact 在 430ms 完成；单帧 delta 最大 50ms，不因窗口恢复瞬间跳完。
- [ ] 飞行轨迹按 48 段弧长表推进；从 8%–92% 路程采样的视觉速度峰谷比不超过 2.40，最终坐标与目标中心误差为 0px。
- [ ] 命中包含 16 Comet、4 Mote、8 delayed Spark，并清楚表达 receipt 落点而非任务完成庆祝。
- [ ] Reduced Motion 下没有跨屏位移和粒子散射，同时真实来源（Dock 或 Agent）与可见目标仍有短促状态反馈。
- [ ] 高对比度、浅色和深色主题下，反馈可见但不只依靠颜色承载业务状态。

### 10.4 性能与调度

- [ ] 同时 active effect 不超过 3，pending visual event 不超过 8，总粒子不超过 84。
- [ ] 粒子不以独立 GPUI element 实现，所有特效共享单一 Canvas overlay。
- [ ] 单个 Impact 的 path 不超过 16、quad 不超过 36；普通粒子不使用独立阴影。
- [ ] effect 生命周期结束后 `canvas_active` 恢复 false，本功能不再触发连续 16ms frame。
- [ ] 容量溢出只跳过视觉，不影响业务 receipt、队列和持久化状态。
- [ ] 基线 Mac 上 1 个特效增量 update + build + paint p95 目标小于 0.75ms，3 个并发小于 1.5ms，并记录实测结果。

### 10.5 自动化证据

实现阶段必须先按项目 TDD 规范补失败测试，并覆盖：

1. App 单元测试：状态筛选、来源筛选、视觉幂等、阶段边界、`delta` clamp、容量、目标删除和 Reduced Motion。
2. Frame schedule 单元测试：特效存活时为 active cadence，清理后返回 idle；必须同时包含正断言和负断言。
3. `crates/e2e` L2：从生产 WireRequest/`DispatchAgentTasks` 到 effect projection 的闭环；覆盖 Dock 来源、同 Workspace Agent 来源、跨 Workspace无伪造飞行、`Failed` 与普通消息。
4. 真实 App L3/Computer Use：验证 Dock 起点、动态命中、层级、主题、Reduced Motion、拖动/缩放/窗口 resize 和并发视觉。
5. 性能诊断：记录 1 个与 3 个并发特效的帧增量以及动画结束后的停帧证据。

所有新增单元测试必须使用项目规定的完整文档注释、数值推导、具体正负断言和断言说明字符串。

## 11. 风险与权衡

- **状态误读风险**：爆炸容易被理解为任务完成。通过触发时机、克制视觉和 receipt 文案保持“已路由”语义。
- **来源误读风险**：Agent-to-Agent 委派若仍从 Dock 发射会产生错误空间因果；通过独立 `DispatchVisualOrigin` 保留真实来源，跨 Workspace 则明确不绘制连续飞行。
- **动态目标抖动**：每帧直接追踪快速拖动目标可能让尾迹轻微变形。首版优先保证真实命中；若实测明显抖动，再增加帧率无关的目标滤波，而不是固定旧终点。
- **GPUI 路径成本**：`PathBuilder` 存在 CPU tessellation；为还原参考动画，Comet/Spark 使用批处理短路径，必须通过 16 paths/effect 上限与 frame diagnostics 守住预算。若真实机 p95 超标，第一降级项是把 Spark 改为单 quad，而不是降低业务帧响应。
- **视觉丢弃**：容量压力下允许跳过部分动画，以换取稳定交互；业务 receipt 永不丢失。
- **高频干扰**：该反馈可能每天出现数十次；为还原参考页的两段结构，默认总时长为 480 + 430 = 910ms，但不阻塞交互，并通过批量限流与 Reduced Motion 控制干扰；后续可基于真实频率决定是否增加“降低特效强度”设置。

## 12. 后续演进（非首版承诺）

- 跨 Workspace 可增加来源窗口发送 pulse + 目标窗口接收 pulse，但仍不伪造跨窗口连续轨迹。
- Offscreen 目标通过 minimap 或边缘方向指示，而不是自动平移 Canvas。
- 基于使用频率自适应降低粒子数量或仅保留目标 pulse。
- 在多个功能确实共享相同需求后，再抽取通用 effect primitive；首版保持 dispatch 专用。
