# 豆包常驻播报通道 实施计划

日期：2026-10-21

## 背景

豆包模式下，只有用户按下 Cmd+E 才会启动整条会话（播报 + 收音 + 对话）。会话没有启动时，Agent 完成通知会以 `Waiting` 状态排进队列（上限 32 条，不会过期）。用户之后一开语音，这些积压的通知就会一次性连续播完。

根因位于 `crates/app/src/app/agent_announcements/availability.rs`：只要存在凭据，Doubao 分支就返回 `Waiting`，而 `reports` 只有在会话启动后才存在。

## 目标行为

1. 项目打开后，若语音模式为豆包且已保存凭据，自动启动**播报通道**并在后台常驻；启动时不打开麦克风。
2. Cmd+E 只开关**收音**：
   - 单次指令模式：按住录音。
   - 持续聆听模式：开关 ASR 与麦克风。
   - 停止收音时不关闭通道。
3. 收音失败只结束这一次收音，通道保留，Dock 显示收音失败状态。
4. 通道断开后自动退避重连，间隔为 1、2、4、8、16、32、60 秒，之后保持 60 秒。鉴权或凭据类失败不重连，直到用户修改设置或手动按 Cmd+E。
5. 关闭项目窗口、项目转入后台时，保留通道并停止收音。窗口重新打开时，通道绑定的是旧窗口，此时重建通道，让工具调用重新绑定到新窗口。
6. 通知兜底（只在通道连接中或重连期间生效）：
   - 排队超过 60 秒的通知直接丢弃。
   - 放行时，同一时刻就绪的多条通知合成一句播报。
   - 通道被永久阻断时不再排队。

## 设计

### 状态拆分（`DoubaoVoiceState`）

| 状态 | 字段 | 控制方 |
|---|---|---|
| 通道 | `cancellation`、`reports`、`session_task`、`binding` | `reconcile_doubao_channel`，在每个运行时帧调用，开销 O(1) |
| 收音 | `recording`（单次）、`listening`（持续）、`listens` | Cmd+E、松键、窗口失活、后台化 |
| 重连 | `retry_attempt`、`retry_at`、`blocked` | 通道断开时设置；`stop()` 或成功连上时清除 |

- 通道的启停判定写成纯函数 `channel_action`，用单元测试覆盖。
- 启动通道需要 `Window`。帧循环中没有 `Window`，因此通过 `project_window` 句柄异步启动，并用 `start_pending` 防止同一时刻重复启动。

### 会话层

- `plan.rs`：持续聆听模式不再在会话启动时调用 `asr::run`，改为由 `listen_continuous` 等待 Cmd+E 下发的收音令牌。
- 单次与持续两种模式的收音失败，都改为发送 `SpeechEvent::ListenEnded(id, error)`，不再结束会话。
- `run_session` 初始化完成后发送 `Connected`，表示通道已就绪。
- 状态循环由窗口绑定的 100 ms 轮询，改为实体绑定的事件驱动接收，使项目转入后台后通道仍能处理状态。项目切换的检测移到 reconcile 中。

### UI

- `waiting_for_input()` 改为 `Connected && !is_listening()`，两种模式统一用这个判断。
- 新增 `DoubaoStatus::ListenFailed`，标签为「收音失败」，显示为错误视觉；下次按 Cmd+E 时清除。

## 测试映射

| 验收项 | 测试 |
|---|---|
| 通道启停判定 | `doubao_channel_tests.rs`：`channel_action` 覆盖模式、凭据、项目、窗口、退避、阻断、换项目、换窗口 |
| 退避与永久失败 | `doubao_channel_tests.rs`：`retry_delay`、`is_permanent_failure` |
| 收音失败不结束会话 | `doubao_listen_tests.rs`：持续 / 单次两种 listener 出错后仍接受下一次收音 |
| 会话转发 ListenEnded | `doubao_hold_conversation_tests.rs` |
| Cmd+E 只开关收音 | `doubao_channel_app_tests.rs`：持续模式按两次 Cmd+E，通道保持，令牌已取消 |
| 后台化保留通道 | `doubao_channel_app_tests.rs`：detach 后 `is_active()` 仍为真，收音已停 |
| 通知 TTL 与合并 | `doubao_announcements_tests.rs` |
| 真实 App | 启动豆包模式的项目，不按 Cmd+E，让 Agent 完成任务，应立即听到播报；Dock 显示待命，系统不出现麦克风占用标记 |

## 风险

- 同时打开多个项目时，每个项目各有一个通道，两条通知可能重叠播放。
- 首次播报后，`AVAudioEngine` 会保持运行到通道关闭（行为与原先的会话期间一致）。
