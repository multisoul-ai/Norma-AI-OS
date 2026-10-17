# 本地语音对话管线 SPEC

> 日期：2026-07-09 · 状态：采访已确认，待实施  
> 来源：interview-spec 多轮采访 + 冲突审查修订  
> 取代方向：旧 OpenAI Realtime + Node sidecar 方案已从当前文档树移除，可从 Git 历史查阅。

> 历史：见 git 提交。原文路径：`docs/superpowers/plans/2026-07-09-local-voice-pipeline-01-speech-skeleton.md`、`docs/superpowers/plans/2026-07-09-local-voice-pipeline-02-llm-and-card-tools.md`。

待确认：本文仍写「待实施」。对应步骤清单已删除，是否已被后续语音规格取代，本次不改状态。

## 1. 背景与目标

将现有 **OpenAI Realtime + Node sidecar** 路径，重构为：

```text
sherpa-onnx 本地语音前端（VAD / ASR / TTS）
  + 可替换 Client 抽象
  + LlmClient（MVP：Mock → Ollama）
  + 扁平 card tool → card_task 串行执行
```

成功时可见变化：

- 去掉 Realtime / sidecar，语音链路本地化
- `AsrClient` / `VadClient` / `TtsClient` / `LlmClient` 可独立替换
- VAD 驱动起停与打断更稳
- 边聊边操作画布架构更扁、更好维护

说明：口语里的「完全使用 sherpa-onnx」指 **听/说/VAD 本地化**；「想」由 `LlmClient` 承担（Mock / Ollama），不是 sherpa 的能力。

## 2. 范围

### 2.1 In Scope

- sherpa-onnx：**VAD + 流式 ASR + TTS（MVP 整句合成；抽象预留流式）**
- Client 四件套；MVP 先 **Mock `LlmClient`**，再接 **Ollama**
- Always Listening；底部条：`Listening` / `Thinking` / `Speaking`
- 扁平 tool：`Create` / `Focus` / `SendText` → `card_task` 串行执行
- **同一轮多轮 tool calling**：`Create` 的 tool result 必须带回 `node_id`，随后的 `Focus` / `SendText` 使用该 id（不再依赖独立 Planner / `*ToPrevious` 编译层）
- 任务结果：**模板句 TTS**，不再二次 LLM
- 用户打断：只停 TTS，**进行中的 card task 继续**；完成后按模板汇报
- 关 Always Listening：停麦、停 TTS、取消未完成任务；**模型进程内常驻**（见 §6.3）
- 关窗 / 进程退出：停麦、停 TTS、取消任务，并释放模型句柄
- 删除 OpenAI Realtime sidecar 及相关无用路径（按 §6.4 分步，避免半删除导致无法编译）
- 逐步交付：优先语音骨架，LLM 可先 Mock

### 2.2 Out of Scope（MVP）

- 说话人分离 / 多说话人
- 关键词唤醒（靠 Always Listening 开关）
- 云端 fallback LLM
- 流式 TTS 实现（仅预留接口）
- 卡片 tool：`List` / `Delete`（**Phase 2**；能力保留在路线图，不与「不能砍」冲突——MVP 收窄交付面）
- 独立 Planner / OpenAI HTTP Planner 层
- 任务活动面板、卡片 chrome badge、ComputerUse 等

### 2.3 与旧 MVP 的关系

| 旧 realtime-card-task-mvp | 本 SPEC |
|---------------------------|---------|
| OpenAI Realtime 语音 | sherpa VAD/ASR/TTS |
| Node sidecar JSONL | 删除 |
| `enqueue_card_task(intent)` + Planner | 扁平 `Create`/`Focus`/`SendText` + 多轮 tool |
| 任务结果回灌 Realtime 再口语化 | 模板句 TTS |
| List/Delete 在 MVP | 推迟到 Phase 2 |

旧文档与依赖 Realtime 的 e2e 用例在迁移完成后标记废弃或改写，不再作为验收依据。

## 3. 用户与使用场景

1. 打开 Always Listening → 说话 → VAD 分段 → ASR → Mock/Ollama 回复 → TTS 播报  
2. 播报中再次开口 → 立刻停 TTS，继续 Listening（任务不因此取消）  
3. 语音要求建卡并输入 → LLM 先 `Create`，拿到 `node_id` 后再 `SendText` → 后台执行 → 模板句播报  
4. 任务执行期间仍可继续聊天  
5. Ollama 未安装/模型未就绪：开启 Listening **失败**，底部条给出明确错误（引导安装/拉模型）  
6. 关闭 Listening：停止采集与播报、取消未完成任务；ASR/VAD/TTS/LLM 句柄保持常驻以便下次快速进入  

## 4. 业务流程与状态

### 4.1 主路径

```text
Mic → VadClient → AsrClient → LlmClient(Mock → Ollama + tools)
                                ├─ 口语回复 → TtsClient（整句）→ Speaker
                                └─ tool calls（可多轮）
                                     Create / Focus / SendText
                                     → card_task 串行执行
                                     → 模板句 → TtsClient
```

### 4.2 会话主状态

```text
Idle → Listening → Thinking → Speaking → Listening …
```

- card task **不占用**主状态机；并行在后台跑  
- 底部条只反映主状态：`Listening` / `Thinking` / `Speaking`  
- 无独立任务面板  

### 4.3 Create → SendText（已拍板）

采用 **同一轮对话内多轮 tool calling**：

1. LLM 调用 `Create`（或等价 schema）  
2. executor 返回 tool result，**必须包含**新节点 `node_id`（及必要摘要）  
3. 同一 turn 内 LLM 再调用 `SendText { node_id, text, submit? }` / `Focus { node_id }`  
4. 不允许再引入独立 Planner，把自然语言 intent 编译成 `SendTextToPrevious`

`card_task` 入队对象从「原始 intent 字符串」演进为「已解析的 command / 小步列表」（由对话层在 tool 边界完成解析）。

### 4.4 打断与取消

| 事件 | TTS | card task | 模型句柄 |
|------|-----|-----------|----------|
| 用户开口打断播报 | 立即停止 | **继续** | 保持 |
| 用户说「取消」等（若暴露 cancel tool） | 按策略 | 可 cancel | 保持 |
| 关闭 Always Listening | 停止 | **取消未完成** | **常驻** |
| 关窗 / 进程退出 | 停止 | 取消 | **释放** |

MVP 是否暴露语音 `cancel_card_task` tool：建议 **Phase 2**；关闭 Listening 已覆盖硬取消。

### 4.5 模板汇报与播报冲突

任务完成时的模板 TTS：

- 若主状态为 `Listening` 且用户未在说话：可以播报  
- 若主状态为 `Speaking`（AI 正在播主回复）：**排队**，主回复结束后再播模板（不打断主回复）  
- 若用户正在说话（VAD active）：**推迟**到本段用户话结束并完成当轮回复策略之后（避免抢麦）  

具体队列实现可简单 FIFO；SPEC 验收只需行为符合上表。

## 5. 数据模型与接口（实现级要点）

### 5.1 Clients

- `VadClient`：PCM in → speech start / end（及可选置信度）  
- `AsrClient`：PCM in → partial / final transcript  
- `TtsClient`：text in → 音频 out；MVP 整句；API 形状预留流式 chunk  
- `LlmClient`：messages + tools → text / tool_calls /（多轮）tool results 继续  

MVP：`MockLlmClient` 可脚本化回复与假 tool call，便于无 Ollama 时跑通状态机。

### 5.2 卡片 tools（MVP）

- `Create` → result 含 `node_id`  
- `Focus { node_id }`  
- `SendText { node_id, text, submit? }`  

Phase 2：`List` / `Delete` / 可选 `CancelTask`。

### 5.3 DialogueSession

编排：采集权限、VAD/ASR、调用 LLM、TTS 播放与打断、tool → `card_task`、模板汇报队列、Listening 开关与关窗收口。

## 6. 技术实现概览

### 6.1 Crate 落点

| 落点 | 职责 |
|------|------|
| 新建或扩展语音域（优先扩 `voice_sherpa`，或抽 `crates/voice` 承载 trait + PCM） | Mic、sherpa VAD/ASR/TTS adapter |
| 新建 `crates/llm` | `LlmClient` trait、Mock、Ollama |
| `crates/card_task` | 保留串行队列与执行；去掉对 Realtime Planner / intent 编译的硬依赖 |
| `crates/app` | Always Listening UI、播放、会话编排、关窗 |
| 删除 | Node Realtime sidecar、OpenAI Realtime 专用 IPC/协议路径 |

依赖方向：域 crate 零 GPUI；`canvas` / `terminal` / `runtime` / `storage` 不依赖 Ollama、sherpa、sidecar。

### 6.2 模型与配置

- ASR/VAD/TTS：**模型目录可配置**；MVP 官方只验证一套中文组合（文档 + 下载脚本）  
- Ollama：可配置 host/model；不可用则禁止进入 Listening，底部条报错  

### 6.3 模型生命周期（已拍板）

- **进程内常驻**：首次成功进入 Listening（或显式预热）后加载，关闭 Listening **不卸载**  
- **关窗 / 进程退出** 时释放 ASR/VAD/TTS/LLM 相关句柄与子资源  
- **首响计时点**：从「已处于 Listening 且本轮用户话 VAD end」起，到 TTS 开始出声；不含冷启动加载  

### 6.4 分步实施顺序（必须遵守）

当前 `voice_sherpa` 的 PCM stream 依赖 `realtime::PcmAudioChunk`。禁止「先删光 realtime 再开发」导致长期无法编译。

1. **抽中立 PCM / 会话基础类型**（移出对 sidecar 的依赖）  
2. **删除** OpenAI Realtime sidecar、JSONL Realtime 协议、app 内 Realtime 专用路径中确认无用的部分  
3. **语音骨架**：Mic → VAD → ASR（TTS/LLM 可先假）  
4. **TTS**：整句播报 + 打断停播  
5. **Mock `LlmClient`**：串起 Listening / Thinking / Speaking  
6. **扁平 card tools**：Mock tool → Create/Focus/SendText（含多轮 `node_id`）+ 模板汇报  
7. **Ollama `LlmClient`**：替换 Mock，真 tool calling  
8. **打磨**：首响、误触发、资源收口测试、e2e 改写  

### 6.5 延迟目标

- 目标：用户说完后约 **1–2 秒** 开始出声（stretch）  
- MVP 门禁：闭环正确、可打断、资源收口正确；延迟不达标不阻塞阶段 1–6 合并，但阶段 7/8 需记录实测并专项优化  

## 7. UI/UX

- 底部条：`Listening` / `Thinking` / `Speaking`  
- 任务无独立面板、无强制 modal  
- 错误可见：麦克风权限、模型缺失、Ollama 不可用  

## 8. 状态、错误与边界

- VAD：优先 **低误触发 + 打断顺滑**；漏检后续再调  
- Ollama/模型缺失：开启 Listening 失败，不进入半开状态  
- tool 缺 `node_id` 或非法参数：任务失败 + 模板错误播报，不崩溃  
- 多任务：保持串行队列（与现 `card_task` 一致）  

## 9. 非功能性需求

- 前台不阻塞：识别、LLM、TTS 合成不在 GPUI foreground 同步重活  
- 可扩展：任一 Client 可替换（例如未来非 sherpa ASR、非 Ollama LLM）  
- 密钥与日志：不打印 API token；本地路径可配置  

## 10. 风险、权衡与未决

### 已做权衡

- 去 Planner：换简单架构，复杂多步指令变脆 → MVP 只三 tool + 多轮 tool  
- 模板汇报：降自然度，换稳定与延迟  
- 模型常驻：换内存占用，换首响与开关体验  
- Mock 先行：语音可并行推进  

### 已知风险

- 本地小模型 tool calling 不稳 → 需要对 tool JSON 做校验；失败可短重试或模板报错（实现阶段细化）  
- 整句 TTS + ASR endpoint + LLM 难稳达 1–2s → 标 stretch  
- 旧 e2e / 设计文档大面积失效 → 迁移清单要单列  

### 仍可在实现计划里细化（不阻塞本 SPEC）

- Mock 脚本的具体 DSL  
- Ollama 默认模型名与最低版本  
- 模板句文案表  
- cancel tool 是否进入 Phase 2  

## 11. 验收标准

- [ ] Always Listening 开关 + VAD 起停 + 流式 ASR 出字  
- [ ] 说完后回复 + sherpa TTS 播报，用户可打断停播  
- [ ] 语音触发 Create →（tool result `node_id`）→ Focus/SendText；任务后台串行执行；模板句汇报  
- [ ] 关闭 Listening：停麦/TTS、取消未完成任务、模型常驻；关窗释放句柄  
- [ ] Ollama 不可用时无法进入 Listening，错误可见  
- [ ] Client 抽象与会话状态机有自动化测试；真麦/真模型可 `#[ignore]`  
- [ ] Realtime sidecar 路径已删除；PCM 类型不再依赖 sidecar crate  
- [ ] 旧 realtime-card-task e2e/文档已废弃或改写说明  

## 12. 采访决策摘要

| 项 | 决策 |
|----|------|
| sherpa 范围 | 听/说/VAD；想 = LlmClient |
| LLM | 抽象 + Mock 先行 + Ollama |
| Client 切分 | Asr / Vad / Tts / Llm 分立 |
| Realtime sidecar | 删除（分步，先抽 PCM） |
| 画布 | 扁平 tool；去 Planner |
| node_id | 同轮多轮 tool，Create 回传 |
| 任务汇报 | 模板 TTS |
| MVP tools | Create / Focus / SendText |
| List/Delete | Phase 2 |
| 打断 | 停 TTS，任务继续 |
| 模型生命周期 | 常驻；关窗释放 |
| TTS | MVP 整句，预留流式 |
| VAD 优先级 | 低误触发 + 打断 |
| 首响 | 1–2s stretch |
| SPEC 路径 | 本文 |
