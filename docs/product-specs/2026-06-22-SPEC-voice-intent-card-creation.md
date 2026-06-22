# 语音识别 + 意图识别 + 创建/驱动卡片 SPEC

日期：2026-06-22

## 1. 背景与目标

Multisoul Desktop 的画布已有 Terminal / Agent live node。第一版语音能力要把底部预留的 `Listening...` 区域升级为语音控制条，让用户在编码时通过语音创建 Terminal/Agent，或把语音内容路由到指定 Agent。

目标：

- 按住说话、松开识别。
- 高置信度意图直接执行。
- 低置信度、重名、不存在目标或高风险命令必须确认/不自动执行。
- 优先本地语音识别，目标体验为松开后尽量 2 秒内得到执行或确认结果。

## 2. 范围

### 2.1 In Scope

- 底部语音控制条：按住录音、松开结束。
- Apple Speech 优先的语音识别 provider。
- Whisper.cpp 后备 provider 边界和评测门槛。
- 无 LLM MVP 意图识别：规则 + 槽位提取 + 模糊别名匹配。
- 支持 Terminal 和 Agent 两类卡片：
  - “创建终端”
  - “创建 Codex”
  - “创建 Claude”
  - “Alan，我们继续聊聊技术方案” → 路由到 alias/name 匹配的 Agent。
- Agent 名称 = profile + 用户自定义 alias。
- 预制英文短 alias，例如 `Alan`、`Nova`、`Koda`、`Echo`、`Iris`、`Sage`。
- 转写与意图历史落盘，暂不做历史 UI。
- 低置信度/高风险动作使用居中模态确认。

### 2.2 Out of Scope

- 文本、任务、网页卡片。
- 跨设备同步语音历史。
- 唤醒词或常驻监听。
- LLM 语义意图识别。
- 历史记录可视化 UI。
- 全局系统级语音快捷键。

## 3. 用户与场景

主要用户：编码中的 Multisoul Desktop 用户。

代表场景：

- 用户按住底部语音条说“创建终端”，松开后新建 Terminal。
- 用户说“创建 Codex”，新建 Codex Agent 并沿用现有布局/reflow。
- 用户说“Alan，我们继续聊聊技术方案”，系统识别 `Alan` 为某个 Agent alias，并把文本发送给该 Agent。
- 如果存在多个 `Alan` 或没有 `Alan`，只展示转写结果，不自动投递。

## 4. 主流程

1. 用户在画布底部语音条按住录音。
2. UI 进入 `Recording` 状态，松开后进入 `Transcribing`。
3. `voice_macos` 调用 Apple Speech 得到 transcript。
4. `voice` 域层执行意图解析：
   - 规则识别创建命令。
   - 槽位提取 action/card/profile/alias。
   - 模糊匹配 Agent alias。
   - 计算 confidence 和 risk。
5. 高置信度安全动作直接执行。
6. 低置信度、目标歧义、高风险 Terminal 命令弹出居中模态确认。
7. 写入 `voice_history`，记录 transcript、intent、confidence、status，不保存音频。
8. UI 回到 Idle 或显示失败/确认状态。

## 5. UI/UX

底部语音控制条：

- 默认状态：Idle，显示麦克风入口和最近状态。
- 按住：Recording，显示录音中反馈。
- 松开：Transcribing / Resolving。
- 完成：短暂显示执行结果，例如 `Created Terminal`、`Sent to Alan`。
- 失败：显示 transcript 或错误摘要，不执行动作。

确认模态：

- 居中显示 transcript、识别出的动作、目标卡片、风险提示。
- 操作：确认执行 / 取消。
- 高风险 Terminal 命令必须确认后才允许回车执行。

## 6. 数据模型

### Agent alias

当前 `AgentSpec` 只有 `profile`。需要扩展：

```rust
pub struct AgentSpec {
    pub profile: AgentProfile,
    pub alias: Option<String>,
}
```

兼容规则：

- 老 snapshot 没有 alias 时，恢复为 `None`。
- 新建 Agent 时自动分配一个未占用预制 alias，用户可修改。
- display title 优先显示 alias，其次显示 profile title。

### Voice history

项目级 SQLite 新增表：

```sql
CREATE TABLE IF NOT EXISTS voice_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  transcript TEXT NOT NULL,
  intent_json TEXT NOT NULL,
  confidence REAL NOT NULL,
  status TEXT NOT NULL,
  target_node_id INTEGER,
  created_at_unix INTEGER NOT NULL,
  error TEXT
);
```

不保存音频。历史写入不得发生在输入投递、runtime route、PTY output、mouse move 等热路径内。

## 7. 技术实现

### Crate 边界

新增：

- `crates/voice/`
  - 零 GPUI 域层。
  - 定义 `Transcript`、`VoiceIntent`、`IntentAction`、`IntentResolution`、`RiskLevel`。
  - 定义 `SpeechProvider` / `IntentResolver` trait。
  - 实现规则、槽位和模糊别名解析。
- `crates/voice_macos/`
  - macOS provider。
  - Apple Speech 优先。
  - 后续接入 Whisper.cpp fallback。
- `crates/storage/`
  - 新增 `VoiceHistoryStore`。
- `crates/app/`
  - 底部语音条 UI。
  - 录音状态机。
  - 居中确认模态。
  - 将 resolved intent 转换为 runtime/app 副作用。

### STT 策略

默认：Apple Speech primary。

评测门槛：

- 建立至少 30 条中文/中英混合测试语句。
- 覆盖创建 Terminal、创建 Codex/Claude、Agent alias 路由。
- 目标：短句松开后 2 秒内完成转写与意图解析。
- 若 Apple Speech 在目标机器上中文/中英混合效果不足，再启用 Whisper.cpp fallback。

Whisper fallback：

- 作为 provider 边界预留。
- 不作为第一实现强依赖，避免模型打包和加载成本阻塞 MVP。

### 意图识别策略

MVP 不使用 LLM。

动作枚举：

- `CreateTerminal`
- `CreateAgent { profile: Claude | Codex }`
- `SendTextToAgent { node_id, text }`
- `SendTextToTerminal { node_id, text, submit: bool }`
- `NeedsConfirmation`
- `NoAction`

解析方式：

- 规则匹配明确创建命令。
- 槽位提取 `action`、`card_type`、`profile`、`alias`。
- alias 做大小写、空格、常见中文音译归一化。
- 对 alias 使用模糊匹配；低于阈值视为不确定。
- 目标不存在或重名：不自动执行，只显示 transcript。

## 8. 错误与边界情况

- 麦克风权限未授予：提示用户授权，不开始录音。
- Apple Speech 不可用：显示错误；若 fallback 已配置则切换 Whisper。
- 识别为空：显示“未识别到内容”，不执行。
- 只有 transcript、无明确 intent：显示 transcript，不执行。
- Agent alias 重名：显示 transcript 和歧义原因，不投递。
- 目标 Agent 不存在：显示 transcript，不新建同名 Agent。
- 高风险 Terminal 命令：必须确认。
- 持久化失败：不影响本次执行，但显示非阻塞错误并写日志。

## 9. 非功能性需求

- 性能：短句从松开到执行/确认尽量 2 秒内。
- 隐私：不保存音频；仅保存 transcript 和 intent history。
- 可测试性：`voice` 域层可独立 `cargo test`。
- 可维护性：语音 provider、意图 resolver、UI 副作用分层。
- 依赖方向：不得让 `canvas/terminal/runtime/storage` 反向依赖 `app/ui`。

## 10. 验收标准

- 按住底部语音条说“创建终端”，松开后创建 Terminal，并沿用现有 reflow。
- 说“创建 Codex”，创建 Codex Agent。
- 已存在 alias=`Alan` 的 Agent 时，说“Alan，我们继续聊聊技术方案”，文本发送到该 Agent。
- 如果两个 Agent 都叫 `Alan`，系统不发送，只显示 transcript/歧义提示。
- 如果说出高风险 Terminal 命令，系统弹出居中确认，不直接回车。
- 成功和失败都写入 `voice_history`。
- 不保存音频文件或音频 blob。
- `cargo test -p voice`、`cargo test -p storage`、`cargo check -p app` 通过。
- 新增测试遵守仓库测试注释和断言规范。

## 11. 参考资料

- Apple Speech framework: https://developer.apple.com/documentation/speech
- Apple SpeechAnalyzer: https://developer.apple.com/documentation/speech/speechanalyzer
- whisper.cpp: https://github.com/ggml-org/whisper.cpp
- whisper-rs: https://github.com/tazz4843/whisper-rs
- CPAL: https://github.com/RustAudio/cpal
