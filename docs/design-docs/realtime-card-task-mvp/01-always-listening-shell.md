# 01 Always Listening 壳子 SPEC

- 日期：2026-07-06
- 状态：草稿
- 上游：无
- 下游：`02-realtime-sidecar-ipc.md`、`03-realtime-voice-chat.md`

## 1. 目标

建立 Always Listening 的产品入口、配置入口和最小状态显示，但暂时不要求真正连通 OpenAI Realtime。这个 SPEC 的完成标准是：用户能在 Settings 中打开或关闭 Always Listening，app 能进入明确的 realtime lifecycle 状态，并且关闭时能释放本阶段拥有的资源。

## 2. 范围

本阶段要做：

- Settings -> Voice 新增 `Realtime conversation` 区域。
- 新增 `Enable Always Listening` 开关。
- 新增 OpenAI API Key、Realtime model、Card planner model、Sidecar path / bundled sidecar status 配置项。
- 底部语音条支持 realtime 基础状态。
- App 内部只允许一个 active realtime session slot。
- Open project / close project / app close 时能停止 Always Listening 壳子状态。

本阶段不做：

- 不连接 OpenAI Realtime。
- 不启动真实 sidecar。
- 不采集或播放真实 realtime 音频。
- 不执行卡片任务。
- 不把配置写到环境变量 fallback。

## 3. 用户流程

```text
用户打开 Settings -> Voice
  -> 打开 Enable Always Listening
  -> app 校验 OpenAI API Key / model / sidecar path 配置
  -> 配置有效则底部语音条进入 Always Listening idle / connecting
  -> 用户关闭开关
  -> app 停止 realtime 壳子状态
  -> 底部语音条回到 Voice
```

如果配置缺失：

```text
用户打开 Enable Always Listening
  -> app 不启动 session
  -> 底部语音条显示 Realtime error 或配置错误状态
  -> Settings 中保留用户输入，等待修正
```

## 4. 状态模型

建议状态：

```rust
pub enum RealtimeUiState {
    Off,
    ConfigError { message: String },
    Connecting,
    Listening,
    UserSpeaking,
    AssistantSpeaking,
    CardTaskRunning,
    Error { message: String },
}
```

底部语音条文案：

| 状态 | 文案 |
|---|---|
| `Off` | `Voice` |
| `Connecting` | `Connecting...` |
| `Listening` | `Always Listening` |
| `UserSpeaking` | `Listening...` |
| `AssistantSpeaking` | `Replying...` |
| `CardTaskRunning` | `Running card task...` |
| `Error` / `ConfigError` | `Realtime error` |

## 5. 技术边界

- Settings 存储仍走现有 settings 路径。
- `crates/app` 负责 GPUI 入口和状态映射。
- 允许提前定义 `crates/realtime` 的轻量配置类型，但不要在本阶段引入网络逻辑。
- `runtime/canvas/terminal/storage` 不应因为本阶段产生依赖变化。

## 6. 验收点

自动验收：

- Settings values 可以保存和读取 Always Listening 配置。
- OpenAI API Key / Realtime model / Planner model 缺失时，不能进入 active session。
- 重复打开开关不会创建第二个 active realtime session slot。
- 关闭开关后状态回到 `Off`。
- app close / project close 会触发 stop path。

手动验收：

- 打开 app，进入 Settings -> Voice，可以看到 `Realtime conversation` 区域。
- 填入配置并打开 `Enable Always Listening`，底部语音条从 `Voice` 变为 `Always Listening` 或 `Connecting...`。
- 清空 API Key 后打开开关，底部语音条显示错误状态，app 不崩溃。
- 关闭开关后，底部语音条回到 `Voice`。

## 7. 失败与边界

- 非 project/canvas 状态下打开 Always Listening，应拒绝启动并显示错误。
- 配置错误不弹 modal，优先通过底部语音条和 Settings 文案反馈。
- 本阶段不要求真实网络失败处理；真实 sidecar 失败放到 `02-realtime-sidecar-ipc.md`。
