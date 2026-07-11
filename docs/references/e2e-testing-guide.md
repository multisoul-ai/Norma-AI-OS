# E2E Testing Guide

`crates/e2e` 是 Multisoul Desktop 的端到端测试 crate。当前包含三类测试：

- app/headless GPUI e2e：在 `TestAppContext` 中驱动真实 `MultisoulApp` 状态机，
  覆盖 boot -> Canvas -> `NewTerminal` -> PTY 输入 -> grid 渲染链路。
- domain-level e2e：使用公开 crate API 串起跨 crate 用户流程，不依赖 app 私有
  `#[cfg(test)]` 入口。本地语音管线（`dialogue::run_voice_turn` + 扁平 card tools）
  的公开边界验收属于这一层。
- L3 真实 `.app` 截图 runner：启动打包产物，经 macOS 菜单/快捷键创建卡片，保存
  用户可见的画面。它不替代确定性的 L1/L2 测试。

参考：实现计划 [`docs/exec-plans/2026-06-28-e2e-testing-infra.md`](../exec-plans/2026-06-28-e2e-testing-infra.md)；
SPEC [`docs/product-specs/2026-06-28-SPEC-e2e-testing.md`](../product-specs/2026-06-28-SPEC-e2e-testing.md)。

## 1. 三层模型

| 层 | 后端 | 用途 | CI 默认 |
| --- | --- | --- | --- |
| **L1 注入剧本** | `TestPtyBackend`（`terminal` crate）：`write(bytes)` 同步回吐预设剧本 / 回显 | CI 主防线，快、确定 | 跑 |
| **L2 真 PTY** | 真实 `portable_pty` 子进程（`/bin/zsh`） | 本地冒烟，覆盖真 PTY 异步链路 | 跳过（`#[ignore]`） |
| **L3 真实 `.app`** | 打包的 macOS app + System Events + `screencapture` | 用户可见的真实窗口/菜单/截图验收 | 手动 |

L2 标 `#[ignore]`：真 PTY 输出依赖 host 时钟与 shell rc，CI 易抖动；本地手动跑。

## 2. 运行命令

```bash
# L1（默认，进 CI）
cargo test -p e2e

# L2（真 PTY 冒烟，本地）
cargo test -p e2e -- --ignored

# L3：先打包 ad-hoc .app；需要 Accessibility 与 Screen Recording 权限
cargo build --release -p app
cargo packager -c Packager.dev.toml
bash scripts/run-card-init-animation-l3.sh
bash scripts/run-card-init-animation-l3.sh "build/dev/Norma OS.app" artifacts/l3-agent "$PWD" agent

# 全 workspace（含 L1 e2e + 全部 domain/app 单测；L2 自动跳过）
cargo test --workspace

# 单个 e2e 测试
cargo test -p e2e e2e_01
cargo test -p e2e e2e_02_terminal_input_echoes
cargo test -p e2e test_local_voice_create_then_send_text_pipeline
```

`gpui` 的 `test-support` feature 仅在 `e2e` crate 局部启用，`cargo build -p app`
生产构建不受影响。

## 2.1 初始化动画 L3 截图

`scripts/run-card-init-animation-l3.sh` 是独立 L3 runner：它会启动实际打包的
`.app`，通过 `Cmd+O` 进入指定 workspace，再以 `Cmd+N`（Terminal）或应用菜单
（Codex Agent）创建卡片，并保存 `<kind>-000ms.png`、`-500ms.png` 与 `-1000ms.png`。
截图默认落在 `artifacts/l3-card-init-animation/`。它要求运行 shell 具有 macOS
Accessibility 与 Screen Recording 权限；受这些机器权限影响，不进入 CI 默认集。首张
截图在创建 action 所在的同一 System Events transaction 内发起，避免 action 返回后的
额外 shell 往返；`000ms` 是该即时采样标签，不承诺绕过 macOS 的截图调度延迟。每次运行
会生成 `l3-run-result.txt`，权限或自动化失败时也会留下失败状态和授权提示。

## 3. Agent 验收用法

subagent 完成涉及 Terminal / Canvas / 焦点路由 / PTY 链路的功能后，把相关 e2e
测试当作"功能做完了没"的自动判定：

1. 跑与改动相关的 e2e 测试（如改了 spawn 路径 -> `e2e_01`；改了 input/drain -> `e2e_02`）。
2. 绿 = 功能闭环；红 = 回到 `superpowers:systematic-debugging` 排查，不要手动"看起来对了"就宣告完成。
3. 改动若影响真 PTY 路径，本地补跑 `cargo test -p e2e -- --ignored`。

本地语音管线（ASR → LLM 工具 → 卡片命令 → 模板播报决策）这类跨 crate 协议闭环，
优先补 domain-level e2e；真实语音、麦克风、扬声器和 Ollama/sherpa 联机验收不能由
`cargo test -p e2e` 替代。

## 4. Harness Helper 速查

helper 定义在 `crates/e2e/src/harness.rs`，经 `lib.rs` 重导出。

| Helper | 一句话 |
| --- | --- |
| `make_app(cx, factory)` | 装配 headless GPUI 窗口 + `MultisoulApp::new_with_pty_factory`，注入测试 PTY 工厂，focus root view |
| `make_app_real(cx)` | 同上但不注入工厂，走真实 `portable_pty` 路径（L2 专用） |
| `enter_canvas(view, cx, root)` | 调 `enter_canvas_for_test` 直接置 `screen = Canvas` + 注入 cwd，绕开 `prompt_open_project` 的 unimplemented 路径 |
| `create_terminal(view, cx)` | dispatch `NewTerminal`，返回新建 Term 节点 id |
| `read_runtime(view, cx, f)` | 借读 `&Runtime` 投影，不触发 notify |
| `read_grid(view, cx, node_id)` | 取指定 Term 节点的 `GridSnapshot`，节点不存在返回 `None` |
| `drain_until(view, cx, pred, timeout)` | 轮询 `advance_clock(12ms)` + `run_until_parked` + 读 runtime，直到谓词返回 `Some` 或超时 |
| `echo_factory()` | echo PTY 工厂：`write(bytes)` -> 回吐相同 bytes |
| `scripted_factory(rules)` | 剧本 PTY 工厂：`write` 含 `rule.trigger` -> 投递 `rule.output` |

## 5. Mock 时钟与 drain timer（关键）

GPUI 的 `TestAppContext` 用 mock 时钟：`background_executor.timer(dur)` 返回的 future
仅在 `advance_clock(dur)` 后才就绪。`MultisoulApp` 的 12ms 周期 drain loop
（`crates/app/src/app/mod.rs`）依赖该 timer 触发 `runtime.drain_all()`。

**仅 `run_until_parked` 不够**：mock 时钟不前进，drain 永不执行，PTY 输出停在
`TerminalSession` 的 rx 队列里，grid 不更新。

`drain_until` 每轮显式 `advance_clock(12ms)` 让 drain timer 就绪，再
`run_until_parked` 推进 executor 执行 drain 回调。任何依赖 timer 驱动输出的 e2e
断言必须经 `drain_until`，不能裸 `run_until_parked`。

## 6. 既有用例

| 测试 | 文件 | 覆盖 |
| --- | --- | --- |
| `smoke_headless_gpui_boots_to_launchpad` | `lib.rs` | headless boot -> `Launchpad` |
| `harness_enters_canvas_and_spawns_terminal` | `harness.rs` | harness 自身可用性烟雾 |
| `e2e_01_*` | `e2e_01_create_terminal.rs` | `NewTerminal` -> 节点数 + 焦点 + layout reflow 正/负断言矩阵 |
| `e2e_02_terminal_input_echoes_scripted_output_into_grid` | `e2e_02_terminal_echo.rs` | L1 剧本：`echo hi` -> grid 含 `hi` |
| `e2e_02_real_pty_echo_hi_into_grid` | `e2e_02_real_pty.rs` | L2 真 PTY（`#[ignore]`）：真 zsh `echo hi` -> grid 含 `hi` |
| `e2e_03_settings_keyboard_shortcut_can_be_changed_and_restored_by_clicks` | `e2e_03_settings_clicks.rs` | Settings 真点击链路：toolbar -> Keyboard -> keycap 录制 -> Restore -> Close |
| `test_local_voice_create_then_send_text_pipeline` | `tests/local_voice_card_tools_pipeline.rs` | domain-level：Mock LLM Create→SendText → `run_voice_turn` 命令顺序 |
| `test_template_report_decision_speaking_queues` | `tests/local_voice_card_tools_pipeline.rs` | domain-level：`decide_template_report` Speaking → Queue |
| `test_template_report_decision_listening_plays_now` | `tests/local_voice_card_tools_pipeline.rs` | domain-level：`decide_template_report` Listening → PlayNow |

## 7. 新增 e2e 测试检查清单

1. app/headless GPUI 用例复用 `harness` helper，不要直接 `add_window_view` 重复装配逻辑。
2. PTY 链路用 `scripted_factory`（确定性）优先；只有覆盖真 PTY 异步路径才用 `make_app_real` 并标 `#[ignore]`。
3. 任何依赖 drain 的断言走 `drain_until(pred, timeout)`；timeout 注释推导（L1 ~500ms，L2 ~2000ms）。
4. domain-level e2e 优先测试公开 API 边界；不要为了 e2e 暴露 app 私有测试 helper。
5. 断言带失败信息字符串；正断言 + 负断言成对（"应含 X" + "不应含 Y"）。
6. 测试函数上方文档注释：场景 / 数据构造（含数值推导）/ 执行过程 / 预期结果。
7. 点击 UI 控件时优先用 `debug_selector` + `VisualTestContext::simulate_click`。`debug_bounds`
   只用于点击前定位；GPUI 0.2.2 不会在每帧清空 debug bounds，元素消失后的负断言要走
   app snapshot 或其它状态证据。
