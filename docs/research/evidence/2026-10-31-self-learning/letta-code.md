# Letta Code：纠正、后台反思与 Git 记忆发布

调查日期：2026-10-31。方法：从官方 GitHub API 获取主分支 SHA、tree 和 raw 源码，静态核对实际调用链；未运行第三方 Agent，未测性能或效果。

当前 `letta-ai/letta-code` main 固定提交 `59df5424838419e083de0691fa4d08524f348937`，提交时间 2026-10-31 05:20:53 UTC。

`letta-ai/letta` main 固定提交 `8816d3a8516893bfdc0fa14bfb1235226012e295` 的 [README](https://github.com/letta-ai/letta/blob/8816d3a8516893bfdc0fa14bfb1235226012e295/README.md#L5) 已将活跃实现指向 Letta Code，旧 V1 API server 位于 archive 分支。因此旧的“主 Agent + sleep-time Agent 共享 memory blocks”架构不能直接当成本次当前实现。

## 实际实现：本地 CLI 反思路径

1. 回合结束追加转录后，`maybeLaunchPostTurnReflection` 根据 off / compaction-event / step-count 触发。没有 agent ID 或未启用 MemFS 时跳过；step-count 使用距上次成功反思的步骤数。这是调度阈值，不是用户偏好的证据次数。[回合后触发](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/post-turn-reflection.ts#L18-L78)
2. `launchReflectionSubagent` 检查 MemFS、自动反思策略、重试退避、云端是否接管、是否已有运行、父记忆是否脏，以及是否有新转录。父目录有未提交改动时不启动。云端 cutover 会跳过这条本地路径，不能据此描述未审阅的完整云端实现。[启动检查](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/reflection-launcher.ts#L785-L857)
3. `createReflectionMemoryWorktree` 固定父记忆 HEAD，创建独立分支和 worktree。不是直接在主记忆上让两个 Agent 同时改文件。[创建暂存工作树](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/memory-worktree.ts#L125-L156)
4. `prepareReflectionMemoryWorktreeLaunch` 构造反思提示和父记忆快照，然后在指定 memoryScope 中运行反思 subagent。[启动准备](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/reflection-launcher.ts#L526-L559)
5. `finalizeReflectionMemoryWorktreeLaunch` 区分 auto 和 explicit；默认选项回退为 auto。explicit 调用主 Agent 的独立后台 integration conversation，由 Agent 审阅和整合，并非向人类等待审批。[默认合并方式](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/reflection-launcher.ts#L57-L62)；[显式整合调度](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/reflection-launcher.ts#L608-L669)；[整合提示](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/reflection-integration.ts#L17-L35)
6. finalize 检查提交、脏状态、父仓库和合并。发生 Git 合并冲突时 abort、清理工作树，保留转录供重试。只有 merged / no_changes 消费转录；只有 merged 要求重编译上下文。[成功条件](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/memory-worktree.ts#L235-L245)；[父目录脏状态](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/memory-worktree.ts#L478-L501)；[合并与冲突](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/memory-worktree.ts#L597-L629)

简化调用链：回合后触发 → 新转录 → 固定父记忆 HEAD → 反思 worktree → 提交记忆/Skill 改动 → 自动合并或主 Agent 审阅 → Git 合并核验 → 消费转录并要求重编译。

## 提示词中的学习规则，不等于确定性验证器

MemFS V2 的反思提示优先审阅错误与用户纠正，其次是偏好/模式、耐久事实、矛盾和可复用过程。多会话输入要求关注跨会话模式，以最新证据处理矛盾；不保存一次性状态，无有效经验允许 no_change。修改前先读现有文件，Skill 每次至多改一项。[反思学习规则](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/subagents/builtin/reflection-v2.md#L15-L43)

这些是给模型的判断原则，不是代码级的“新证据永远正确”或“自动识别拒绝 A 后的 B”。所查路径主要提供真实转录和模型归纳，没有发现专门的 proposal rejection → replacement 结构化关系协议，也没有固定三次纠正才生效的门槛。

反思提交消息要求记录 reviewed transcripts、变化和原因；提交失败不能当成已保存。这能追溯运行与改动，但不是逐条用户偏好的完整因果证明。[提交和输出约定](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/subagents/builtin/reflection-v2.md#L45-L88)

## 记忆怎样进入下一次行为

MemFS V2 根目录 Markdown 是核心上下文；根 MEMORY.md 为索引，子目录包含自身 MEMORY.md 才作为子记忆目录。Skills 是外部程序性记忆，和核心记忆分开。文件改变后需 commit，之后的 recompile / 新会话 / 提交版本变更才影响编译上下文，不会倒改正在进行的一轮提示。[目录与系统上下文](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/prompts/letta_root_memfs.md#L16-L41)；[提交与生效语义](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/prompts/letta_root_memfs.md#L55-L58)

这是当前源码中的 MemFS V2 路径。仓库仍含其他 memory 格式与远程路径，本次不把它们混画成一个默认部署。

## 对 Norma 的含义（调研推断）

- 借鉴“快照 → 独立暂存 → 合并核验 → 上下文更新”的发布结构，而不必引入 Git 作为 Norma 存储实现。
- 发布检查证明版本和文件一致性；语义归纳仍可能错。需要另外检查后续建议是否符合用户纠正。
- “Agent 审阅”与“人类采纳”必须在产品中明确区分。不能因为配置名叫 explicit 就标成用户批准。
- 所查模块未证明删除记忆会抑制旧转录、Git 历史和所有派生内容再提取。Git 能恢复历史，也意味着普通删文件不等于完整遗忘。
- 该方案不提供 Norma 的项目隔离或提议派发确认规则；这些仍属于 Norma 宿主边界。

## 图解节点

架构图：主 Agent / 转录增量 / 反思调度 / 独立 memory worktree / 反思 subagent / 可选主 Agent 审阅 / Git 合并检查 / MemFS 主版本 / 上下文重编译。

流程图的边界标注：新事实是否耐久、如何解读纠正属于模型判断；脏状态、合并成功、转录是否消费属于代码检查；explicit 审阅不是人工批准。
