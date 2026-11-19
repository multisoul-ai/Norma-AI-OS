# 幕僚按「类型 + 模型」派发，并对提议去重

## 背景

幕僚派发 Worker 时必须指定具名智能体（`StaffOp::Assign.agent`），宿主在 `reserve_staff_worker`
里按 `entry.name == proposal.agent && entry.enabled` 查 roster。具名智能体是会变的运行时状态
（在线、启用），却被写进了幕僚的决策和协议里：Chloe 下线后只能「派给 Alan」。

另外 `Propose` / `Assign` 每次都 `proposals.push` 新记录，没有去重，同一件事可能出现多张待采用卡片。
（重复来源由代码推断，实施前须用真实任务数据确认。）

## 目标

1. 幕僚派发只指定 **engine（类型）** 与可选 **model**，由宿主新开卡片。
2. 同一任务里同一件事只保留一张待采用的提议卡。

## 非目标

- 不改自主权设置（`assign_followups`）与风险关键词策略（`crates/brain/src/staff/policy.rs`）。
- 不改续派语义。
- 不处理「任务还没送达」（Worker 启动后 prompt 未确认送达，属 `crates/brain/src/staff/delivery.rs` 投递链路，单独立项）。

## 决策记录

| 问题 | 决定 |
| --- | --- |
| 派发粒度 | 类型 + 模型，不再指定具名智能体 |
| 续派 | 仍按 `dispatch_id`，engine / model 沿用原 Worker |
| 去重 | 两层：提示词规则（主）+ 入库最小守卫（兜底）。同任务内，同 engine 且标题归一化后相同的未处理提议只保留最新一条 |
| 可选集合 | 宿主提供已安装且启用的类型及模型清单，写入幕僚 prompt / CLI 帮助；清单外的值直接报错并列出可选项 |
| 未指定 / 不可用 | 未指定 model 用该 CLI 默认模型；engine 不可用则报错回给幕僚改选，不降级 |
| 卡片展示 | 「类型 · 模型」（如 `Claude · Opus`），派发卡不再显示具名智能体 |
| 相似判定 | 同任务 + 同 engine + 标题归一化后相同 |
| 旧 `agent` 参数 | 快照读取兼容；CLI 新命令用 engine/model，旧参数报错并提示 |

## 设计

### 协议（`crates/norma/src/protocol/staff.rs`）

- `StaffOp::Propose`、`StaffOp::Assign`：`agent` 改为 `engine`，新增 `model: Option<String>`。
- `StaffProposal`：新增 `engine`，沿用已有 `model`；旧快照的 `agent` 在反序列化时映射为 `engine`。
- 续派：engine / model 沿用原 Worker，删除 `staff.worker_agent_mismatch`；`staff.worker_unknown`、`staff.worker_busy`、`staff.worker_pending` 保留。

### 宿主（`crates/app/src/app/brain_host/staff/dispatch.rs`）

- `reserve_staff_worker` 按 engine 在已启用类型中选择，卡片名由宿主自动生成。
- 有 `model` 调用 `set_staff_model`；没有则用 CLI 默认。
- engine 未安装或被禁用返回 `staff.engine_unavailable` 并附可选清单，不降级。
- 可选清单写入幕僚 prompt（`crates/brain/src/lane/staff_prompt.rs`）与内置 `norma-brain` skill。

### 展示与 i18n

- 派发卡、提议卡显示「类型 · 模型」。
- 新增键同步 `crates/ui/locales/` 中英文；运行 `python3 scripts/check-i18n.py`、`python3 scripts/test-i18n.py`。

### 去重（提示词 + 最小守卫）

第一层，提示词（`crates/brain/src/lane/staff.rs` 幕僚规则，与 `norma-brain` skill 同步）：
- 新增一条规则：提议或派发前先看本任务已有的待采用提议；同一件事不要重复 `task propose` / `task assign`，要修改就说明替换哪一条。

第二层，代码兜底（`crates/brain/src/staff/assign.rs`、`crates/brain/src/staff/workflow.rs`），提示词失效时保证不出现多张卡：
- key = (task, engine, 标题归一化)。归一化复用 `policy.rs` 的 `normalize`：全角转半角、去零宽字符、小写、压缩空白。
- 新提议入库时，把同 key、未采用、未忽略、未派发的旧提议标记 `superseded`。`Propose` 与 `Assign` 降级后的提议走同一条路径。
- 已派发或已采用的提议不动。

## 测试要求（先写失败测试，遵循 `docs/quality/test-requirements.md`）

`brain`：
- 同 key 连提三次，只剩最后一条待采用，前两条为 `superseded`。
- 标题仅大小写、空白、全角差异，视为同一条。
- engine 不同或标题不同，互不覆盖。
- 已派发的旧提议不被标记。
- 旧快照仅含 `agent` 字段，可反序列化为 `engine`。

提示词：`crates/brain/src/lane/staff.rs` 的现有文本断言新增一条，确认含"不要重复 propose/assign"规则。

`app`：
- engine 不可用返回 `staff.engine_unavailable`，并附可选清单。
- 有 model 时设置到卡片；无 model 时不设置。
- 续派沿用原 Worker 的 engine / model。

真实 App：`scripts/desktop-e2e.sh` 跑幕僚模块，确认派发卡显示「类型 · 模型」、重复提议只剩一张；新场景登记看图版。

## 风险

- 协议字段变更影响 iOS 端与 `crates/norma/src/mobile/` 对 `StaffProposal` 的读取，需核对。
- 重复卡片的真实根因待复现：实施第一步先用用户那次任务的数据确认来源。若来自幕僚重复调用，提示词加守卫即可；若来自宿主重复写入或重试，须先修宿主，提示词对此无效。
- 守卫不处理旧快照里已存在的重复卡片。

## 实施流程

按项目默认：写实施计划 → 创建 worktree → TDD → subagent 逐 Task 执行 → 独立 subagent 对抗式 review → PR。
