# GPT-Live 完整 Norma 系统提示词接入

日期：2026-10-18。范围：`feat/gpt-live-duplex` 工作树中的系统提示词接入；其他 GPT-Live 整改仍按原方案推进。

## 已实现

- 复用 `agent::norma_system_prompt_base`，传入当前项目环境及智能体名册。
- 从现有设置存储读取 Norma 人格，保留用户的交流风格和补充说明。
- 将原有固定记忆规则提取为 `NORMA_MEMORY_RULES`，NormaSession 与 GPT-Live 使用同一原文。
- 为 Live 追加 Client 委派要求；真实项目操作继续由原工具后台执行。
- 明确环境与会话尾部是启动快照，当前状态须委派后台只读核验。

原有记忆规则共 608 UTF-8 字节，提取前后逐字节相同。原后台文件除了引用该常量，没有其他变化；12 个豆包文件与本次实施前的哈希一致。

## 验收映射

| 要求 | 证据 |
| --- | --- |
| 完整系统身份、规则与用户人格进入实际请求 | `live_start_includes_norma_system_prompt_saved_persona_and_project` 检查真实 `session.start.instructions` |
| 保留名册、项目、音色及 Client 委派 | 同一启动请求测试包含正断言，且排除默认人格覆盖 |
| 无项目时可正常聊天；下次连接读取新偏好 | `new_live_call_reloads_persona_without_requiring_a_project` 使用隔离 SQLite 保存两个人格版本 |
| 损坏的人格不可静默丢失 | `invalid_saved_persona_stops_live_config_instead_of_using_defaults` 要求配置加载失败并保留错误原因 |
| 共用规则提取不改变现有 Agent 行为 | `cargo test -p agent --release`：114 项通过，1 项按既有标记跳过 |
| 豆包不变 | 实施前后 12 个源码文件哈希一致 |

测试位于 `crates/app/tests/unit/gpt_live_prompt_tests.rs`，从生产 `gpt_live/prompt.rs` 挂载。固定身份、启动快照、记忆回执与数据边界断言均先在缺失内容时失败，再补对应实现。

## 验证记录

`cargo test -p app --release --lib gpt_live::prompt::tests -- --nocapture`：3 项通过，0 失败。此结果来自 App 原生测试目标，使用真实人格加载函数与启动消息序列化。

`cargo check -p app --release`：通过，覆盖生产配置下的启动接线及跨 crate 引用。

格式检查、单文件行数和禁止 `#[allow]` 检查已通过。独立审查指出的启动快照与固定记忆规则遗漏均已修正并复核。

本地红灯记录、规则原文比较与源码 manifest 位于 `/Users/user/.codex/artifacts/gpt-live-20261018/diagnosis/prompt-integration/`。

## 生效与验证边界

- 需要使用更新后的构建并重新建立 Live 会话；已经连接的会话不会自动替换提示词。
- 本次没有迁移用户配置、操作真实麦克风或发起云端通话，未声称模型的人格表现已经完成真人验收。
- 本次未替换或重启常驻 `/Applications/Norma OS.app`。
