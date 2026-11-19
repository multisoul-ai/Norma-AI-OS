# OpenBot Channel 协作：源码证据

核对日期：2026-11-12。源码：`/tmp/openbot-research-20261112`；固定提交：`7dae493c89a6c8d5c42a5669d67fc872bbfc0b67`。这是静态源码调研，未启动 OpenBot 或运行其测试。

## 1. 先纠正“一个固定主 Agent”的理解

Channel 有成员和 lead，但 **lead 与任务 owner 是两种职责**。显式收件人、开头的 @mention、回复关系等可确定 owner；无法确定时，lead 的 provider/model 在独立、无工作工具的 session 中输出路由结果。新版可为同一请求选多个成员，分别建立独立 root task；每个 task 仍只有一个 owner。最小复刻可固定只有一个主 owner，省掉模型路由与多 root 功能。

证据：[路由设计与多 root 语义](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/docs/architecture/channels-and-messaging.md#L215-L270)、[路由落库](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L1751-L1787)。

## 2. 派发是宿主维护任务状态，不是模型彼此直接对话

工具调用由运行时绑定 channel、agent、turn 身份。`channel_assign` 的参数含 `recipientAgentId`、`task`、`expectedResult`、`sourceMessageIds`，可选 `resources` 和 `dependencies`。宿主检查成员、禁止自派发、检查来源消息与循环依赖，然后原子记录子任务、父依赖和可见派发消息。

- `channel_assign`：新建 child，沿用 root ID；父 owner 不变，父任务依赖 child。
- `channel_transfer`：修改原 task 的 owner、instruction、revision；旧 owner 停止，新 owner 接手。
- `channel_result`：向共享聊天写结果。该调用不会单独结束 provider turn；完成事件再结算 assignment/task。
- `channel_history`：拉取历史，不唤醒成员。
- `channel_reply`：短问题产生受限制的回复任务；它不是自由的 Agent 群聊。

证据：[工具契约](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-tools.ts#L13-L64)、[身份及任务绑定](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L3296-L3310)、[校验与任务更新](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L3490-L3627)。

## 3. 通信路径：task → assignment → mailbox → provider turn → 共享消息

`ChannelService` 从 queued task 建立一次具体执行的 assignment，记录 taskRevision、agentId、resources、awaitedTaskIds。随后用 `channel-assignment:<assignmentId>` 作为幂等键写 mailbox delivery；delivery 带 channelId。真正启动 provider 的权威仍是 Agent drain scheduler。

每个 channel-agent 对有独立 execution thread，普通 Agent 私聊 thread 不被替换。Channel 使用独立 mailbox lane。一个 Agent 在所有 channel 中同时最多跑一个 channel turn；是否还能和自己的私聊并行，取决于 provider 的 concurrentTurns 能力。不能笼统称“每个 Agent 全局永远串行”。

证据：[assignment 构建](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L1872-L1889)、[delivery 入队](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L1911-L1960)、[独立 thread/lane 与并行条件](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/docs/architecture/channels-and-messaging.md#L379-L391)。

## 4. 等待与汇总：父 Agent 先结束 turn，宿主稍后重新唤醒

父任务在本 turn 新增 child 后，正常 turn 完成事件触发如下状态转换：

```text
父 task running
  ├─ 新 child 尚未全部 settled → waiting
  ├─ 新 child 已全部 settled   → queued，revision + 1
  └─ 本 turn 没有新增 child    → completed

最后一个 child settled
  → 检查 parent.state == waiting
  → 检查 parent 的每个 dependency 都 settled
  → parent queued，revision + 1
  → scheduler 再起 owner turn，读取结果并由 LLM 汇总
```

因此没有一个父模型调用长期阻塞等待 Promise，也没有自动向所有成员广播每条消息。汇总内容来自共享 channel history；宿主负责把依赖状态和相关消息带回 owner，真正整合答案由 owner 模型完成。

**失败语义需准确**：普通派发子任务只有 completed 才 settled；failed 不自动放行父任务。短 `channel_reply` 任务 failed 算 settled，避免一个简短问题永远卡住调用者。中断通常变 paused。复刻时应明确失败处理策略，不能把所有结束状态都当成功。

证据：[turn 完成结算](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L2869-L2945)、[settled 与 releasedParent](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L4095-L4122)、[依赖概览及结果位置](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-history.ts#L575-L586)。

## 5. 共享历史不等于共享整个上下文

ChannelStore 把 channels、messages、tasks、assignments、contexts 和 summaries 存进 SQLite。messages 有 sequence；每个 channel-agent context 有 threadId/sessionId、throughSequence、summaryVersion。上下文包含本次 assignment、channel 信息、依赖状态、引用消息、近期聊天及旧消息摘要；其摘要、近期记录与按需 history 接口共同控制体积。新的 session 或 compact 后 cursor 清零，重新发完整包；已有 session 发增量，并避开模型已见过的自身回复。

工具规则明确“普通回复不启动工作”。成员思考不会作为共享协作上下文传给别人；任务来源及结果是共享证据，不是共享推理链。

证据：[SQLite schema](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-schema.ts#L5-L52)、[context 与 assignment 数据结构](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-store.ts#L65-L103)、[context packet 构建](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-history.ts#L650-L727)、[过滤思考与完成回复优先](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/docs/architecture/channels-and-messaging.md#L412-L421)。

## 6. 防循环、取消、恢复是可复刻的关键

当前实现每个 channel 最多 2 个 active assignment；每个 root 的自动派发/转交计数上限为 8。超限暂停整个任务树，需要用户继续或重派。task dependencies 拒绝循环。相同 browser 或有包含关系的 workspace 路径串行，资源从 assignment 的启动快照读取，避免转交途中提前释放旧 owner 的资源。

task.revision 与 assignment.taskRevision 防止旧执行完成新任务；工具 operation ID、assignment 入队键和 result ID 分别抵御重试重复。停止会暂停所选 task 与后代并中断执行；恢复会重新排队；取消停止/失败任务会取消其整个 root run，避免父 task 永远等待已取消 child。记录保留。

恢复启动时重新核对 mailbox delivery：确认完成的调用可结算；确实仍运行的保留；结果未知则暂停并要求检查，不盲目重跑。**这些机制不保证外部文件、网络请求等副作用 exactly-once。**

证据：[限制常量](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/packages/contracts/src/ipc-chat-channels.ts#L21-L28)、[单 owner 和并发检查](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L1236-L1259)、[资源冲突及排队](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L1851-L1869)、[stop/resume/cancel](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L1095-L1140)、[恢复](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L2332-L2399)、[副作用保证范围](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/docs/architecture/channels-and-messaging.md#L509-L523)。

## 7. 与直接 send_message 的边界

Channel 执行中，OpenBotToolRouter **硬性拒绝** `send_message`，要求用 `channel_assign` 或 `channel_transfer`；这不只是 prompt 建议。直接 Agent 私聊可用 `send_message`，其回信进入 mailbox 语义；channel 则另有父子任务、owner、依赖、revision、共享 transcript。不能用“私聊发消息”解释 channel 的结构化 join。

从普通私聊给 channel 发工作，可用 `send_to_channel`；进入 channel 后不允许再这样绕行。`channel_reply` 也不是普通发消息：它创建回复子任务且要求调用者结束 turn，等待该成员回答；回复 turn 禁止再次 reply/assign/transfer，限制连锁对话。

证据：[硬性工具边界](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/agent/openbot-tool-router.ts#L1645-L1697)、[reply 子任务与限制](https://github.com/nightly-labs/openbot/blob/7dae493c89a6c8d5c42a5669d67fc872bbfc0b67/src/backend/channel-service.ts#L3368-L3468)。

## 8. 最小复刻建议（本调研推导，不是原仓库接口）

保留 1 个固定主 Agent、2 个 Worker、一个本地协调器、SQLite 持久任务/消息、每 Agent 独立 session。先不做智能路由、多个 root、频道配置 UI、工具搜索、摘要模型或跨设备。

最小记录：`Agent(id, role, sessionId)`；`Task(id, parentId, rootId, ownerId, instruction, expectedResult, state, revision)`；`Dependency(parentId, childId)`；`Attempt(id, taskId, taskRevision, deliveryId, state)`；`Message(id, taskId, authorId, seq, text)`；`Receipt(operationId)`。

最小工具：`delegate(agentId, instruction, expectedResult)`、`report_result(text)`；如需查询再加 `read_task(taskId)`。协调器必须提供 turn-ended 事件入口；仅有这几个工具、没有事件结算，就无法可靠唤醒父任务。

必须保留的验收不变量：

1. 相同 delegate call 重试只产生一个 child。
2. child 未完成时父 task waiting，不忙等、不提前成功。
3. 最后一个 child 完成后只唤醒一次 owner 汇总。
4. task revision 改变后，旧 turn 不能把新任务标成完成。
5. 取消整个 run 后迟到结果不能重启它；未知恢复结果不自动重复外部动作。

若共享代码目录，第一版可串行执行写入任务；要并行写则声明互不重叠的路径，或给 Worker 独立 worktree。后者是复刻建议，不代表 OpenBot 自带自动 worktree 隔离。
