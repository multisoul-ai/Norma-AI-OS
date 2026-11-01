# 从纠正到长期行为：Mem0 与 LangMem 源码证据

调研日期：2026-10-31。方法：只读官方 GitHub API、固定提交的源码与仓库内示例；没有安装依赖、运行项目或验证 LLM 实际提取质量。源码快照下载到 `/tmp/norma-self-learning-memory-libraries/`，不进入项目业务目录。

| 项目 | 固定提交 | GitHub API 查询时状态 |
|---|---|---|
| `mem0ai/mem0` | `753496cf1435d76c3a711f3af51303f7fdcb6420`，2026-10-30 21:35:34 UTC | main，`archived=false` |
| `langchain-ai/langmem` | `839c3fa0fc2c118734611d5d86503e52fc3a3dd1`，2026-10-30 19:35:50 UTC | main，`archived=false`，本次未见需要按已归档或已迁移项目处理的证据 |

状态来源：GitHub `GET /repos/{owner}/{repo}` 与 `GET /repos/{owner}/{repo}/commits/main`。归档状态是查询时事实，提交链接固定的是代码事实。

标记：**已实现**＝存在实际函数链；**框架能力**＝应用接线后可用；**示例**＝官方用法，不能直接当作运行时保证；**推断/缺失**＝从已核验路径得到的边界，不声称扫描了所有商业服务内部。

## 先给判断

两者都能把交互转换为跨会话记忆，但不能据此说“模型权重自我训练”或“自动理解用户每次拒绝”。Mem0 的当前 Python OSS 主路径是“召回旧记忆 → 单次 LLM 提取 → 只新增”；LangMem 的主路径是“召回旧文档 → Trustcall 提取/补丁 → store.put/delete”，另有独立的提示词优化入口。代码改变的是外部记忆或提示词；这里没有模型训练闭环。[M1][M2][L1][L2][L7]

**版本陷阱：当前 Mem0 不应画成常见的旧版 ADD/UPDATE/DELETE 自动决策图。** `add()` 的 docstring 仍称会决定 add/update/delete，文件里也保留 `DEFAULT_UPDATE_MEMORY_PROMPT`，但当前真正被调用的是 `ADDITIVE_EXTRACTION_PROMPT`，写入和返回事件均为 ADD。应以可执行调用链为准。[M0][M1][M2][M3]

**没有在这两个核心库里找到“UI 拒绝 A + 紧接着输入 B → 确定的长期偏好标签”的专用事件建模。** 把整段对话交给 LLM 后，模型可能提取出纠正；但拒绝的对象、原因、适用范围和 B 是否长期偏好，仍需应用表达或接受模型推断。LangMem 默认甚至明确鼓励归纳和溯因；Mem0 当前默认也会从请求推断隐含偏好，并提取 assistant 推荐。不能把这种猜测当成用户明确确认。[M3][M4][L3][L8]

## Mem0：当前 OSS Python 路径

### 实际函数链与绘图节点

```text
调用方提交 messages + user_id/agent_id/run_id
    ↓ Memory.add()
身份过滤/metadata → infer=False 直接逐条保存原文
    ↓ infer=True（默认）
SQLite 最近 10 条消息 + 同身份向量 top-10 旧记忆
    ↓ 单次 LLM：ADDITIVE_EXTRACTION_PROMPT
提取 text / attributed_to（prompt 还要求 linked_memory_ids）
    ↓ batch embedding + 文本 MD5 去重
新 UUID → vector_store.insert → SQLite ADD 历史
    ↓
实体抽取 → entity store 的实体→memory 关联；保存最近消息

下次请求 → search(filters=身份) → 语义/BM25/实体打分
    ↓
返回 memory 列表 → 调用方负责放入下次 LLM 上下文

显式修正支路：update(memory_id, text) → 原 ID 内容覆盖 + UPDATE 历史
显式删除支路：delete(memory_id) → 向量删除 + DELETE 历史 + 清实体关联
```

函数依据：`add → _add_to_vector_store`，[M1]；提取、去重、写入、返回，[M2][M5]；召回，[M6]；显式更新删除，[M7][M8]。`Memory.chat()` 本身未实现，因此不能把 search 返回等同于已经进入回答上下文。[M9]

### 提取、重复与冲突

1. **已实现：V3 是追加式提取。** 旧记忆以 top-10 检索结果提供给 LLM；历史消息用于代词/上下文；一次 LLM JSON 响应读取 `memory` 数组。之后每条提取生成新 UUID，持久化成功项返回 `event: ADD`，不在此路径调用 `_update_memory` 或 `_delete_memory`。[M1][M2][M5]
2. **已实现 + prompt 约束：两层去重。** prompt 要求语义等价且没有新上下文时跳过；代码再对“本次召回旧记忆的 hash + 当前 batch 的 hash”去重。代码不是对整个用户历史做全量语义唯一性检查；未召回的旧项无法通过这一步 hash 集合被排除。[M3][M2]
3. **已实现边界：纠正并不自动废止旧事实。** prompt 要求把“从 A 改为 B”连同过渡原因和临时性写入新的记忆，还要求在偏好变化/矛盾时用 `linked_memory_ids` 关联旧记忆。实际 ADD payload 构造只从模型结果保留 `text` 与可选 `attributed_to`，没有保存该模型字段；旧向量记录也没有被废止。因此在旧 A 已存在、LLM 确实提取 B 时，A 与 B 可并存。不能把 prompt 所描述的关联当作已实现的冲突状态机。[M4][M2][M5]
4. **已实现边界：实体关联不是纠正链。** entity store 保存的 `linked_memory_ids` 来自实体抽取后当前记忆的 ID 集合，用于搜索加分；与 LLM 输出的“新记忆关联旧记忆”不同，没有 `supersedes`/`invalidated_by` 语义。[M5][M6]
5. **已实现边界：默认召回不会保证新纠正胜过旧事实。** `score_and_rank` 组合 semantic、BM25 和 entity boost，并按分数排序；没有使用 created_at、updated_at、来源可信度或明确纠正优先级。过期项默认过滤，但过期隐藏不等于自动冲突消解。[M6][M10]

### 证据、身份与时间

- **已实现：身份来自调用参数。** add 至少要求 user_id/agent_id/run_id 之一，metadata 中相同身份键会被剥除，防止自由 metadata 改变作用域；search/get_all 同样要求身份过滤。它是数据分区机制，不能替代调用方认证/权限校验。[M11][M12]
- **已实现 + 模型判断：来源归因。** 新 prompt 同时提取用户与 assistant；要求用户事实和 assistant 推荐分开，assistant 生成内容以“用户被推荐了 X”等措辞保留。`attributed_to` 由模型输出后复制进 payload。没有强制保存原始 message ID、原文 span、拒绝对象或来源证据哈希。[M3][M4][M2]
- **已实现边界：SQLite 消息不是完整证据库。** 每个 session_scope 只保留最近 10 条，history 保存 old/new 内容和 ADD/UPDATE/DELETE 事件。没有把每个提取 memory_id 指回对应原始 message_id；不能靠这一层重建所有历史偏好证据。[M13][M14]
- **时间语义有两层。** prompt 要求把相对时间落到 Observation Date，并描述“转变/试用”的时效；但 OSS `add(timestamp=...)` 明确报不支持，构造 additive prompt 时没有传原始 observation timestamp，默认当天 UTC 日期。created_at/updated_at 是写入/更新时间。不要将这些当作完备的双时间事实模型。[M4][M15][M16]
- **已实现：手工 update 能保留同 ID 变更历史。** 更新从旧 payload 出发，剥除身份变更，重新 embedding，更新内容和 updated_at，再写 old/new 的 UPDATE 历史；文本变化会清理并重建实体关联。这是调用方可用的确定修正入口，不是 add 自动触发的行为。[M7]

### 调度、召回注入和 feedback

- **框架能力：`Memory.add()` 同步完成提取/保存；`AsyncMemory.add()` 是 await 的异步 API。** 后者把阻塞存储/embedding 放进 `asyncio.to_thread` 等路径，但仍需调用方何时调用、是否脱离主响应等待、如何重试；不能称为独立后台学习调度器。[M1][M17]
- **框架能力：Python OSS 的 search 返回记忆，并不自动修改主 agent 的 prompt。** `Memory.chat` 未实现。调用方必须显式召回并注入；反例见下一项官方插件，它确实做了接线。[M6][M9]
- **已实现（集成层，不能冒充 Python 核心）：官方 OpenClaw 插件具有自动闭环接线。** `before_prompt_build` 召回后通过 `prependContext` 注入；`agent_end` 在成功、交互、非 subagent、未调用记忆修改工具等条件下提取消息并 fire-and-forget `provider.add`。用户内容短于 50 字符会被跳过，故“短纠正一定会学到”也不成立。这个插件可选择不同 backend，不能把其 backend 的算法一概认作上面的 Python V3。[M18][M19][M20]
- **已实现（云客户端入口）：`MemoryClient.feedback(memory_id, feedback, feedback_reason)`** 校验 POSITIVE/NEGATIVE/VERY_NEGATIVE 后 POST `/v1/feedback/`。这里只能证明客户端提供反馈接口；商业后端如何据此重排、删除或再训练，在这段开源实现中不可见。Python OSS `Memory` 没有相同反馈处理函数，不能声称 OSS 负反馈会自动调权。[M21]
- **缺失：本次核验的 Python OSS 核心没有独立的 prompt optimizer。** custom_instructions/prompt 是调用方提供的提取指令；并不自动由纠正生成、评估、发布下一版行为规则。[M1][M16]

### 删除和恢复边界

**已实现：delete 删除向量项并保留 SQLite 的 old_memory / DELETE 事件，清理实体引用。** delete_all 按身份分批删记忆；这里没有删除相应近期消息或清空旧 history。`reset()` 才会丢弃 history/messages 表并重建存储。[M8][M14][M22]

**推断/缺失：没有在 Memory 公共实现找到 restore/undo API。** history 可以帮助调用方查看旧文本，但这不等于一键还原：恢复还需处理 ID、metadata、embedding、实体引用。删除记忆也不等于擦除其原始消息上下文；若应用再次提交含该事实的对话，仍可能重新提取。调用方需设计忘记标记/禁止重学，不能依赖 delete 本身。[M8][M13][M14]

## LangMem：可组合记忆与独立提示词优化

### 实际函数链与绘图节点

```text
热路径：agent 获得 manage_memory / search_memory tools
    ├─ create/update → namespace + key → BaseStore.put
    ├─ delete → BaseStore.delete
    └─ search → BaseStore.search → tool result → agent 上下文

后台路径（应用显式接线）：
会话消息 → ReflectionExecutor.submit(after_seconds, thread_id)
    ↓ 本地优先队列/线程，或远程 runs.create
MemoryStoreManager.invoke
    ↓ namespace 内检索相关旧文档
MemoryManager → Trustcall create_extractor
    ↓ 新文档 / 同 ID patch / RemoveDoc（按 enable 开关）
可选 phases：整合、去重、丰富
    ↓
有变化的文档 store.put；允许的 RemoveDoc → store.delete

独立优化路径（应用显式接线）：
trajectories + optional feedback + 当前 prompt
    ↓ create_prompt_optimizer(kind=gradient/metaprompt/prompt_memory)
反思 → 是否值得调整 → 新 prompt 字符串
    ↓ 应用负责保存、评估、选版、用于未来回答
```

真实入口与导出：[L1][L2][L4][L5][L7][L8][L18]。这两条学习路径不是默认自动互相串联：记住偏好不自动意味着修改系统提示词，获得新提示词也不自动意味着已经投入使用。

### 写入、冲突和来源

1. **已实现：存储管理器先检索再提取。** 有 query_model 时由模型生成查询，无则用近期对话窗口查询；取 query_limit 个相关文档，以 stable ID 映射传给 MemoryManager；可选 phases 再组织记忆，最后只 put 变化内容并执行删项。[L1]
2. **框架能力：同 ID 更新和 RemoveDoc。** MemoryManager 默认 enable_inserts=True、enable_updates=True、enable_deletes=False；交由 Trustcall extractor 生成/修改结构化 schema。实际补丁算法位于外部依赖 `trustcall`，本调研没有把该依赖内部视作已核验。LangMem 自己将回传 json_doc_id/RemoveDoc 映射到 store 的原 key。[L2][L1]
3. **默认值陷阱：公开 `create_memory_store_manager()` 的 enable_deletes=False。** class 构造器是 True，docstring 又说默认 True；公共 factory 签名和实参传递实际为 False。图上应写“更新默认开启，删除按配置允许”，不能只读类签名或说明文字。[L6][L19]
4. **模型判断而非硬证据：默认指令明确允许 deduction/induction/abduction。** 要求引用支持信息、给不确定结论置信度、按可靠度/新近性巩固，更新过时事实。但默认 Memory schema 只有 content 字符串，没有强制 evidence_id、confidence、explicit_user_correction、scope、valid_from 字段。因此这些要求主要靠模型遵循 prompt；开发者可自定义 schema 补字段。[L3][L9]
5. **框架能力：调用方可用结构化 schema 区分事实/偏好/规则。** 强类型 schema 被传进 Trustcall 工具；框架本身没有一个统一的“所有纠正优先于推断”的确定排序与状态机，也没有观察 UI 拒绝事件的内建 detector。[L2][L3][L8]

### 身份、调度和召回

- **已实现：namespace 隔离。** 默认 `("memories", "{langgraph_user_id}")`，模板从 configurable 取值，缺少配置键会报错。manage/search 工具始终解析 namespace，再调用 BaseStore；namespace 值可信与否、用户鉴权及跨项目权限仍由应用负责。[L6][L10][L4]
- **已实现：后台延迟与去抖机制。** LocalReflectionExecutor 建优先队列和工作线程；submit 支持 after_seconds 和 thread_id，同线程的新待执行任务会取消旧 Future/设置 cancel_event。工作循环等待到期后调用 reflector.invoke。它是本地进程内队列，不是持久化任务日志；不要将“取消尚未执行的任务”画成“撤回已经写入的记忆”。[L5][L17]
- **已实现：远程调度接口。** RemoteReflectionExecutor 通过 LangGraph client `runs.create` 传 after_seconds、multitask_strategy="rollback"。该 rollback 是远程 run 并发策略，不是库提供的长期记忆版本恢复。服务端行为在此未执行验证。[L11]
- **框架能力：工具召回可直接变成工具结果；主动召回仍需注入。** search_memory_tool 返回搜索项 JSON；官方 `my_agent` 示例是先 manager.asearch，将结果拼入 system，然后回答后显式 manager.ainvoke。后一段是示例接线，不是 MemoryStoreManager 自动控制所有主 agent prompt。[L16][L12]

### Feedback 与 prompt optimization

**已实现：公开 `create_prompt_optimizer` 支持 gradient、metaprompt、prompt_memory。** 输入是当前 prompt 和 trajectories；AnnotatedTrajectory 的 feedback 可以是字典、文本或 None。一个官方类型示例包含用户“Stop advertising to me”和 developer_feedback/score，说明它能消费纠正轨迹与显式反馈，但调用方必须把轨迹交给 optimizer。[L7][L8]

**已实现：默认 gradient 返回字符串候选。** 无 trajectories 时返回原 prompt；先反思并得 warrants_adjustment，False 时不改，True 才执行 prompt update 并返回 improved_prompt。这里没有内建独立验证集、效果回归测试、人工发布闸门或自动替换主 agent prompt。所谓 gradient 是自然语言反思/建议，并未在这里对基础模型权重求梯度训练。[L13]

**额外实现边界：仓库还包含 stateful reflection graph。** `update_general` 从 Store 取现有 prompt，用 messages/feedback/instructions 反思，若 update_prompt=True 就直接 `store.aput(...new_prompt, index=False)`；因此不能绝对说整个仓库“绝不保存 prompt”。但它不在上述主要 optimizer 的返回字符串路径里，也未提供本身的版本审计/回滚。[L14]

### 删除和恢复边界

**已实现：工具 delete 是直接 BaseStore.delete；store manager 将 RemoveDoc 映射到原 namespace/key 后删除。** 更新是同 key 的 put。这里没有附带审核/撤销 UI，也没有库级 tombstone 阻止未来重复提取。[L4][L1]

**推断/缺失：不能将 docstring 的“maintains a versioned history of all changes”当成已验证能力。** 本次核验的实际 manager 持久化路径只有 store.put/delete，未见同步写历史版本、event log 或 restore API。若某 BaseStore 后端有版本化或备份，它属于外部实现，需要另查。图中写“存储后端；审计/恢复由应用/后端补齐”。[L15][L1]

## 对“拒绝 A → 输入 B”的具体回答

| 场景 | Mem0 当前 Python OSS | LangMem |
|---|---|---|
| 用户说“不要 A，以后都用 B”并提交整段对话 | 默认模型有机会抽取“从 A 改 B”，追加新项；原 A 不自动失效 | manager 可将相关旧 A 更新成 B；结果依赖召回与模型；默认 factory 不允许删项 |
| UI 拒绝按钮没有自然语言解释 | 核心 add 不知道按钮含义，应用需提交事件/说明 | 同样需应用把事件编码成 trajectory / structured schema / feedback |
| 用户紧接着手工输入 B | 可能被模型推断为偏好；不是确定的因果/持久偏好证据 | 默认反思会推断，但无法保证判断的是任务局部还是长期偏好 |
| B 是临时例外 | prompt 有保留试用/临时信息的指导；无强制作用域字段 | 可自定义 schema/instructions；默认字符串不强制建模 |
| 要求确定地撤回 A | 调用 update/delete，并补充忘记/防重学策略 | 工具按 ID update/delete，或 manager 更新；仍需应用控制证据与防重学 |

该表是从以上函数链与默认 prompt 得到的**边界分析**，不是实际模型成功率测试。[M2][M4][M7][L1][L3][L4][L8]

## Norma 可借鉴的部分（建议，不是两个库已有事实）

1. **把纠正事件放在模型推断之前。** Norma 已知 proposal_id、被拒绝的 A、随后 B、用户身份、项目、会话和时间；应直接保留原始事件与因果候选，不要先把它们压成“用户偏好 B”。区分明确纠正、任务局部选择、模型推断、用户确认。
2. **分开保留原始证据与有效规则。** 证据可追加；当前生效偏好需要 scope、status、supersedes、source_event_ids 与撤销。借鉴 Mem0 追加保存的可追溯思路，同时补上它当前缺少的明确失效关系。
3. **后台做提取、合并、查重。** 借鉴 LangMem submit 延迟/同 thread 去抖；Norma 应用自己的后台执行与持久任务记录，并避免 GPUI 前台阻塞。重复触发需幂等，取消/关项目后防迟到写回。
4. **先召回可验证偏好再行动。** 检索必须带 user/project/agent 作用域；明确纠正优先于助手推测，过期/已撤销项不注入。记录本轮注入了哪些记忆，才可解释为什么这样做。
5. **提示词优化作为独立候选流程。** 可借鉴 LangMem trajectories + feedback → 是否应修改 → candidate prompt，但加入保留集回归、版本、启用与回滚；单次拒绝不能自动改全局行为。

## 固定源码索引

[M0]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/configs/prompts.py#L176-L185
[M1]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L760-L879
[M2]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L918-L1098
[M3]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/configs/prompts.py#L468-L578
[M4]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/configs/prompts.py#L605-L701
[M5]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L1100-L1220
[M6]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L1642-L1745
[M7]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L2052-L2114
[M8]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L2116-L2144
[M9]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L2191-L2192
[M10]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/utils/scoring.py#L60-L139
[M11]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L314-L404
[M12]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L1453-L1476
[M13]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/storage.py#L257-L324
[M14]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/storage.py#L102-L141
[M15]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L817-L829
[M16]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/configs/prompts.py#L1007-L1045
[M17]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L2548-L2618
[M18]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/integrations/openclaw/index.ts#L552-L590
[M19]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/integrations/openclaw/index.ts#L660-L757
[M20]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/integrations/openclaw/index.ts#L877-L923
[M21]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/client/main.py#L1232-L1254
[M22]: https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/storage.py#L326-L339
[L1]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L1139-L1280
[L2]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L217-L339
[L3]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L185-L206
[L4]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/tools.py#L263-L354
[L5]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/reflection.py#L254-L329
[L6]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L1666-L1707
[L7]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/prompts/optimization.py#L249-L269
[L8]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/prompts/types.py#L36-L92
[L9]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L64-L92
[L10]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/utils.py#L58-L95
[L11]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/reflection.py#L161-L198
[L12]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L1767-L1785
[L13]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/prompts/gradient.py#L293-L399
[L14]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/prompts/stateful.py#L16-L54
[L15]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L1682-L1685
[L16]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/tools.py#L433-L486
[L17]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/reflection.py#L401-L471
[L18]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/__init__.py#L1-L27
[L19]: https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L2092-L2105
