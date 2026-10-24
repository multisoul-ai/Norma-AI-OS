# GPT-Live 配置与输入控制整改验收

范围：`feat/gpt-live-duplex` worktree。本轮修正截图中的旧配置，以及与其关联的后台准入、独立交互和补字调度。

## 实际改动

- GPT 页面移除“项目工具／智能体供应商”、模型、地址与手工准备 VAD；保留 Key、交互方式、麦克风、音色。连接、静音、播报与操作记录是状态操作。
- 工具后台从 GPT 自身的活动凭据引用读取 Key，固定使用 `https://api.openai.com/v1` 和 `gpt-5.4-mini`；不读取或改写全局 Norma 模型配置。语音仍使用 `gpt-live-1`。
- 连接时在后台自动准备 GPT 独立的 VAD 目录。失败保留聊天能力，并显示项目工具暂不可用及重连重试提示。
- GPT 实时／按住偏好独立持久化；默认实时。按住模式支持松 E、松修饰键、失焦和冷启动松键取消。设置页明确提示返回主界面使用快捷键。
- 松键停止采集，排空已采尾音；主动隐私静音立即丢弃待上传声音，后续按键不会覆盖该选择。
- 工具调度等待最后真实尾帧的本地上传观察，后续协议静音不会阻塞释放；这不是服务端转写完成证明。输入确认携带 revision，旧确认不能放开新一轮输入。
- 按住开始同步冻结旧原文，直到新的用户转写证据到达。补字取消的无副作用规划保留原委派及稳定意图；已有副作用或待定结果不自动重做。

## 验收映射与证据

| 要求 | 证据 | 结果 |
| --- | --- | --- |
| 只保存 Live Key，旧全局后台失败也不拦工具 | `live_call_and_delegation_have_independent_readiness` 先在旧实现失败，再修准入 | 通过 |
| 固定官方后台，Key 替换／删除／外来引用不回退 | `live_backend_uses_only_live_key_and_official_defaults`、`live_backend_rejects_stale_missing_and_foreign_credentials` | 通过 |
| 独立模式持久化、主动静音、冷启动取消 | `gpt_live_hold_tests.rs` 中设置与状态测试 | 通过 |
| 不以松键恢复旧授权 | `live_hold_release_waits_for_new_transcript_before_replanning` | 通过 |
| 200ms 尾音完整上传，停止确认后丢弃迟到 PCM | `live_hold_audio_gates_pcm_without_overriding_manual_mute`，真实本地 WebSocket | 通过 |
| 尾帧待发时不放行，后续静音积压时也能最终放行 | `live_hold_waits_for_transport_to_send_the_last_frame`，实际 transport 队列与可控时钟 | 通过 |
| 单个委派遇补字继续，无需第二次通知；副作用不重做 | Bridge 单元测试与生产源码离线回放 | 通过 |
| 采集停止／重新打开／满队列取消 | 独立编译生产 capture 模块及 `e2e-test-support` 原测试；现有音频 fixture | 2 项通过 |
| GPT App 回归 | `cargo test -p app --release --lib gpt_live -- --nocapture` | 62 项通过，0 失败 |
| 豆包兼容性 | 同一新 App 测试目标运行 `doubao` 过滤 | 71 项通过，2 项沿用 ignored |
| 旧模式解析与默认值 | 同一目标运行 `desktop_live_mode_roundtrips` | 1 项通过 |
| UI 目录与共享基础 | `cargo test -p ui --release` | 12 项通过 |
| 生产编译检查 | `cargo check -p app --release --tests` | 通过 |
| 新 App 构建 | `cargo build -p app --release` | 通过 |
| 真实页面 | `scripts/desktop-e2e.sh settings-live-configuration` | 重跑退出 0，6 步通过，33.10 秒；8 张图已逐张查看 |

完整 App 测试使用真实生产媒体循环、本地 WS 和项目 Adapter；GPUI 测试上下文不等于真实窗口验收。额外的快速媒体 harness 仅替换未使用的物理播放边界，结果不作为物理播放证据。

## 真实窗口证据

本次成功运行目录：`artifacts/desktop-workspace-e2e/2026-10-18T15-40-18Z/`。本次 Release 二进制复制到脚本规定的 `target/debug/norma-os` 后验收，两者 SHA-256 相同。

| 截图 | 视觉结论 |
| --- | --- |
| `01-live-dark.png` | 符合：GPT 已选，缺 Key 提示及禁用连接／静音／停止播报可辨 |
| `02-live-disabled.png` | 符合：点击连接后仍未连接 |
| `03-audio-dark.png` | 符合：实时模式及 Cedar 勾选、麦克风行可读 |
| `03b-simple-dark.png` | 符合：按住模式与 Cedar 勾选；页面到底，无供应商、模型或手工 VAD 配置 |
| `04-live-light.png` | 符合：浅色 GPT 选择及禁用操作清晰 |
| `05-audio-light.png` | 符合：重启后按住／Cedar 保持，浅色可读 |
| `06-doubao-regression.png` | 符合：豆包原连接及交互区域，GPT 区域未混入 |
| `07-legacy-regression.png` | 符合：旧语音引导恢复，GPT 未选中 |

首次运行 `2026-10-18T15-28-19Z` 因 Vision OCR 抛出 `e5rtError(..., 13)`，脚本记录 PRODUCT、退出 1，无截图。该记录保留。期间 Computer Use 在独立包上确认深色页面和保存偏好。随后默认 Vision 独立复测成功；使用相同 App 二进制、相同场景和断言重跑通过。两次自动尝试共 1/2 通过，不能把首次失败记为成功。未修改 OCR 引擎或弱化断言。

## 修改边界

本轮前后 22 个豆包文件 SHA-256 一致。未重启或替换常驻 `/Applications/Norma OS.app`，未迁移用户设置，未发起云端通话或读取测试 Key 内容。

独立只读审查指出的旧授权抢跑、松键截尾、上传提前放行和持续静音阻塞释放问题均已修正并复核。Rust 行数、禁止 `#[allow]` 和 `git diff --check` 通过。

日志根目录：`/Users/user/.codex/artifacts/gpt-live-20261018/`。主要记录为 `config-final-tests.log`、`config-doubao-regression.log`、`config-ui-tests.log`、`config-capture-tests.log`、`config-release-build.log`。

## 尚未验收

- 真实账户对 `gpt-live-1` 与 `gpt-5.4-mini` 的权限和云端效果。
- 真人通过 DJI 麦克风创建智能体卡片、连续派任务、查询结果和插话。
- 耳机／外放的声学效果。以上留给用户统一测试，不能以本地受控测试替代。

测试启动器：`/Users/user/.codex/artifacts/gpt-live-20261018/run-gpt-live.command`。测试包与原隔离验收数据绑定，沿用已保存 Key；本轮未读取其内容。
