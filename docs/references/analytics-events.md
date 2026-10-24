# 产品分析事件字典

实现：`crates/analytics`（域层）+ `crates/app/src/telemetry.rs`（适配）。方案：[`docs/exec-plans/2026-10-24-posthog-analytics.md`](../exec-plans/2026-10-24-posthog-analytics.md)。

构建期环境变量：`NORMA_POSTHOG_KEY`（缺省则整体 no-op）、`NORMA_POSTHOG_HOST`（默认 `https://us.i.posthog.com`）、`NORMA_ANALYTICS_SALT`。用户开关在 Settings → 通用 → 使用统计，状态以 `~/.norma/analytics/analytics_opted_out` 标记文件持久化。

| 事件 | 属性 | 接入点 |
| --- | --- | --- |
| `app_session_started` | app_version, os_version | `telemetry::install` |
| `app_foregrounded` / `app_backgrounded` | foreground_ms, active_ms | 窗口激活观察 |
| `license_activated` | success | `license_gate/gate_root.rs`（成功后 `identify`） |
| `terminal_node_created` | source（暂为 other） | `app/mod.rs` |
| `agent_card_created` | agent | `app/agent_actions.rs` |
| `agent_task_started` / `agent_task_completed` | agent, entry_point / success, duration_bucket | `app/frame_loop.rs` hook 事件 |
| `preview_card_opened` / `browser_card_created` / `idea_created` | — | 各创建入口 |
| `feature_used` | feature, success（同功能 10 秒去抖） | 语音录音、壁纸 |
| `analytics_opted_out` | — | 设置开关 |

未接入（枚举已定义，待找到稳定入口）：`app_updated`、`voice_input_started`（需区分本地/云）、`norma_assistant_used`、`mobile_connection_enabled`、`mobile_device_paired`、`wallpaper_changed`、`onboarding_step_completed`、`setting_changed`。

禁止上报：命令/输出、路径与文件名、Prompt/回复、剪贴板、Key/邮箱、转写文本。新增事件必须在 `crates/analytics/src/event.rs` 中以枚举/布尔/数值桶表达。
