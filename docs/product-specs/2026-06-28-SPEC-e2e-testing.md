# E2E Testing SPEC

> 日期：2026-06-28 · 状态：已确认，待实施
> 来源：采访式需求梳理（interview-spec）+ GPUI 0.2.2 源码可行性验证

## 1. 背景与目标

Multisoul Desktop 当前测试体系分两层：
- **域层单测**：`canvas / terminal / runtime / storage` 纯逻辑，`cargo test -p <crate>` 可独立跑。
- **App 层"模块内联测试"**：通过 `#[path = "../tests/unit/xxx_tests.rs"]` 挂进源码模块，`use super::*` 访问私有项，但**绝大多数只测纯函数/状态结构体**（如 `InteractionState`、`compute_responsive_grid_layout`、`workspace_left_rail_visible()`），**不真正启动 GPUI Application / 窗口 / drain loop**。

缺失的那一层：**"启动真实 app 状态机 → 模拟真实用户输入 → 断言运行时/视觉行为"**。最接近 e2e 的现有手段是 `app_chrome_tests` 里对渲染字符串的 `render_source.contains(...)` 快照断言，既不驱动输入，也不推进 drain loop。

**目标**：建立一套 e2e 测试体系，既能作为 **CI 自动化验收**，也能作为 **subagent 完成任务的自动判定标准**（"功能做完了没"由测试机器可判定为通过/失败，不靠人眼看窗口）。覆盖范围含真实 PTY 链路。

## 2. 关键可行性结论（已查证）

基于 `gpui =0.2.2`（`crates/app/Cargo.toml`）源码查证（`~/.cargo/registry/src/.../gpui-0.2.2/`）：

1. **GPUI 0.2.2 暴露完整 headless 测试 API**，由 `feature = "test-support"` 闸控。
   - `TestAppContext`（`src/app/test_context.rs`）：`single()` / `add_window` / `add_window_view` 建 headless 窗口视图；`update` / `read` 访问 `App`；`spawn` 起 background task；`run_until_parked` 推进事件循环到收敛。
   - 输入驱动：`dispatch_action<A>`（直接派发 action，如 `NewTerminal`）、`simulate_keystrokes` / `simulate_input` / `simulate_click` / `simulate_mouse_move`。
   - 断言支撑：`events` / `next_event` / `notifications` 订阅 entity 事件。
2. **headless 平台独立**：`src/platform/test/`（`platform.rs` / `window.rs` / `display.rs` / `dispatcher.rs`）是专用测试平台，test 模式下 `add_window` **不走真实 Metal/GPU**，用测试平台实现。意味着 headless 不依赖真实显卡窗口。
3. **`MultisoulApp::new(cx)` 构造耦合度低**（`crates/app/src/app/mod.rs:97`）：纯状态构造，不依赖 Metal、不强依赖真实 window；`Runtime::new()`、`AgentHookServer::start()`、`NormaServer::start()` 均可在测试上下文里建立；`runtime.drain_all()` 是可驱动方法，drain loop 由 12ms background timer 推进，`run_until_parked` 可推进它。

**结论**：headless + 真窗口视觉两层都能在同一个 `TestAppContext` 基础上做；headless 路线在本 pin 版本下可行，不只是假设。

## 3. 架构方案

### 3.1 两层 e2e，共享一个 `MultisoulApp` 测试入口

| 层 | 用途 | 窗口 | PTY | 截图 | 运行场景 |
|---|---|---|---|---|---|
| **L1 主防线** | CI + agent 验收 | headless | 注入剧本 | 无 | `cargo test -p e2e`（进默认链路） |
| **L2 真 PTY 冒烟** | 真链路回归 | headless | 真实 `portable_pty` | 无 | `cargo test -p e2e -- --ignored`（手动/夜间） |
| **L3 视觉回归**（可选/未来） | release 交付信心 | 真窗口 | 同 L1 | 截图对比 | 独立命令，本 SPEC 仅预留 |

L1 与 L2 共享驱动逻辑：把"发 write → 等 grid 出现 X"这类序列抽成共享 helper，区别只在 PTY 后端是注入还是真实。

### 3.2 测试组织

- 新建 **`crates/e2e`** 独立 crate：`[lib]` test target，`dev-dependencies`（实为 `dependencies`，因整个 crate 即测试）拉 `app`、`runtime`、`canvas`、`terminal`、`gpui = { workspace = true, features = ["test-support"] }`。
- 加入 workspace `members`。
- **不**复用现有 `#[path = "tests/unit/..."]` 模块内联模式——e2e 走独立 crate，与"模块内联测试"物理隔离，避免 headless GPUI 启动污染现有快速单测链路。
- `cargo test --workspace` 默认**包含** L1（快且确定）；L2 用 `#[ignore]` 隔离；L3 走独立 `[[test]]` target 或 feature，不入默认链路。

### 3.3 PTY 注入方案（L1 核心）

在 **`terminal` crate** 抽一个后端注入点（不落 app，遵守"domain crate 保持 UI-free、可独立测试"约束）：

- 在 `terminal::session` 或 `terminal::pty` 层定义 trait（例如 `PtyBackend`），生产实现走 `portable_pty`，测试实现 `TestPtyBackend` 按"被 write 时吐预编 ANSI 串"的剧本同步回放。
- `runtime::nodes::term` 通过依赖注入接收后端；`MultisoulApp::new` 生产路径用默认真实后端，e2e 路径注入 `TestPtyBackend`。
- 注入后端**同步**吐输出，使 `run_until_parked` 真正"推到收敛"，零 OS 调度抖动，agent 100% 稳定判定。

> 实现期需评估：现有 `PtyHandle` / `TerminalSession` 的构造签名改动面；尽量用 builder 或环境开关，不破坏生产 `on_new_terminal` 路径。

### 3.4 真 PTY 冒烟（L2）

- 同 L1 用例，仅后端换真实 `portable_pty`，spawn 固定 `SHELL=/bin/sh`（或 `/bin/sh -c '...'`）规避 zsh prompt/rc 污染。
- 输出等待用**带超时的轮询**（如 "500ms 内 grid 出现 hi"），不依赖 `run_until_parked` 的收敛语义。
- 全部打 `#[ignore]`，CI 默认不跑。

## 4. 验收标准（In Scope）

### 4.1 脚手架（基础设施验收）

- **A1**：`crates/e2e` crate 加入 workspace；`cargo test -p e2e` 可独立编译运行。
- **A2**：`terminal` crate 后端注入点落地，生产路径行为不变（现有 `terminal` 单测全绿、`cargo test -p terminal` 通过）。
- **A3**：共享 helper 模块成立：建 `MultisoulApp` 视图、dispatch action、`run_until_parked`、读 runtime 状态、读 `GridSnapshot`。

### 4.2 首条用例 E2E-01：Cmd+N 创建终端节点并完成焦点路由与 layout reflow（L1）

- **前置**：`TestAppContext` + 注入 `TestPtyBackend`；`cx.add_window_view(|_, cx| MultisoulApp::new(cx))` 建主视图。
- **步骤**：
  1. `cx.dispatch_action(window, NewTerminal)`。
  2. `cx.run_until_parked()`。
  3. 读 `MultisoulApp` runtime 状态。
- **正断言**：
  - 恰好存在 1 个 `Term` 节点。
  - 该节点处于 focused（runtime focus state 指向它）。
  - layout 已 reflow：网格含且仅含该节点 1 格。
- **负断言**：
  - 不存在其他类型节点（Agent / Preview 等）。
  - focus 不为 None。
  - layout 不为空。
- **不覆盖**：PTY 写入与回显（留给 E2E-02）。

### 4.3 E2E-02：终端输入回显（L1 注入 / L2 真 PTY）

- **前置**：E2E-01 已建立 1 个 focused Term 节点。
- **步骤**：
  1. 向该节点 PTY `write("echo hi\r")`。
  2. L1：`TestPtyBackend` 吐 `hi\r\n`；`cx.run_until_parked()`。L2：真实 shell，500ms 轮询。
  3. 读该节点 `GridSnapshot`。
- **正断言**：grid 中存在 `hi`。
- **负断言**：grid 中不存在 `bye`（或其它未发送字串，防误匹配）。
- **L2 标记** `#[ignore]`。

### 4.4 后续用例（本 SPEC 仅登记，不要求首批实现）

- E2E-03：多节点创建与三列 tile reflow（`at most three columns` 约束）。
- E2E-04：`Esc` 退出节点焦点。
- E2E-05：persistence 闭环——建节点 → SQLite 落库 → 重建 `MultisoulApp` → 节点/布局恢复。
- E2E-06：settings 快捷键录制恢复。

## 5. Out of Scope

- 视觉/截图回归（L3）：本 SPEC 仅预留入口，不实现。
- 不含 macOS accessibility（AXUIElement）驱动路线——`TestAppContext` 已提供等价 headless 输入，不重复造。
- 不覆盖 `voice` / `voice_sherpa` 真实模型链路（模型加载、麦克风）；语音状态机可单测覆盖即可。
- 不做跨平台（Linux/Win）e2e——GPUI/Metal 路线当前限定 macOS。
- 不重构现有"模块内联测试"模式，仅与之隔离共存。

## 6. 约束与风险

- **GPUI pin**：`gpui =0.2.2` 精确 pin。`test-support` feature 与现有 `cargo test --workspace` 兼容性需在 A1 落地时验证（确认开 feature 不影响生产编译）。
- **domain crate 约束**：后端注入点必须在 `terminal` / `runtime`，不得引入 GPUI 依赖；`cargo tree -p canvas -p terminal -p runtime | grep -i gpui` 应仍为空。
- **文件行数约束**：e2e crate 源文件 ≤500 行（`scripts/check-max-file-lines.sh`）；helper 按职责拆分。
- **测试规范**：遵循 `docs/quality/test-requirements.md`——每条 e2e 用例带场景描述/数据构造/执行过程/正负断言注释。
- **`#[allow(...)]` 禁用**：不压制诊断。
- **timing 风险（L2）**：真 PTY 输出等待只能用超时轮询，CI macOS runner shell 环境需固定；接受 L2 偶发抖动，故隔离为 `#[ignore]`。
- **drain loop 推进**：`run_until_parked` 推进 background executor 队列，但 12ms timer 是周期性 task——需确认 `run_until_parked` 对周期 timer 的推进行为与单次 task 一致；若不一致，e2e helper 需显式多次 `run_until_parked` 直到稳定。

## 7. 验收矩阵（实施期）

| 变更区 | 验证命令 |
|---|---|
| `terminal` 注入点 | `cargo test -p terminal`（现有全绿）|
| `runtime` 后端注入接线 | `cargo test -p runtime` |
| e2e crate 脚手架 | `cargo test -p e2e`（L1 全绿）|
| 真 PTY 冒烟 | `cargo test -p e2e -- --ignored`（L2 本地绿）|
| 全量回归 | `cargo test --workspace` + `cargo check -p app` |
| 边界 | `cargo tree -p canvas -p terminal -p runtime \| grep -i gpui`（空）|
| 行数 | `bash scripts/check-max-file-lines.sh` |
| 诊断 | `bash scripts/check-no-allow.sh` |

## 8. 后续步骤

本 SPEC 仅定方案与验收标准，不含实现。确认后进入：
1. `superpowers:writing-plans` 拆解实施计划（脚手架 → 注入点 → E2E-01 → E2E-02 → L2 冒烟）。
2. `superpowers:using-git-worktrees` + `subagent-driven-development` 逐 Task 执行（遵循 CLAUDE.md 默认开发模式）。
