# GPT-Live 口语创建授权修复

> 历史记录：本轮词表扩充已由后续 [GPT-Live 自然语言硬编码清理](2026-10-19-gpt-live-language-gate-removal.md) 取代。当前源码已从 GPT 路径移除这层语言门禁，本文保留当时的诊断和测试包证据。

工作区：`.worktrees/gpt-live-duplex`，分支 `feat/gpt-live-duplex`。

## 已复现的原因

北京时间 2026-10-19 00:45:29，官方模型已下发 `create_agent`，分别请求 `codex` 与 `claude`。本地门禁却将两项请求均拒绝为“本轮用户原话没有明确授权执行此操作”。此次失败发生在本地授权阶段，未到卡片宿主。

用户原话是“喂喂创建两个智能体卡片嗯,你就帮我创建一下Codex跟- 那个 Claude Code这两个智能体卡片吧”。原门禁只接受有限的句首祈使词，不能越过“喂喂”“你就帮我”等口语前缀。

## 修复边界

GPT-Live 的 `CreateAgent` 授权判断识别问候、填充词和明确请求前缀，使用原始用户原话判定。引号内的文字不充当撤回指令；带口语前缀的更正、撤回仍撤销先前授权。仅描述“创建卡片的操作很标准”等评价不能授权创建。

原 `current_request` 严格扫描路径保留。GPT 适配器仅在 `GptLiveVoice` 分支调用新增的 crate 私有授权入口，且只有 `CreateAgent` 使用口语规则。公共 `realtime_command_authorized` 入口保留原行为，供手机实时会话等原调用方使用。不修改豆包、原语音指令、公共工具定义、删除执行器、操作取消、去重或持久化协议。

撤回判断保留原文中的对象，只有无对象短句或明确针对卡片、智能体、创建操作的否定才撤销创建；“只读审查，别执行代码／别创建文件／别新建分支／别执行‘rm -rf’”等任务要求不能被当成撤回。

## 验收记录

诊断根目录：`/Users/user/.codex/artifacts/gpt-live-20261018/diagnosis/spoken-agent-creation-20261019/`。

| 验收项 | 对应证据 |
| --- | --- |
| 真实原话能创建两类卡片 | `red.log` 先复现原拒绝；公开委派入口经真实回环 HTTP、生产工具和授权校验，隔离宿主接收 Codex/Claude 各一次创建 |
| 否定、撤回、引用和评价不能授权 | `gpt_live_spoken_fillers_preserve_creation_authorization_boundaries`；审查反例先在 `review-red.log`、`review-second-red.log` 失败 |
| 任务内部限制不等于撤回创建 | `gpt_live_creation_task_payload_is_not_a_spoken_veto`；覆盖术语引用与“只读审查，别执行代码／别创建文件／别新建分支／别执行‘rm -rf’”；`task-payload-red.log` 和 `quoted-task-probe-red.log` 保留先失败证据 |
| 原来源工具不变 | Voice、DoubaoVoice、Other 的实际 HTTP 工具定义逐项对比公共注册表，原工具往返成功 |
| 删除校验保留 | 同一真实委派路径拒绝无目标、双目标，合法稳定编号到达宿主 |
| 公共授权入口不变 | `strict-authorization-red.log` 先复现隔离遗漏；新增 `non_gpt_authorization_retains_original_strict_creation_requests` 锁住原严格入口 |
| 保护源码边界 | `protected-before.json`、`protected-result.json`；178 项中 177 项 SHA-256 不变，`adapter.rs` 仅将原 GPT 来源分支改调 GPT 私有入口，逐字比对确认仅此一处变化 |

最终聚焦集成：`source-isolation-green.log`，6 项通过、0 失败、1 项官方云端测试跳过。独立审查发现的撤回、评价和任务载荷误拒均已修复，最终全仓核对同时覆盖手机公共入口、GPT 私有入口及适配器逐字差异。审查为只读，未替代构建、真人语音或 UI 验收。

完整 Agent 回归：`agent-release-final.log`，120 项通过、0 失败、2 项跳过，已完成 doc-tests。未复跑旧版 App 测试二进制，以免把旧构建结果记为本轮证据；本轮通过 Agent 公开委派入口验证工具命令和来源隔离，另外重新构建实际 App。

`cargo build -p app --release --bins` 通过；Rust 格式、单文件 500 行、禁止 allow 与 diff 空白检查通过。真实测试包已更新到 `/Users/user/.codex/artifacts/gpt-live-20261018/Norma GPT-Live Test.app`，本地签名严格校验通过。保留原 Info.plist、隔离测试数据和已保存的 Key；正式 `/Applications/Norma OS.app` 的可执行文件 SHA-256 保持不变。

原包备份：`/Users/user/.codex/artifacts/gpt-live-20261018/preserved/before-spoken-creation-20261018T172540Z/`。兼容入口 `bin/norma-os` 同步更新并保留旧版本。`run-gpt-live.command` 已实测启动新包，Computer Use 确认“Norma GPT-Live Test”的真实窗口打开；未自动开始麦克风通话。

最新构建、源码、签名后可执行文件的独立哈希及实际启动 PID 见 `/Users/user/.codex/artifacts/gpt-live-20261018/spoken-creation-fix-manifest.json`。上一轮 `tool-schema-fix-manifest.json` 已标记后继记录，保留原验收证据。

## 验收限制

本地 HTTP 集成测试使用隔离宿主，证明生产授权判断与工具命令，不创建用户画布卡片。此次真实日志已确认模型正确下发工具，因此无需重复调用收费云端接口。真人麦克风连续创建、实际画布结果与声学效果仍需在更新后的测试包复测。
