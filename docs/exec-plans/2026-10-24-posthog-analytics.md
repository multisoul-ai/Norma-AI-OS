# PostHog 产品分析接入方案

> 日期：2026-10-24　状态：待确认
> 依据：[平台调研](../research/2026-07-28-independent-developer-product-analytics-platforms.md)（PostHog Cloud 为首选）

## 目标

回答两个问题：**多少用户在用**（DAU/WAU/MAU、新增、留存）与**用户用什么功能**（功能采用率、Agent/Terminal/Voice 漏斗）。

非目标：注册总数（以 Keygate 授权后端为准）、Session Replay、任何用户内容采集、官网访问统计（另用 Umami/Plausible）。

## 设计决策

| 决策 | 理由 |
| --- | --- |
| 用 PostHog **HTTP Capture API**（`POST /batch/`），不用 Rust SDK | 官方 SDK 偏服务端语义；桌面端需自管队列与 Session。workspace 已有 `ureq`，不新增依赖 |
| 新 crate `analytics`，不依赖 GPUI/Storage/Keychain | 遵循域 crate 约束，可独立测试 |
| 事件为强类型枚举，属性白名单 | 编译期杜绝误传自由文本（命令、Prompt、路径） |
| 发送在独立后台线程；热路径只做内存计数/`try_send` | 符合“不阻塞 GPUI 前台、PTY drain/鼠标/渲染不做 IO” |
| 离线队列落到 JSONL 文件（`dirs` 数据目录），不进 SQLite | 避免触碰 SQLite 热路径与 `check-storage-boundaries.sh` |
| 默认开启但提供 Settings 开关；关闭后立即清空队列并停止发送 | 隐私可退出；首启在 Settings/引导中披露 |

## 架构

```text
app (生命周期/功能点调用 analytics::track)
  └─ analytics  ── Event(enum) → Sanitizer(白名单) → Queue(mpsc) → Worker thread
                                                       ├─ DiskQueue(JSONL, 上限 5MB/1000 条)
                                                       └─ PostHogSink (ureq, 批量 20 条/30s, 指数退避)
```

依赖方向新增：`app -> analytics`；`analytics -> {serde, serde_json, ureq, dirs, sha2, hex}`。`analytics` 不得依赖 app/runtime/storage/ui。

### `crates/analytics` 模块

- `event.rs`：`Event` 枚举 + `properties()`（只返回 `&'static str` key 与枚举/bool/数值桶）。
- `identity.rs`：`anonymous_id`（首启随机 UUID，存本地文件）；`analytics_user_id = HMAC-SHA256(secret, 授权 UUID)`，**不用邮箱 / License Key**；`reset()`。
- `session.rs`：`ActiveTimer`（纯逻辑，注入时钟）：前台累计 `foreground_ms`，60 秒内有交互才累计 `active_ms`。
- `queue.rs`：内存通道 + 磁盘补发；启动时补发上次遗留。
- `sink.rs`：`trait Sink`（测试用 fake）+ `PostHogSink`。
- `config.rs`：`enabled`、`host`、`project_key`（PostHog 项目 key 是公开写入 key，可编译进二进制；dev/prod 用不同 key，测试环境不发送）。

## 事件模型（首批 10 个）

| 事件 | 属性 | 触发点 |
| --- | --- | --- |
| `app_session_started` | `session_id`, `app_version`, `os_version` | App 启动 |
| `app_foregrounded` / `app_backgrounded` | `session_id`, `foreground_ms`, `active_ms` | `observe_window_activation` |
| `feature_used` | `feature`(枚举), `surface`, `success` | 各功能入口 |
| `terminal_node_created` | `source`(shortcut/menu/dock) | Cmd+1 等创建路径 |
| `agent_card_created` | `provider_family` | Cmd+2~9 创建 Claude/Codex 等 |
| `agent_task_started` / `agent_task_completed` | `provider_family`, `entry_point`; `success`, `duration_bucket` | Agent 会话生命周期 |
| `voice_input_started` | `provider_kind`(local/cloud) | 语音输入 |
| `onboarding_step_completed` | `step` | 引导 |
| `analytics_opted_out` | 无 | 关闭开关（发送后清队列） |

`$set` 仅含 `app_version`、`os`、`first_seen_version`。**禁止**：命令/输出、工作目录与文件名、Prompt/回复、剪贴板、Key/邮箱、转写文本。

## 接入点（均在 `crates/app`）

1. `lib.rs` 启动：初始化 `analytics::start(config)`，返回轻量句柄存入 App 状态。
2. `app/lifecycle.rs` / `background_project.rs`：窗口激活/失活、退出时 `flush()`（最长等待 1s，**不得在前台线程同步等网络**，仅写盘）。
3. 用户输入入口（`runtime::route` 之后的 app 层副作用处）调用 `handle.touch()`，仅更新原子时间戳。
4. `settings/general*`：新增 “分析与隐私” 开关；文案列出采集类别、用途、保留期。
5. 授权成功后（`license_gate`）调用 `identify`；退出登录 `reset`。

## 步骤（TDD，每步独立可合并）

1. **crate 骨架 + Event/Sanitizer**：测试“未知属性被拒”“属性只含白名单 key”。
2. **ActiveTimer**：测试前台/后台切换、60 秒空闲不累计、时钟注入。
3. **Queue + DiskQueue**：测试断网积压、重启补发、上限淘汰最旧、关闭开关清空。
4. **PostHogSink**：用本地 `TcpListener` 假服务测试批量体、`distinct_id`、重试与退避；4xx 丢弃、5xx/超时重试。
5. **Identity**：测试 HMAC 稳定性、reset 后换 `anonymous_id`。
6. **App 接入 + Settings 开关**：最小事件先接 `app_session_started`/前后台/`feature_used`，再逐个补功能点。
7. **守卫脚本**：`scripts/check-analytics-boundaries.sh`——`analytics` 不含 gpui/storage 依赖、`app` 中不得出现 `ureq` 直连 PostHog 域名；并入 `check-performance-guardrails.sh` 检查热路径无同步 IO。
8. **文档**：`docs/references/`（事件字典）、隐私政策披露、CLAUDE.md 增加 `analytics` crate 与依赖方向（保持 ≤300 行）。

## 验收

- 单元：上述各步测试通过；`cargo test -p analytics`、`cargo check -p app`、`bash scripts/check-max-file-lines.sh`、`check-no-allow.sh`、`check-performance-guardrails.sh`。
- 真实 App：开关开启时，创建 Terminal/Claude 卡后 PostHog Live Events 出现对应事件；关闭开关后无新事件且本地队列文件被清空；断网操作后联网补发。
- 性能：拖拽、pan、PTY 输出时 `analytics` 线程无 CPU 抖动，前台无新增阻塞（`frame_diagnostics` 对比）。
- PostHog 控制台搭建：DAU/WAU/MAU Insight、`feature_used` 按 `feature` 分组、“启动 → 创建 Agent → 任务完成”漏斗、周留存。

## 风险与待定

- **无账号体系**：未登录用户只有 `anonymous_id`，重装/多设备会重复计数；注册总数仍看 Keygate。
- **区域与合规**：PostHog US/EU 实例二选一，需先定并写入隐私政策；默认开启 vs 首启征求同意需产品/法务确认。
- **事件量**：`feature_used` 须去抖（同功能 10 秒内合并），避免免费额度（100 万/月）被打满。
- **project key**：需要你在 PostHog 创建 dev/prod 两个项目并提供 key。
