# 07 端到端验收 SPEC

- 日期：2026-07-06
- 状态：草稿
- 上游：`01-always-listening-shell.md` 到 `06-task-result-callback.md`

## 1. 目标

定义 MVP 的最终端到端验收标准。通过本 SPEC 后，才能认为“边聊天，边异步操作画布卡片，并自然汇报结果”的最小闭环成立。

## 2. 核心验收场景

用户打开 `Always Listening` 后，可以和 AI 实时语音聊天。聊天过程中，AI 会派发卡片操作任务。任务在后台执行，完成后 AI 自然汇报结果。

核心手动验收句子：

> “创建一个 Codex 卡片，让它分析 README，然后发送。”

预期结果：

```text
1. 用户能在 Always Listening 中直接说出这句话。
2. AI 可以先自然回应，例如：“我来处理。”
3. 聊天主线不中断，用户仍可以继续说话。
4. 后台创建一个 Codex 卡片。
5. 系统向该 Codex 卡片输入“请分析 README”并发送。
6. 任务成功后，AI 在当前对话中自然汇报：
   “我已经创建了 Codex 卡片，并把 README 分析请求发过去了。”
```

## 3. 最终验收清单

### 3.1 配置与生命周期

- Settings -> Voice 有 Realtime conversation 区域。
- 可以打开和关闭 `Enable Always Listening`。
- 配置缺失时不启动 session，并显示错误状态。
- app close / project close 会停止 session、PCM stream 和 sidecar。
- 一个 app 进程最多一个 active realtime session。

### 3.2 实时语音聊天

- 用户语音能进入 OpenAI Realtime。
- AI 能用语音回复。
- AI 语音首响目标约为 `1-2 秒`。
- 用户可以打断 assistant 回复。
- 打断语音回复不会取消 active card task。

### 3.3 卡片任务

- `enqueue_card_task(intent)` 能创建 task。
- task 串行执行。
- 支持 `Create`、`List`、`Focus`、`Delete`。
- 支持 `SendText`。
- 支持 create -> send previous-step。
- task 可取消，取消不回滚已完成 step。

### 3.4 结果回报

- 成功结果能回灌 Realtime。
- 失败结果能回灌 Realtime。
- 取消结果能回灌 Realtime。
- 用户或 assistant 正在说话时，结果进入 callback queue。
- AI 用自然语言汇报结果，不弹额外 modal。

### 3.5 安全边界

- Realtime 模型不能直接修改画布。
- Node sidecar 不能直接修改画布。
- Card Planner 输出必须经过 Rust `compile_voice_plan`。
- Executor 只能执行白名单 `CardCommand`。
- 不执行模型生成代码。
- OpenAI API Key 不写日志。

## 4. 必跑验证命令

实现完成后至少运行：

```bash
cargo test -p voice
cargo test -p voice_sherpa
cargo test -p card_task
cargo test -p realtime
cargo test -p app
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

如果某个 crate 尚未创建，对应命令在该任务完成前不作为阻塞项；创建后必须加入验证。

## 5. 手动验收脚本

### 脚本 A：纯聊天

```text
1. 打开 Always Listening。
2. 说：“我们聊一下今天要做什么。”
3. 预期：AI 约 1-2 秒首响，并继续语音对话。
4. 在 AI 回复中途说：“等一下，我补充一下。”
5. 预期：assistant playback 被打断，session 继续 listening。
```

### 脚本 B：创建卡片

```text
1. 打开 Always Listening。
2. 说：“创建一个 Codex 卡片。”
3. 预期：画布新增 Codex 卡片。
4. 预期：AI 可以继续聊天，不因任务执行阻塞。
```

### 脚本 C：创建并发送

```text
1. 打开 Always Listening。
2. 说：“创建一个 Codex 卡片，让它分析 README，然后发送。”
3. 预期：画布新增 Codex 卡片。
4. 预期：该卡片收到分析 README 的请求并发送。
5. 预期：任务完成后 AI 自然汇报结果。
```

### 脚本 D：取消任务

```text
1. 打开 Always Listening。
2. 说：“创建一个 Codex 卡片，让它分析 README，然后发送。”
3. 在任务执行中说：“取消刚才那个操作。”
4. 预期：queued task 直接取消；active task 在 step 边界取消。
5. 预期：已经完成的 step 不回滚。
6. 预期：AI 自然说明取消结果。
```

### 脚本 E：失败回报

```text
1. 打开 Always Listening。
2. 在没有目标卡片时说：“把 hello 发送到那个卡片。”
3. 预期：task failed。
4. 预期：AI 自然说明没有找到目标卡片或目标不可输入。
5. 预期：app 不崩溃，不弹额外 modal。
```

## 6. 不通过条件

出现以下任一情况，端到端验收不通过：

- Always Listening 打开后无法关闭或关闭后 sidecar 残留。
- 用户正常聊天时 AI 长时间无首响，超过 MVP 可接受范围。
- 卡片任务执行期间聊天主线被阻塞。
- Realtime 模型或 sidecar 能绕过 Rust executor 直接修改画布。
- task 成功但不回灌结果。
- task 失败导致 app 崩溃。
- OpenAI API Key 出现在日志里。
