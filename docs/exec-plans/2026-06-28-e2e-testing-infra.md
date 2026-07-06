# E2E Testing Infrastructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立独立 `crates/e2e` 测试 crate，基于 GPUI 0.2.2 的 `test-support` feature 在 headless 上下文里驱动真实 `MultisoulApp` 状态机，落地两条端到端用例——E2E-01（Cmd+N 建节点 + 焦点路由 + layout reflow）与 E2E-02（Terminal 输入回显）。PTY 链路采用"注入剧本为主防线 + `#[ignore]` 真 PTY 冒烟"双轨（SPEC §3.3/§3.4）。产出既是 CI 防线，也是 subagent 完成任务的自动判定标准。

**Architecture:**
- **注入注入口（关键改动）**：在 `terminal` crate 抽 `PtyBackend` trait。生产实现 `PortablePtyBackend` 包装现有 `PtyHandle` 逻辑；测试实现 `TestPtyBackend` 按"被 write 时同步吐预编 ANSI 剧本"。`TerminalSession` 持有 `Box<dyn PtyBackend>`。注入工厂经 `Runtime`（新增 `pty_factory` 字段）→ `TermLive::spawn_with_backend` 向下穿透。`MultisoulApp::new` 生产路径用默认工厂，e2e 路径注入测试工厂。
- **e2e crate**：`crates/e2e`，dependencies 拉 `app / runtime / canvas / terminal / gpui(test-support)`。共享 helper 模块：建 `MultisoulApp` 视图、进入 Canvas、dispatch action、`run_until_parked`、读 runtime 状态、读 `GridSnapshot`。
- **进入 Canvas**：`MultisoulApp::new` 初始 screen 是 `Launchpad`；`on_new_terminal` 不 gating screen，但 `visible_canvas_rect` 依赖 window 布局。helper 用 `TestAppContext::simulate_new_path_selection` 走 `OpenProject` 进入 Canvas（或直接 `app.screen = AppScreen::Canvas` + 设 workspace root，取决于 `prompt_open_project` 实现）。

**Tech Stack:** Rust, GPUI 0.2.2（`test-support` feature / `TestAppContext` / `add_window_view` / `dispatch_action` / `simulate_keystrokes` / `run_until_parked` / `simulate_new_path_selection`），现有项目测试规范（`CLAUDE.md` §7 + `docs/quality/test-requirements.md`）。

参考 spec：`docs/product-specs/2026-06-28-SPEC-e2e-testing.md`。

## Global Constraints

- 单个 Rust 源文件 ≤500 行（`scripts/check-max-file-lines.sh`）；helper 按职责拆分。
- 禁止 `#[allow(...)]`（`scripts/check-no-allow.sh`）。
- 每个 `assert!`/`assert_eq!`/`assert_ne!` 必须带失败信息字符串。
- 每个测试函数上方文档注释：场景、数据构造（含关键数值推导）、执行过程、预期结果（正/负断言）。
- **domain crate 边界**：`PtyBackend` trait 与 `TestPtyBackend` 落在 `terminal` crate，不得引 GPUI。`runtime` 同理。`cargo tree -p canvas -p terminal -p runtime | grep -i gpui` 必须为空。
- **生产路径不变**：默认工厂走真实 `portable_pty`；现有 `terminal`/`runtime` 单测全绿。`on_new_terminal` 菜单/键盘路径行为不变。
- **GPUI pin**：`gpui =0.2.2`，开 `test-support` feature 不影响生产编译。
- 改动文件集：`crates/terminal/src/{pty.rs,session.rs,lib.rs}`、`crates/runtime/src/{runtime.rs,nodes/term.rs,lib.rs}`、`crates/app/src/app/mod.rs`（仅 `MultisoulApp::new` 注入）、新建 `crates/e2e/**`、`Cargo.toml`（workspace members）。
- 不触碰 `storage`/`workspace`/`canvas`（除读 layout 纯函数）。

## 入口与运行

- `cargo test -p e2e`：默认跑 L1（注入剧本）。
- `cargo test -p e2e -- --ignored`：跑 L2（真 PTY 冒烟）。
- `cargo test --workspace`：含 e2e L1，全绿。
- 验收矩阵见 SPEC §7。

---

### Task 1: `terminal` crate 抽 `PtyBackend` trait 与默认实现

**Files:**
- Modify: `crates/terminal/src/pty.rs`
- Modify: `crates/terminal/src/lib.rs`

**Why:** 现有 `PtyHandle` 是具体 struct，无注入点。先抽 trait，生产逻辑保持等价。

- [ ] 1.1 定义 trait `PtyBackend`：`fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError>`、`fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError>`、`fn kill(&mut self)`。trait 留在 `pty.rs`，无 GPUI 依赖。
- [ ] 1.2 把现有 `PtyHandle` 重命名为 `PortablePtyBackend`（或保留 `PtyHandle` 名 + `impl PtyBackend for PtyHandle`），`impl PtyBackend`。`spawn`/`spawn_command` 仍返回自身。
- [ ] 1.3 `lib.rs` 导出 `PtyBackend`。
- [ ] 1.4 保持现有 `pty.rs` 内 `test_spawn_echo_emits_output_and_exit` 通过（真 PTY 冒烟，作为本 task 回归门）。

**验证:** `cargo test -p terminal`（现有全绿，含 `test_spawn_echo_emits_output_and_exit`）。

---

### Task 2: `TestPtyBackend` 注入实现

**Files:**
- New: `crates/terminal/src/test_backend.rs`
- Modify: `crates/terminal/src/lib.rs`

**Why:** L1 主防线需要确定性 backend。落在 `terminal` crate（域层、可独立测）。

- [ ] 2.1 新建 `test_backend.rs`：`pub struct TestPtyBackend`，内含写接收端 + 预编剧本队列 + 一个 `Sender<PtyEvent>`（同步回吐）。`cfg(any(test, feature = "test-support"))` 闸控，或始终编译（更简单，体积可忽略）——选始终编译，保持 domain crate 干净无 feature。
- [ ] 2.2 `impl PtyBackend for TestPtyBackend`：`write` 把 bytes 推入剧本求值（按"收到 `echo X\n` → 吐 `X\r\n`"规则，或全量剧本表），通过 `tx.send(PtyEvent::Output(...))` 同步回吐。
- [ ] 2.3 构造器 `TestPtyBackend::scripted(scripts: Vec<(trigger, output)>)` + `tx` 注入；或 `TestPtyBackend::echo()`（回显模式：write 什么吐什么）。
- [ ] 2.4 单测落在 `test_backend.rs` 内 `#[cfg(test)]`：write `b"echo hi\r"` → 收到含 `hi` 的 `PtyEvent::Output`；遵循测试规范文档注释 + 正负断言。
- [ ] 2.5 `lib.rs` 导出 `TestPtyBackend`。

**验证:** `cargo test -p terminal`（含新 test_backend 单测）。

---

### Task 3: `TerminalSession` 接受 backend 注入

**Files:**
- Modify: `crates/terminal/src/session.rs`

**Why:** `TerminalSession` 当前直接 `PtyHandle::spawn_command`。需改成持有 `Box<dyn PtyBackend>` 并能从外部注入工厂。

- [ ] 3.1 `TerminalSession.pty: Box<dyn PtyBackend>`。新增字段类型替换。
- [ ] 3.2 保留 `spawn` / `spawn_command`（生产，内部用默认 `PtyHandle` 工厂）。
- [ ] 3.3 新增 `spawn_with_backend(id, cols, rows, backend: Box<dyn PtyBackend>, rx)`：用外部传入的 backend + 共享通道。`drain`/`write_input`/`write_bytes`/`resize`/`kill` 改走 trait method。
- [ ] 3.4 现有 session 单测（spawn sh 系）保持真 PTY 路径，全绿。
- [ ] 3.5 新增 session 注入单测：`spawn_with_backend` + `TestPtyBackend::echo()` → write `b"hi"` → drain → `GridSnapshot` 含 `hi`。文档注释 + 正负断言。

**验证:** `cargo test -p terminal`。

---

### Task 4: `runtime` 穿透 backend 工厂

**Files:**
- Modify: `crates/runtime/src/nodes/term.rs`
- Modify: `crates/runtime/src/runtime.rs`
- Modify: `crates/runtime/src/lib.rs`

**Why:** `Runtime::spawn_terminal` → `TermLive::spawn_command` → `TerminalSession`。需把工厂从 Runtime 透到 session。

- [ ] 4.1 定义工厂类型 `pub type PtyFactory = Box<dyn Fn(PtyCommand, u16, u16, Sender<PtyEvent>) -> Result<Box<dyn PtyBackend>, PtyError> + Send + Sync>`（放 `runtime` 顶层或 `nodes/term`）。
- [ ] 4.2 `TermLive::spawn_command` 保留；新增 `TermLive::spawn_with_factory(id, command, cols, rows, factory)`。
- [ ] 4.3 `Runtime` 新增 `pty_factory: PtyFactory` 字段，`Runtime::new()` 设默认工厂（调 `PtyHandle::spawn_command`）。`spawn_terminal` / `spawn_agent` 改用 `self.pty_factory`。
- [ ] 4.4 `Runtime::new_for_test(factory)` 或 `Runtime::set_pty_factory(factory)` setter，供 e2e 注入 `TestPtyBackend` 工厂。
- [ ] 4.5 现有 runtime 单测（spawn / drain / resize）保持真 PTY，全绿。
- [ ] 4.6 新增 runtime 注入单测：`new_for_test` + echo 工厂 → spawn_terminal → send `echo hi` → drain → `node_view` 的 `GridSnapshot` 含 `hi`。

**验证:** `cargo test -p runtime`；`cargo tree -p runtime | grep -i gpui` 为空。

---

### Task 5: `MultisoulApp::new` 接受可选工厂

**Files:**
- Modify: `crates/app/src/app/mod.rs`

**Why:** e2e 需在 `MultisoulApp::new` 时注入 backend 工厂，使 Cmd+N 走测试 backend。

- [ ] 5.1 `MultisoulApp::new(cx)` 保持生产默认（真 PTY 工厂）。
- [ ] 5.2 新增 `MultisoulApp::new_with_pty_factory(cx, factory)`：设 `runtime.set_pty_factory(factory)`，其余与 `new` 一致。生产入口 `main.rs` 不变。
- [ ] 5.3 确认 `on_new_terminal` 无需改动——它调 `self.runtime.spawn_terminal`，工厂已在 runtime 内。
- [ ] 5.4 `cargo check -p app` 通过；现有 app 模块内联测试不破。

**验证:** `cargo check -p app`；`cargo test -p app`（现有）。

---

### Task 6: 创建 `crates/e2e` crate 脚手架

**Files:**
- New: `crates/e2e/Cargo.toml`
- New: `crates/e2e/src/lib.rs`
- Modify: root `Cargo.toml`（workspace members + workspace deps 如需）

**Why:** 独立 crate 隔离 headless GPUI 启动，不污染现有快速单测。

- [ ] 6.1 `crates/e2e/Cargo.toml`：`[lib]`，`doctest=false`，dependencies = `app / runtime / canvas / terminal { path }` + `gpui = { workspace = true, features = ["test-support"] }` + `crossbeam-channel`。
- [ ] 6.2 root `Cargo.toml` `members` 加 `"crates/e2e"`。
- [ ] 6.3 `src/lib.rs`：声明 helper 子模块 + 一个 `smoke` 测试——只 `TestAppContext::single()` + `add_window_view(MultisoulApp::new)` + `run_until_parked` + 断言 `app.screen == Launchpad`。证明 headless 启动通。
- [ ] 6.4 确认 `test-support` feature 不破坏生产 `cargo build -p app`。

**验证:** `cargo test -p e2e`（smoke 绿）；`cargo build -p app`（生产未破）；`cargo test --workspace` 编译通过。

---

### Task 7: e2e helper 模块

**Files:**
- New: `crates/e2e/src/harness.rs`

**Why:** 共享驱动逻辑，供 E2E-01/02 与 L2 复用。控制在 500 行内。

- [ ] 7.1 `fn make_app(cx, factory)` → `Entity<MultisoulApp>`：`add_window_view` + `new_with_pty_factory`（factory 默认 echo 或参数传入）。
- [ ] 7.2 `fn enter_canvas(view, cx, path)`：用 `simulate_new_path_selection` 走 `OpenProject` 进入 Canvas；若 `prompt_open_project` 用文件选择器则配合 simulate，否则直接 `update` 设 `screen=Canvas` + workspace root。**实施时先验证 `prompt_open_project` 实际路径**（见 Task 8 调研，可能先于 7）。
- [ ] 7.3 `fn create_terminal(view, cx)`：`dispatch_action(NewTerminal)` + `run_until_parked`，返回 `NodeId`。
- [ ] 7.4 `fn read_runtime(view, cx, f)`：`view.read(cx, |app, _cx| f(&app.runtime))`。
- [ ] 7.5 `fn drain_until(view, cx, predicate, timeout)`：循环 `run_until_parked` + 读状态直到 predicate 或超时（L2 真 PTY 必需；L1 因同步注入收敛快）。
- [ ] 7.6 `fn read_grid(view, cx, node_id)` → `GridSnapshot`：经 `runtime.node_view(node_id).Term.grid`。
- [ ] 7.7 每个 helper 带文档注释；模块不超 500 行。

**验证:** `cargo test -p e2e`（编译通过；helper 由 E2E-01 间接验证）。

---

### Task 8: E2E-01 创建终端节点 + 焦点 + layout reflow

**Files:**
- New: `crates/e2e/src/e2e_01_create_terminal.rs`（或 `tests/` 集成测试 target）

**Why:** SPEC §4.2 首条用例，验证整个脚手架 + 注入 + 状态机 + layout。

- [ ] 8.1 **先调研**（不写测试前）：确认 `prompt_open_project` 实现（`crates/app/src/workspace/`），定 `enter_canvas` 方案；确认 `run_until_parked` 对 12ms 周期 timer 的推进行为（必要时多次 `run_until_parked`）。
- [ ] 8.2 用例：`make_app(echo_factory)` → `enter_canvas(tempdir)` → `create_terminal` → `drain_until` 收敛。
- [ ] 8.3 正断言：runtime `live_len()==1`；该节点 `node_view` 为 `Term` 且 focused（`runtime` focus state via `activate_spawned_node`）；layout 网格含 1 格含它（读 `compute_responsive_grid_layout` 或 app `last_reflow_window_size`/`master_layout`）。
- [ ] 8.4 负断言：无其他节点类型；focus 不为 None；layout 非空。
- [ ] 8.5 完整文档注释（场景/数据构造/执行过程/预期结果）+ 每个 assert 带失败信息。

**验证:** `cargo test -p e2e`（E2E-01 绿）。

---

### Task 9: E2E-02 终端输入回显（L1 注入）

**Files:**
- New: `crates/e2e/src/e2e_02_terminal_echo.rs`

**Why:** SPEC §4.3，覆盖 write → grid 回显（注入剧本）。

- [ ] 9.1 复用 E2E-01 前置拿到 1 个 focused Term 节点。
- [ ] 9.2 `runtime.send_node_input(node_id, NodeInput::Text("echo hi".into()))`（或 `dispatch_action` + `simulate_input`）→ backend 吐 `hi` → `drain_until` grid 含 `hi`。
- [ ] 9.3 正断言：`read_grid(node_id)` 含 `hi`。
- [ ] 9.4 负断言：grid 不含未发送字串（如 `bye`）。
- [ ] 9.5 完整文档注释 + 带失败信息的 assert。

**验证:** `cargo test -p e2e`（E2E-02 绿）。

---

### Task 10: L2 真 PTY 冒烟（`#[ignore]`）

**Files:**
- New: `crates/e2e/src/e2e_02_real_pty.rs`

**Why:** SPEC §3.4，覆盖真实 portable_pty 链路，隔离抖动。

- [ ] 10.1 与 E2E-02 同步骤但工厂换真实（`MultisoulApp::new` 默认），spawn 固定 `/bin/sh -c 'echo hi'` 或写 `echo hi\r` 到默认 shell。固定 `SHELL=/bin/sh` 环境规避 rc。
- [ ] 10.2 输出等待用 `drain_until(predicate, 500ms)` 超时轮询。
- [ ] 10.3 `#[ignore]` 标记。正负断言同 E2E-02。
- [ ] 10.4 完整文档注释说明"真 PTY 集成测试，CI 默认跳过，本地 `--ignored` 跑"。

**验证:** `cargo test -p e2e -- --ignored`（本地绿）；`cargo test -p e2e` 默认跳过它。

---

### Task 11: 文档与约束门

**Files:**
- New: `docs/references/e2e-testing-guide.md`（运行命令速查）
- 各 check 脚本

**Why:** 留下运行入口，过全部机械约束。

- [ ] 11.1 写 `docs/references/e2e-testing-guide.md`：如何跑 L1 / L2 / 进默认链路；agent 验收用法。
- [ ] 11.2 `bash scripts/check-max-file-lines.sh` 通过。
- [ ] 11.3 `bash scripts/check-no-allow.sh` 通过。
- [ ] 11.4 `cargo tree -p canvas -p terminal -p runtime | grep -i gpui` 为空。
- [ ] 11.5 `cargo test --workspace` 全绿 + `cargo check -p app` 通过。

**验证:** 全部约束脚本 + workspace 测试绿。

---

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| `run_until_parked` 对 12ms 周期 timer 推进不确定 | Task 8.1 先验证；必要时 helper 多次 `run_until_parked` |
| `prompt_open_project` 文件选择器在 headless 行为 | Task 8.1 调研；可能改用直接设 `screen=Canvas` + workspace root，绕选择器 |
| `test-support` feature 与生产编译冲突 | Task 6.4 验证 `cargo build -p app` 不破 |
| 注入工厂穿透改动面大、影响生产 spawn 路径 | Task 1–5 保持默认工厂=真实，逐 task 留现有真 PTY 单测作回归门 |
| 真 PTY 冒烟 CI 抖动 | L2 `#[ignore]` 隔离，不进默认链路 |
| `terminal`/`runtime` 体积膨胀过 500 行 | trait/backend 拆独立文件；helper 按职责拆 |

## 后续（不在本计划）

- E2E-03 多节点三列 tile、E2E-04 Esc、E2E-05 persistence 闭环、E2E-06 settings 录制恢复（SPEC §4.4 登记）。
- L3 真窗口视觉回归（SPEC §3.1 预留）。
