# GPT-Live 创建 Agent 工具协议修复

工作区：`.worktrees/gpt-live-duplex`，分支 `feat/gpt-live-duplex`。

## 原因与边界

2026-10-18 23:51:28、23:51:51、23:53:09（北京时间），OpenAI 以 HTTP 400 `invalid_function_parameters` 拒绝 `delete_canvas_card` 的根级 `oneOf`。请求包含完整工具定义，因此查询卡片、创建 Codex/Claude 都在工具选择前失败。

修复仅在 `GptLiveVoice` 来源注册 GPT 专用删除工具适配器：发往模型的参数移除根级 `oneOf`，其余属性、描述及执行入口保持原样。执行继续委托原工具，本地单目标、原文授权、取消、去重和回执校验不变。公共工具注册表及删除执行器未修改；豆包、普通语音和文字来源继续使用原定义。

GPT 专属错误分类将稳定错误码 `invalid_function_parameters` 解释为应用工具定义兼容性错误。待核对副作用仍优先返回，不能误报“未执行”。

## 验证

| 验收项 | 证据与结果 |
| --- | --- |
| 原 HTTP 400 可复现 | 新增真实 HTTP 测试先失败于 `delete_canvas_card` 的 Schema，再经适配通过 |
| 创建两类 Agent | 本地 HTTP 校验整套生产工具定义，Codex/Claude 各创建一次；没有删除或重复创建 |
| 原来源保持一致 | Voice、DoubaoVoice、Other 的实际 HTTP 参数逐项等于原公共注册表，且原 Terminal 工具成功执行 |
| 本地删除门禁保持 | 无目标、双目标均拒绝；明确编号的单目标成功到达宿主 |
| 官方账户 | 使用已授权的已保存 Key、固定官方地址与 `gpt-5.4-mini`，实际收到两项创建调用；仅隔离宿主接收，没有操作用户画布或采集麦克风 |
| Agent 完整回归 | 117 通过、0 失败、2 跳过；其中新增官方账户测试默认显式跳过 |
| App GPT-Live | 63 通过、0 失败 |
| App 豆包 | 71 通过、0 失败、2 项既有跳过 |
| App 其余语音 | 161 通过、0 失败、2 项既有跳过；过滤已单独验收的 GPT/豆包 |
| 保护边界 | 178 个受保护源码 SHA-256 与本轮开始一致，包括豆包、voice、dialogue、ASR、voice_input、语音设置和公共删除校验 |
| 独立审查 | 来源隔离、执行授权、错误分类和副作用不确定性均通过只读审查 |

错误提示测试同样先在真实 App 测试目标上失败于误报“模型权限或网络”，再修复并通过。附加删除测试首次使用了不表示稳定编号的“卡片2”，已改用原执行器支持的“卡片编号2”；生产授权逻辑没有为测试放宽。原失败日志保留。

日志与哈希根目录：`/Users/user/.codex/artifacts/gpt-live-20261018/diagnosis/tool-schema-20261018/`。

## 验收边界

官方验证证明真实 API 接受生产工具定义并能下发两类创建命令；本地集成验证工具与宿主交互。真人通过 DJI 麦克风在真实画布连续创建卡片、插话及声学效果，仍留给用户统一验收。

## 测试包交付

`cargo build -p app --release --bins --tests`、`cargo check -p app --release` 均通过。App 回归直接执行本次新构建的原生测试目标，并使用独立测试目录。Rust 格式、500 行、禁止 allow、差异空白检查通过。

已更新 `/Users/user/.codex/artifacts/gpt-live-20261018/Norma GPT-Live Test.app` 及兼容的 `bin/norma-os`。新包通过本地签名严格校验；保留原 Info.plist、隔离数据目录和已保存的 Key，未替换正式安装的 App。原包和旧二进制保存在本轮 `preserved/before-tool-schema-*` 目录。

`run-gpt-live.command` 已实测启动新包，并通过 Computer Use 确认测试 App 窗口打开。最新构建哈希、测试结果、备份位置及日志索引见 `/Users/user/.codex/artifacts/gpt-live-20261018/tool-schema-fix-manifest.json`。没有自动开始麦克风通话。

后续同路径测试包已继续修复口语创建被本地门禁误拒的问题，当前交付见 [GPT-Live 口语创建授权修复](2026-10-19-gpt-live-spoken-creation-fix.md) 及其 `spoken-creation-fix-manifest.json`；本文保留工具协议修复当时的验证记录。
