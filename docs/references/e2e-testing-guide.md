# E2E Testing Guide

`crates/e2e` 是 Multisoul Desktop 的端到端测试 crate：在 headless GPUI `TestAppContext` 中驱动真实 `MultisoulApp` 状态机，覆盖 boot → 进 Canvas → `NewTerminal` → PTY 输入 → grid 渲染整条链路。它既是 CI 主防线，也是 subagent 任务完成判定标准。

参考：实现计划 [`docs/exec-plans/2026-06-28-e2e-testing-infra.md`](../exec-plans/2026-06-28-e2e-testing-infra.md)；SPEC `docs/product-specs/2026-06-28-SPEC-e2e-testing.md`。

## 1. 两层模型

| 层 | 后端 | 用途 | CI 默认 |
| --- | --- | --- | --- |
| **L1 注入剧本** | `TestPtyBackend`（`terminal` crate）：`write(bytes)` 同步回吐预设剧本 / 回显 | CI 主防线，快、确定 | 跑 |
| **L2 真 PTY** | 真实 `portable_pty` 子进程（`/bin/zsh`） | 本地冒烟，覆盖真 PTY 异步链路 | 跳过（`#[ignore]`） |

L2 标 `#[ignore]`：真 PTY 输出依赖 host 时钟与 shell rc，CI 易抖动；本地手动跑。

## 2. 运行命令

```bash
# L1（默认，进 CI）
cargo test -p e2e

# L2（真 PTY 冒烟，本地）
cargo test -p e2e -- --ignored

# 全 workspace（含 L1 e2e + 全部 domain/app 单测；L2 自动跳过）
cargo test --workspace

# 单个 e2e 测试
cargo test -p e2e e2e_01
cargo test -p e2e e2e_02_terminal_input_echoes
```

`gpui` 的 `test-support` feature 仅在 `e2e` crate 局部启用，`cargo build -p app` 生产构建不受影响。

## 3. Agent 验收用法

subagent 完成涉及 Terminal / Canvas / 焦点路由 / PTY 链路的功能后，把相关 e2e 测试当作"功能做完了没"的自动判定：

1. 跑与改动相关的 e2e 测试（如改了 spawn 路径 → `e2e_01`；改了 input/drain → `e2e_02`）。
2. 绿 = 功能闭环；红 = 回到 [`superpowers:systematic-debugging`](../../.claude/skills) 排查，不要手动"看起来对了"就宣告完成。
3. 改动若影响真 PTY 路径，本地补跑 `cargo test -p e2e -- --ignored`。

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
| `echo_factory()` | echo PTY 工厂：`write(bytes)` → 回吐相同 bytes |
| `scripted_factory(rules)` | 剧本 PTY 工厂：`write` 含 `rule.trigger` → 投递 `rule.output` |

## 5. Mock 时钟与 drain timer（关键）

GPUI 的 `TestAppContext` 用 mock 时钟：`background_executor.timer(dur)` 返回的 future 仅在 `advance_clock(dur)` 后才就绪。`MultisoulApp` 的 12ms 周期 drain loop（`crates/app/src/app/mod.rs`）依赖该 timer 触发 `runtime.drain_all()`。

**仅 `run_until_parked` 不够**——mock 时钟不前进，drain 永不执行，PTY 输出停在 `TerminalSession` 的 rx 队列里，grid 不更新。

`drain_until` 每轮显式 `advance_clock(12ms)` 让 drain timer 就绪，再 `run_until_parked` 推进 executor 执行 drain 回调。任何依赖 timer 驱动输出的 e2e 断言必须经 `drain_until`，不能裸 `run_until_parked`。

## 6. 既有用例

| 测试 | 文件 | 覆盖 |
| --- | --- | --- |
| `smoke_headless_gpui_boots_to_launchpad` | `lib.rs` | headless boot → `Launchpad` |
| `harness_enters_canvas_and_spawns_terminal` | `harness.rs` | harness 自身可用性烟雾 |
| `e2e_01_*` | `e2e_01_create_terminal.rs` | `NewTerminal` → 节点数 + 焦点 + layout reflow 正/负断言矩阵 |
| `e2e_02_terminal_input_echoes_scripted_output_into_grid` | `e2e_02_terminal_echo.rs` | L1 剧本：`echo hi` → grid 含 `hi` |
| `e2e_02_real_pty_echo_hi_into_grid` | `e2e_02_real_pty.rs` | L2 真 PTY（`#[ignore]`）：真 zsh `echo hi` → grid 含 `hi` |

## 7. 新增 e2e 测试检查清单

1. 复用 `harness` helper，不要直接 `add_window_view` 重复装配逻辑。
2. PTY 链路用 `scripted_factory`（确定性）优先；只有覆盖真 PTY 异步路径才用 `make_app_real` 并标 `#[ignore]`。
3. 任何依赖 drain 的断言走 `drain_until(pred, timeout)`；timeout 注释推导（L1 ~500ms，L2 ~2000ms）。
4. 断言带失败信息字符串；正断言 + 负断言成对（"应含 X" + "不应含 Y"）。
5. 测试函数上方文档注释：场景 / 数据构造（含数值推导）/ 执行过程 / 预期结果。
