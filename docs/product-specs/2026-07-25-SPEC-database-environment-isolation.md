# 数据库生产与测试环境隔离 SPEC

> 日期：2026-07-25
> 状态：已实施（2026-07-26）
> 来源：采访式需求梳理（`interview-spec`）

## 1. 背景与目标

Norma OS 当前的正式 App、本地 `cargo run`、单元测试和 E2E 测试可能解析到相同的 SQLite 数据库路径，导致测试设置、最近项目、Canvas、Session 和 Agent 数据混入生产数据。

本次目标是通过简单、明确的运行模式判断和数据库路径切换，实现生产与测试数据库的物理隔离。

成功后的表现：

- 正式打包 App 只读写生产数据库。
- 本地 `cargo run` 只读写固定测试数据库。
- 单元测试和 E2E 使用临时数据库。
- 测试代码不能因路径解析失败而回退到生产数据库。
- 现有生产数据库路径和数据保持不变。

## 2. 范围

### 2.1 In Scope

隔离以下两个 SQLite 数据库：

1. 全局数据库 `app.sqlite`
   - Settings
   - Recent Workspaces
   - Orchestration Authorizations
2. 项目数据库 `project.sqlite`
   - Canvas Snapshot
   - Layout
   - Terminal/Agent Session
   - Voice History
   - Agent Definitions
   - Agent Dispatch Tasks
   - Agent Conversation Tails

覆盖以下运行方式：

- 正式打包的 Norma OS `.app`
- 本地 `cargo run`
- Rust 单元测试和集成测试
- `crates/e2e` E2E 测试

### 2.2 Out of Scope

本次不处理：

- 壁纸目录
- ASR/TTS 模型目录
- 浏览器缓存和登录状态
- Keychain 凭据
- License 文件
- 日志目录
- 已混入生产数据库的历史测试数据清理
- 数据库 Schema 调整
- Settings 页面或其他 UI

## 3. 运行模式与数据库路径

### 3.1 正式打包 App

正式打包的 Norma OS 使用现有生产路径。

全局数据库：

```text
~/Library/Application Support/Norma OS/app.sqlite
```

项目数据库：

```text
<workspace>/.multisoul/project.sqlite
```

本次不得迁移、重命名或复制生产数据库。

### 3.2 本地 `cargo run`

所有通过 `cargo run` 启动的 App 均视为测试环境，包括 `cargo run --release`。

全局数据库：

```text
~/Library/Application Support/Norma OS Test/app.sqlite
```

项目数据库：

```text
<workspace>/.multisoul/project.test.sqlite
```

测试数据库长期保留，以支持本地调试时跨启动保存设置、最近项目、Canvas 和 Session。

本地 `cargo run` 不提供切换到生产数据库的环境变量。只有正式打包 App 可以使用生产数据库。

### 3.3 单元测试与集成测试

每个需要 SQLite 的测试用例使用独立临时目录。

示意路径：

```text
$TMPDIR/norma-os-tests/<test-id>/app.sqlite
$TMPDIR/norma-os-tests/<test-id>/<workspace>/.multisoul/project.test.sqlite
```

要求：

- 不同测试用例不得共享数据库。
- 并行测试不得通过进程级环境变量争用同一路径。
- 同一个测试用例中的多个 App 实例可以显式复用同一个临时数据库，以验证持久化和恢复。
- 测试结束后临时目录由测试框架清理。

### 3.4 E2E 测试

E2E 与单元测试采用相同的临时数据库原则。

每个 E2E 用例：

- 创建独立临时数据根目录。
- 创建独立临时 workspace。
- 在同一用例内允许重启 App 并复用数据库。
- 不得使用用户真实的 Application Support 数据库。
- 不得在真实项目中创建或修改 `.multisoul/project.sqlite`。

## 4. 路径选择规则

数据库路径应在应用或测试实例构造前确定。

优先级如下：

1. 测试 Harness 显式提供临时数据库路径。
2. 本地 `cargo run` 使用固定测试数据库路径。
3. 正式打包 App 使用生产数据库路径。

具体如何识别“正式打包 App”和“本地运行”属于技术设计文档内容，但必须满足：

- 默认开发运行进入测试环境。
- 生产模式必须由正式打包流程明确产生。
- 不依赖调用方手动设置环境变量才能保证安全。
- 可以保留测试数据根目录环境变量，供独立 E2E Runner 等外部测试进程覆盖测试路径。
- 不允许通过普通环境变量让 `cargo run` 进入生产数据库。

## 5. 初始化与迁移

### 5.1 测试数据库初始化

测试数据库首次使用时从空库开始，通过现有 SQLite migration 创建表结构。

禁止：

- 从生产 `app.sqlite` 复制数据。
- 从生产 `project.sqlite` 复制数据。
- 从旧 `Multisoul` 生产目录迁移数据到测试目录。

### 5.2 生产数据库兼容

正式 App 继续使用现有生产路径和旧数据迁移逻辑。

本次不改变：

- 生产数据库文件位置。
- SQLite Schema。
- 旧 `Multisoul` 数据向 `Norma OS` 数据目录的兼容迁移。

### 5.3 历史混合数据

本次不自动识别或删除已经混入生产数据库的测试数据。

发布前建议人工备份：

```text
~/Library/Application Support/Norma OS/app.sqlite
<workspace>/.multisoul/project.sqlite
```

历史数据清理如有需要，应作为独立需求处理。

## 6. 错误处理

- 测试数据库目录创建失败时，启动或测试直接失败。
- 测试数据库打开失败时，不得回退生产数据库。
- HOME 或 Application Support 路径解析失败时，不得退回当前目录的 `app.sqlite`。
- 项目测试数据库打开失败时，不得回退 `.multisoul/project.sqlite`。
- 错误信息应包含当前运行模式和目标数据库路径。
- 日志不得输出数据库中的敏感设置内容。

## 7. 技术约束

- 路径切换逻辑应集中维护，避免 Theme、Voice、Wallpaper、Shortcuts、Recent Projects 等模块分别解析默认路径。
- `storage` crate 继续保持零 GPUI 依赖。
- 不改变 `app -> storage` 的依赖方向。
- 不在 Runtime drain、PTY 输出、输入路由、鼠标移动等热路径解析路径或打开 SQLite。
- 后台持久化任务必须使用启动时已经确定的数据库路径。
- 不通过给同一数据库表增加 `environment` 字段实现逻辑隔离。
- 不引入复杂的多环境配置系统。

## 8. 验收标准

### 8.1 正式 App

- 正式打包 App 继续读取现有生产 `app.sqlite`。
- 正式 App 打开项目后继续使用 `.multisoul/project.sqlite`。
- 升级后原生产设置、最近项目和项目状态仍然存在。

### 8.2 本地 `cargo run`

- `cargo run -p app` 使用 `Norma OS Test/app.sqlite`。
- `cargo run --release -p app` 仍使用测试数据库。
- 本地修改设置后重新执行 `cargo run`，测试设置能够恢复。
- 本地打开真实项目时只修改 `project.test.sqlite`。
- 本地运行不得修改生产 `app.sqlite` 和 `project.sqlite`。

### 8.3 单元测试和 E2E

- 每个测试用例获得不同的临时数据库路径。
- 两个并行测试不会互相看到数据。
- 同一测试内重启 App 后能够读取该测试之前写入的数据。
- 测试执行前后，生产数据库文件哈希或修改时间不变。
- 测试执行前后，真实项目的 `project.sqlite` 不变。
- 测试路径初始化失败时测试失败，而不是继续使用生产库。

### 8.4 回归验证

实现完成后至少运行：

```text
cargo test -p storage
cargo test -p app
cargo test -p e2e
cargo test --workspace
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
```

## 9. 已确认决策

- `cargo test`、E2E 和本地 `cargo run` 都属于测试环境。
- `cargo run` 使用固定、长期保留的测试数据库。
- 单元测试和 E2E 每个用例使用独立临时数据库。
- 同时隔离全局数据库和项目数据库。
- 测试数据库从空库开始，不复制生产数据。
- 不在本次清理现有生产数据库中的历史测试数据。
- `cargo run` 不允许通过环境变量切换到生产数据库。
- 本次只处理 SQLite，不扩展到其他应用数据。

## 10. 后续文档

对应技术设计见：

```text
docs/design-docs/2026-07-25-database-environment-isolation.md
```

技术设计进一步明确：

- 正式打包模式的可靠识别方式。
- 数据库路径解析函数放置位置。
- `AppStore`、`ProjectStore` 和 App 构造入口的调整边界。
- E2E 临时目录的持有方式。
- 如何防止测试环境回退生产路径。
- 自动化验收用例与分阶段实施顺序。
