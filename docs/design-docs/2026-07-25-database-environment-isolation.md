# 数据库生产与测试环境隔离设计

> 日期：2026-07-25
> 状态：已实施（2026-07-26）
> 对应 SPEC：[`../product-specs/2026-07-25-SPEC-database-environment-isolation.md`](../product-specs/2026-07-25-SPEC-database-environment-isolation.md)

## 1. 决策

Norma OS 只保留两种数据库模式：

- `Production`：仅正式打包 App 使用。
- `Test`：本地 `cargo run`、单元测试和 E2E 使用。

不建立通用的多环境配置系统。实现只需要：

1. 在 App 启动前决定数据库模式。
2. 集中解析一次全局数据库路径和项目数据库文件名。
3. 将解析结果传给所有需要持久化的 App 状态。
4. 允许自动化测试显式注入临时路径。

生产与测试使用不同的物理 SQLite 文件，不在表中增加环境字段。

## 2. 当前问题

当前全局数据库默认路径由 `storage::default_app_db_path()` 直接从 HOME 解析：

```text
~/Library/Application Support/Norma OS/app.sqlite
```

`AppStore::open_default()` 还会检查并复制旧 `Multisoul/app.sqlite`。这个行为对生产兼容有价值，但测试环境不应触发。

项目数据库由 `ProjectStore::for_workspace()` 固定解析为：

```text
<workspace>/.multisoul/project.sqlite
```

App 构造阶段的 Theme、Voice、Wallpaper、Shortcuts 和 Recent Projects 分别调用默认加载入口。E2E 构造器在生产默认加载之后才替换部分状态，因此即使测试后来注入临时 Store，启动阶段仍可能访问生产数据库。

另一个风险是 `WorkspaceState` 初始化默认 Store 失败后会退回当前目录下的 `app.sqlite`。隔离实现必须删除这种路径兜底。

## 3. 最小运行模式选择

### 3.1 正式打包标记

为 `app` crate 增加仅发布打包流程启用的编译期标记，例如：

```text
production-data
```

`Packager.toml` 的 `before-packaging-command` 必须显式使用该标记构建主 App：

```text
cargo build --release -p app --features production-data
```

因此：

- `cargo run -p app` 没有该标记，进入 `Test`。
- `cargo run --release -p app` 没有该标记，仍进入 `Test`。
- `cargo packager` 通过固定打包命令产生 `Production` App。

不使用 `debug_assertions` 判断模式，因为它无法区分本地 release build 和正式打包。

不通过运行时环境变量开启 `Production`，避免本地命令误写生产数据库。

### 3.2 自动化测试覆盖

单元测试和 E2E 不依赖进程级环境变量选择数据库。测试 Harness 直接传入临时数据库路径。

只有独立进程运行的 L3/E2E Runner 可以使用测试根目录覆盖变量，例如：

```text
NORMA_TEST_DATA_ROOT
```

该变量只能改变 `Test` 路径，不能切换到 `Production`。

## 4. 路径模型

实现一个小型、不可变的路径值对象。名称可在实现阶段确定，本文使用 `DatabasePaths`：

```text
DatabasePaths
├── mode
├── app_db_path
└── project_db_filename
```

它不是服务容器，也不管理数据库连接，只保存已经解析完成的路径选择。

### 4.1 Production

```text
mode                = Production
app_db_path         = ~/Library/Application Support/Norma OS/app.sqlite
project_db_filename = project.sqlite
```

生产模式允许执行现有的旧目录兼容迁移。

### 4.2 本地固定 Test

```text
mode                = Test
app_db_path         = ~/Library/Application Support/Norma OS Test/app.sqlite
project_db_filename = project.test.sqlite
```

本地 Test 模式不检查、不复制旧 `Multisoul` 生产数据库。

### 4.3 自动化测试

测试 Harness 显式构造：

```text
mode                = Test
app_db_path         = <temp-root>/app.sqlite
project_db_filename = project.test.sqlite
```

workspace 本身也必须来自该测试用例的临时目录，因此最终项目库位于：

```text
<temp-workspace>/.multisoul/project.test.sqlite
```

## 5. 组件边界

### 5.1 `storage` crate

`storage` 继续只负责：

- 根据显式路径打开 SQLite。
- 执行现有 migration。
- 提供生产和本地测试默认路径的纯解析函数。
- 根据显式项目数据库文件名构造 `ProjectStore`。

`storage` 不负责判断当前是否为正式打包 App，因为该信息属于 App 组合根。

生产旧库迁移入口和普通路径打开入口应分开：

- 生产入口可以从旧 `Multisoul` 目录复制数据库。
- Test 入口只能在目标测试路径创建空库。

### 5.2 App 组合根

主 App 启动顺序调整为：

```text
main / test harness
        |
        v
选择 DatabasePaths
        |
        v
构造 AppStore / RecentStore / SettingsStore
        |
        v
MultisoulApp::build
        |
        v
Theme / Voice / Wallpaper / Shortcuts / Workspace
```

关键要求是 `MultisoulApp::build` 不得先调用生产默认加载，再由测试覆盖。

### 5.3 App 状态加载

以下模块应改为接收已经选定的 Store 或数据库路径：

| 模块 | 当前问题 | 调整方向 |
|---|---|---|
| Theme | 自行 `AppStore::open_default()` | 接收共享 `SettingsStore` |
| Voice | 自行 `AppStore::open_default()` | 接收共享 `SettingsStore` |
| Wallpaper Settings | 单独读取测试环境变量 | 接收共享 `SettingsStore` |
| Shortcuts | 自行打开默认 AppStore | 接收共享 `SettingsStore` |
| Recent Projects | `RecentStore::open_default()` | 接收共享 `RecentStore` |
| Orchestration Authorization | 直接解析默认 app DB | 使用已选定的 app DB 路径 |
| Workspace Persistence | 固定 `ProjectStore::for_workspace()` | 使用 `DatabasePaths` 解析项目 Store |

Wallpaper 文件目录本身不在本次范围内；只有存储在 `app.sqlite` 中的 Wallpaper Settings 随数据库模式隔离。

### 5.4 项目 Store

所有生产和测试项目 Store 必须通过同一个解析入口创建：

```text
project_db_path(workspace)
```

其结果只由 `project_db_filename` 决定：

- Production → `.multisoul/project.sqlite`
- Test → `.multisoul/project.test.sqlite`

Workspace flow、PersistenceScheduler、Agent 设置和会话恢复不得绕过该入口重新调用固定生产路径。

## 6. 测试临时目录生命周期

### 6.1 单元测试

Store 单元测试继续由每个用例创建自己的 `tempfile::TempDir`，并把路径显式传给 Store。

禁止为了方便在并行单元测试中设置全局 `NORMA_TEST_DATA_ROOT`。

### 6.2 E2E

E2E Harness 为每个测试用例持有一个临时目录 Guard：

```text
E2e database root
├── app.sqlite
└── workspace/
    └── .multisoul/project.test.sqlite
```

Guard 的生命周期必须覆盖：

- `MultisoulApp` 构造。
- 后台持久化任务完成。
- 同一用例内的 App 重建。
- 最终断言。

需要验证重新加载的用例应让两个 App 实例共享同一个 Guard；不同测试用例不得共享。

## 7. 错误与失败封闭

### 7.1 禁止回退

以下回退全部禁止：

- Test app DB 打不开 → Production app DB。
- `project.test.sqlite` 打不开 → `project.sqlite`。
- HOME 解析失败 → 当前工作目录 `app.sqlite`。
- 临时目录丢失 → 固定本地测试库。

路径或数据库初始化失败时，返回带模式和目标路径的错误。

### 7.2 状态降级

若某个非关键设置读取失败，App 可以使用内存默认值继续启动，但后续保存必须：

- 保持禁用，或
- 继续使用原先解析出的同一测试路径。

不得构造新的相对路径 Store 作为兜底。

### 7.3 后台写入

后台任务必须捕获已经解析完成的 Store 或路径，不得在线程内部再次调用默认路径函数。

这同时避免路径模式在任务执行期间发生变化。

## 8. 迁移行为

### 8.1 Production

保留当前兼容行为：

```text
Multisoul/app.sqlite
        |
        | 当前生产库不存在时复制
        v
Norma OS/app.sqlite
```

迁移不删除旧文件。

### 8.2 Test

Test 永远从目标路径的空库开始：

```text
Norma OS Test/app.sqlite
```

不存在时直接运行现有 Schema migration，禁止读取生产库作为种子。

`project.test.sqlite` 同理，不从 `project.sqlite` 复制。

## 9. 自动化验证设计

所有新增测试必须遵守项目的测试文档注释、数值推导、正负断言和断言说明规范。

### 9.1 路径解析单元测试

至少覆盖：

- Production app DB 等于现有 `Norma OS/app.sqlite`。
- Test app DB 等于 `Norma OS Test/app.sqlite`。
- Production 项目文件名是 `project.sqlite`。
- Test 项目文件名是 `project.test.sqlite`。
- Production 和 Test 路径明确不相等。
- Test 路径解析不检查旧 `Multisoul` 数据库。

### 9.2 App 构造测试

建立两个带哨兵值的数据库：

- Production settings 包含 `production-sentinel`。
- Test settings 包含 `test-sentinel`。

以 Test 路径构造 App 后：

- 必须读到 `test-sentinel`。
- 不得读到 `production-sentinel`。
- App 初始化期间 Production 文件修改时间不得变化。

### 9.3 项目持久化测试

同一个临时 workspace 同时放置：

- `project.sqlite`，包含生产哨兵。
- `project.test.sqlite`，包含测试哨兵。

Test App 打开 workspace 后：

- 必须恢复测试哨兵。
- 不得恢复生产哨兵。
- 保存后只能改变 `project.test.sqlite`。

### 9.4 E2E 隔离测试

至少覆盖：

- 两个测试用例获得不同 app DB 路径。
- 同一用例两个 App 实例共享临时数据库并成功恢复。
- E2E 运行前后生产数据库哈希不变。
- 临时根目录不可写时测试明确失败。
- 失败场景中不创建当前目录 `app.sqlite`。

### 9.5 打包验证

发布构建需要检查：

- `Packager.toml` 的主 App 构建启用了正式标记。
- 裸 `cargo run` 未启用正式标记。
- `cargo run --release` 仍解析到 Test。
- 打包 `.app` 解析到 Production。

## 10. 实施顺序

遵循 TDD，按以下顺序实施：

1. 为 Production/Test 全局和项目路径补失败测试。
2. 实现最小 `DatabasePaths` 路径解析。
3. 拆分生产旧库迁移与 Test 空库初始化。
4. 让 App 构造入口接收已经解析的路径和 Store。
5. 迁移 Theme、Voice、Wallpaper Settings、Shortcuts 和 Recent Projects。
6. 迁移 Orchestration Authorization 和项目持久化调用点。
7. 移除当前目录 `app.sqlite` 兜底。
8. 改造 E2E Harness，使每个用例持有临时数据库 Guard。
9. 接入正式打包标记。
10. 运行局部、E2E、全仓和存储边界验证。

## 11. 验证命令

```bash
cargo test -p storage
cargo test -p app
cargo test -p e2e
cargo test --workspace
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

正式打包路径还应验证：

```bash
cargo build -p app --release
cargo packager --release
```

其中裸 `cargo build -p app --release` 仍属于 Test；只有 Packager 配置中启用正式标记的构建产物属于 Production。

## 12. 权衡

### 选择固定本地测试库

优点：

- `cargo run` 跨启动保留调试状态。
- 与生产数据物理隔离。
- 路径直观，容易排障。

代价：

- 本地测试数据会长期累积，需要开发者按需手工删除。

### 选择 `project.test.sqlite`

优点：

- 实现简单。
- 不需要建立 workspace 哈希和集中项目索引。
- 正式 App 与本地开发可打开同一目录而不共享项目状态。

代价：

- 真实 workspace 内会出现额外的测试数据库文件。
- 如项目需要保持工作区整洁，应确保 `.multisoul` 已被版本控制忽略。

### 不清理历史混合数据

优点：

- 不会因来源不可判定而误删用户数据。
- 本次变更保持小范围。

代价：

- 当前生产库中已经存在的测试痕迹不会自动消失。

## 13. 明确不做

- 不建立 `Production / Development / Staging / Test` 四层配置。
- 不允许运行时环境变量切换到 Production。
- 不把 Production 和 Test 放进同一数据库。
- 不自动复制生产数据到测试库。
- 不清理历史数据库。
- 不扩展到数据库以外的数据目录。

## 14. 实施结果

实现已按本设计落地：

- `storage::DatabasePaths` 集中表达 `Production` 与 `Test` 的全局、项目数据库路径。
- 默认本地构建使用固定 Test 数据库，打包命令通过 `production-data` 特性选择 Production。
- App 组合根只初始化一次显式 `AppStore`，Theme、Voice、Wallpaper、Shortcuts、
  Recent Projects 与后台授权写入均复用同一数据库选择。
- Workspace 持久化统一通过 `DatabasePaths` 解析 `project.sqlite` 或
  `project.test.sqlite`，不再回退生产路径或当前目录 `app.sqlite`。
- 单元测试和 E2E Harness 持有独立临时数据库 Guard；需要重建 App 的用例可以显式
  复用同一个临时 Store。
- 初始化错误包含数据库模式和目标路径；Test 初始化不会复制旧生产数据库。
- 外部 `NORMA_TEST_DATA_ROOT` 会拒绝直接、符号链接或 `..` 解析后与 Production
  `app.sqlite` 碰撞的路径。
- E2E 验证 Production app/project 哨兵字节不变，并用独立子进程守住不可写根失败
  与当前目录无 `app.sqlite` fallback。
