# GPT-Live 自然语言硬编码清理

工作区：`.worktrees/gpt-live-duplex`，分支 `feat/gpt-live-duplex`。本记录接续口语创建修复；此前的词表扩充方案已由本轮完整移除取代。

## 执行方式

GPT-Live 的原始转写与对话背景交给模型理解。模型根据 Norma 系统提示词和工具定义选择是否调用工具，本地执行已选定且参数合法的工具；不再按用户原话的关键词、问号、句首动词或固定句式做第二次意图判断。

本地仍核对项目与来源、通话代次、用户输入快照和修订、取消令牌、工具参数、目标状态、操作身份与回执。模型未调用工具时，本地不会根据姓名、别名或指令模板自行派工。

## 审计与移除范围

| 原规则 | 现行处理 | 代码位置 |
| --- | --- | --- |
| 含“如何／如果／不要／吗／呢”或问号便拒绝；句首动词白名单；记忆写入专用词表 | GPT 专用授权入口和调用已删除，口语补丁模块一并删除 | `agent/src/adapter.rs`、`agent/src/realtime_intent.rs`；原 `realtime_intent/creation.rs` 已移除 |
| 模型前按姓名、别名、“新任务／继续／刚才”直接派工、回复或拒绝 | GPT 来源在直接寻址入口返回 `None`，始终交给模型 | `agent/src/direct_address_turn.rs` |
| 删除工具再次从原话中解析删除动词和编号、位置、名字 | GPT 跳过该原话解析；模型参数仍必须是唯一合法目标，宿主核对真实目标 | `agent/src/adapter.rs`；原参数解析器保持 |
| “停一下／暂停播报／继续播报”等转写直接改变本地播放 | 自动检测器和转写调用已删除；保留界面显式播放控制 | `app/src/app/gpt_live/session.rs`、`output.rs` |

## 保留的约束

以下检查依据结构和状态，不对自然语言作意图分类：

- 工具名称、支持的内置 Agent 类型、必填参数、单一删除目标和目标存在性。
- 项目与来源匹配、通话 generation、输入完整性、快照 revision 和取消令牌。
- 工具预算、稳定操作 ID、回执去重、未知结果停止与账本持久化。
- 记忆的启用开关、scope、版本与先读后写约束。
- 忙碌卡片的关闭状态确认、未跟踪 Agent 的派工限制。
- API 协议字段、官方地址与凭据归属、音频格式及缓冲限制。

模型系统提示词、Norma 人格和工具描述保持原样。手机公共 `realtime_command_authorized` 与其他来源的直接寻址、删除规则仍保留，GPT 不再调用这些语言规则。豆包和原语音流程未改动。

## 验收映射

| 验收项 | 测试证据 |
| --- | --- |
| 已选定工具不被原话拦截 | `gpt_live_model_tools`：14 种工具调用，覆盖问句、否定、引用、非标准动词、记忆及三种删除目标 |
| 参数和取消保护仍有效 | 同目标中的非法 profile、缺目标、双目标、负数目标和已取消请求均无副作用 |
| 原来源不被放宽 | Voice、Other、MobileExecution 的删除规则保持；Doubao 原豁免保持；旧公共严格入口测试保持 |
| 姓名和别名不强制执行 | `delegated_direct_address`：真实 HTTP 模型收到完整原话；模型无工具时零宿主副作用 |
| 模型工具可执行且可取消 | 同目标验证模型选择 reply／assign、原始上下文、稳定 step ID、unknown 停止与等待期间取消 |
| 任务文字不再触发门禁 | `gpt_live_tool_schema`：任务正文含“如何／别执行／别创建”等仍执行模型指定工具 |
| 转写不触发本地静音 | App 真实回环 WS → `run_audio_loop` → 可观测 sink：四条固定话术原样转发，零 `OutputBlocked`，PCM 正常播放 |
| 显式播放控制保留 | 原真实音频循环测试仍验证 UI 停播期间继续上传麦克风，恢复后不补播旧音频 |

诊断证据：`/Users/openclawd/.codex/artifacts/gpt-live-20261018/diagnosis/language-gate-removal-20261019/`。`model-tools-red.log` 先复现原词表拒绝；`model-routing-red.log` 回放移除前直接寻址分支，证明模型被跳过；`audio-keyword-red.log` 使用原生产检测器证明固定话术会自行改变播放。`before/` 保存原源码。

独立只读审查已覆盖全仓生产调用点和以上保留边界，未发现 GPT 路径残留的原话关键词执行门禁。完整 Agent 回归 `agent-final.log`：116 项通过、0 失败、2 项跳过，含 doc-tests。原 GPT 词表拒绝测试按新需求替换为模型选择工具、无工具不执行与保留执行保护的行为测试；其他来源严格入口和原工具定义回归保持。

App 真实新构建的原生单元测试目标：GPT-Live 63 项通过；豆包 71 项通过、2 项跳过；其余语音 161 项通过、2 项跳过。三组按模块过滤并排除重复，零失败。新真实 WS 音频测试与原显式停播／恢复测试均通过。合计相关测试 411 项通过、6 项跳过。

`cargo build -p app --release --bins` 和 `cargo check -p app --release` 通过；Rust 格式、500 行、禁止 allow、diff 空白检查通过。177 个受保护文件 SHA-256 不变；唯一预期差异为 `adapter.rs` 中移除 GPT 语言门禁并跳过 GPT 原话删除解析，公共工具参数定义保持。

初次构建同时链接主程序和大型 App 测试目标时遇到本机内存交换压力，曾串行化自己的两个链接任务；随后停止无关真实云端集成测试目标的额外构建。保留 `app-all-test-build-superseded.log` 作为中断记录。最终使用上述正常运行包构建，以及 Cargo `--lib --no-run --message-format=json` 确认的最新原生测试目标进行验收；其路径、SHA-256 和分组结果见 `app-test-summary.json`。

测试包 `/Users/openclawd/.codex/artifacts/gpt-live-20261018/Norma GPT-Live Test.app` 已更新并通过本地签名严格验证，保留原 Info.plist、隔离数据和已保存的 Key，未替换正式 `/Applications/Norma OS.app`。旧包备份在 `preserved/before-language-gate-removal-20261018T182214Z/`。

`run-gpt-live.command` 已实测启动新包，Computer Use 确认真实测试 App 窗口打开，未自动开始麦克风通话。最新源码、可执行文件、原生测试目标哈希、实际启动 PID 与结果见 `/Users/openclawd/.codex/artifacts/gpt-live-20261018/language-gate-removal-manifest.json`；上一轮 manifest 已标记后继记录。

本地 HTTP／WS 测试验证客户端行为与工具执行边界，不代表已完成云端模型语义质量、真人麦克风或实际画布操作验收。本轮无需读取或重新传输用户 Key。
